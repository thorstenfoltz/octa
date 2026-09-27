//! Filling the workspace: open tabs, attached databases, cloud files, picker actions.

use super::*;

impl OctaApp {
    /// Make every other open tab queryable from the active tab's SQL panel,
    /// under a SQL-safe version of its own name.
    ///
    /// Before this, joining two open files meant picking the second one again
    /// through a *file* dialog, which re-read it from disk and so could not
    /// see that tab's unsaved edits - and a tab with no file behind it could
    /// not be reached at all. The MCP surface has resolved open tabs by name
    /// since it was written (`run_sql`'s `extra_tables`); this is the same
    /// capability, wired to the panel people actually use.
    ///
    /// Not done for tabs backed by a live database: on the server their
    /// sibling tables already join by their real names, so copying them into
    /// DuckDB would be slower and less correct. Not done for tabs above
    /// `sql_auto_register_max_rows` either - registration copies every row.
    pub(super) fn sync_open_tabs_into_workspace(&mut self) {
        if !self.settings.sql_auto_register_open_tabs {
            return;
        }
        let max_rows = self.settings.sql_auto_register_max_rows;
        let active = self.active_tab;
        // Rows and titles both matter: a tab that grew past the ceiling, or
        // was renamed, has to be re-registered under the right name.
        //
        // The ACTIVE tab is excluded from the signature as well as from the
        // registration below. It is the one tab that streams rows in as the
        // user scrolls, so including its row count would change the
        // signature every frame and re-copy every other tab into DuckDB with
        // it.
        let sig: String = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != active)
            .map(|(i, t)| format!("{i}:{}:{}|", t.title_display(), t.table.row_count()))
            .collect();
        if self.tabs[active].sql_auto_register_sig == sig {
            return;
        }
        let Some(mut ws) = self.tabs[active].sql_workspace.take() else {
            return;
        };
        // Drop what a previous pass registered: a closed or renamed tab must
        // not linger as a queryable name pointing at stale rows.
        for name in std::mem::take(&mut self.tabs[active].sql_auto_registered) {
            let _ = ws.remove_table(&name);
        }
        let mut taken: std::collections::HashSet<String> = ws
            .list_tables()
            .iter()
            .map(|t| t.sql_name.clone())
            .collect();
        taken.insert("data".to_string());
        let mut registered: Vec<String> = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == active
                || tab.large.is_some()
                || tab.db_origin.is_some()
                || tab.table.col_count() == 0
                || tab.table.row_count() > max_rows
            {
                continue;
            }
            let base = octa::sql::sanitize_sql_name(&tab.title_display());
            let name = octa::sql::dedupe_sql_name(&base, |s| taken.contains(s));
            let origin = octa::sql::TableOrigin::TabClone(tab.title_display());
            // Clone only when there are edits to apply: `apply_edits` needs an
            // owned table, and copying a large one for nothing is the cost
            // this whole function is trying to keep in hand.
            let added = if tab.table.edits.is_empty() {
                ws.add_table(&name, &tab.table, origin)
            } else {
                let mut snapshot = tab.table.clone();
                snapshot.apply_edits();
                ws.add_table(&name, &snapshot, origin)
            };
            if added.is_ok() {
                taken.insert(name.clone());
                registered.push(name);
            }
        }
        let tab = &mut self.tabs[active];
        tab.sql_workspace = Some(ws);
        tab.sql_auto_register_sig = sig;
        tab.sql_auto_registered = registered;
    }

    /// ATTACH a saved live-database connection into the active tab's SQL
    /// workspace. Synchronous: the DuckDB extension handshake runs on the UI
    /// thread, like the file ATTACH beside it.
    // ponytail: blocks the UI for the handshake (and the whole import for
    // SQL Server); move onto a worker if users attach slow servers.
    pub(super) fn workspace_attach_db_connection(
        &mut self,
        conn_id: &str,
        scope: &octa::sql::AttachScope,
    ) {
        let Some(conn) = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == conn_id)
            .cloned()
        else {
            return;
        };
        let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &self.settings);
        let ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &self.settings);
        let tab = &mut self.tabs[self.active_tab];
        ensure_workspace(tab);
        // Only a native ATTACH gets an alias, and it is always the whole
        // server (the menu hands those an empty scope), so the connection name
        // is the alias. An import registers plain workspace tables and names
        // them after themselves.
        let base = octa::sql::sanitize_sql_name(&conn.name);
        let ws_imm = tab.sql_workspace.as_ref().expect("ensured");
        let existing_aliases: std::collections::HashSet<String> = ws_imm
            .list_attached()
            .iter()
            .map(|a| a.alias.clone())
            .collect();
        let alias = octa::sql::dedupe_sql_name(&base, |s| existing_aliases.contains(s));
        let ws = tab.sql_workspace.as_mut().expect("ensured");
        match ws.attach_db(
            &conn,
            secret.as_deref(),
            ssh_secret.as_deref(),
            &alias,
            scope,
        ) {
            Ok(octa::sql::AttachOutcome::Attached(_)) => {
                tab.sql_workspace_open = true;
                self.status_message = Some((
                    format!("Attached `{alias}` to SQL workspace"),
                    std::time::Instant::now(),
                ));
            }
            Ok(octa::sql::AttachOutcome::Imported(names)) => {
                tab.sql_workspace_open = true;
                self.status_message = Some((
                    format!("Added to SQL workspace: {}", names.join(", ")),
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                // `{e:#}`, not `{e}`: the outermost context alone would say
                // "ATTACHing ..." and drop the server's own reason.
                tab.sql_error = Some(format!("{e:#}"));
            }
        }
        Self::prune_inspector_cache(&mut self.tabs[self.active_tab]);
    }

    /// Download the objects picked in the cloud picker, then hand them to
    /// `workspace_add_cloud_files` on the main thread.
    ///
    /// One worker for the batch, like the sidebar's Union: the network calls
    /// must not run on the interface thread, and workers must not touch tabs.
    pub(crate) fn start_cloud_workspace_fetch(
        &mut self,
        ctx: &egui::Context,
        conn_id: &str,
        picked: Vec<(String, String)>,
        combine: bool,
    ) {
        let Some(conn) = self.find_cloud_conn(conn_id) else {
            return;
        };
        if picked.is_empty() {
            return;
        }
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.union_progress = Some(crate::app::state::UnionProgress::new(
            &octa::i18n::t("cloud.union_downloading"),
            picked.len(),
        ));
        let progress = self
            .union_progress
            .as_ref()
            .map(|p| p.done.clone())
            .unwrap_or_default();
        std::thread::spawn(move || {
            let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
            let mut skipped = 0usize;
            for (key, name) in &picked {
                match crate::app::cloud_browser::fetch_object_to_temp(&conn, key, name, &settings) {
                    Ok(path) => {
                        let stem = std::path::Path::new(name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "table".to_string());
                        files.push((path, octa::sql::sanitize_sql_name(&stem)));
                    }
                    Err(_) => skipped += 1,
                }
                progress.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            if let Ok(mut p) = pending.lock() {
                p.push(crate::app::cloud_browser::CloudOpenResult::WorkspaceReady {
                    files,
                    combine,
                    skipped,
                });
            }
            ctx.request_repaint();
        });
    }

    /// Register downloaded cloud objects as workspace tables on the active tab.
    ///
    /// `combine` unions them into one table first. It is the picker's opt-in
    /// box, never a default: a union reconciles differing schemas, and doing
    /// that unasked because two files were ticked would change the data the
    /// user gets back.
    pub(crate) fn workspace_add_cloud_files(
        &mut self,
        files: Vec<(std::path::PathBuf, String)>,
        combine: bool,
        skipped: usize,
    ) {
        if files.is_empty() {
            self.tabs[self.active_tab].sql_error = Some(octa::i18n::t("sql.cloud_pick_all_failed"));
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        ensure_workspace(tab);
        let mut added: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        if combine {
            match Self::union_cloud_files(&files) {
                Ok(table) => {
                    let name = Self::free_ws_name(tab, &files[0].1);
                    let ws = tab.sql_workspace.as_mut().expect("ensured");
                    match ws.add_table(
                        &name,
                        &table,
                        TableOrigin::Db(octa::i18n::t("sql.cloud_pick_origin")),
                    ) {
                        Ok(_) => added.push(name),
                        Err(e) => errors.push(format!("{e:#}")),
                    }
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
        } else {
            for (path, stem) in &files {
                let name = Self::free_ws_name(tab, stem);
                let ws = tab.sql_workspace.as_mut().expect("ensured");
                match ws.add_table_from_file(path, None, &name) {
                    Ok(_) => added.push(name),
                    Err(e) => errors.push(format!("{}: {e:#}", path.display())),
                }
            }
        }
        tab.sql_workspace_open = true;
        if !errors.is_empty() {
            tab.sql_error = Some(errors.join("\n"));
        }
        let mut msg = octa::i18n::t("sql.cloud_pick_added")
            .replace("{n}", &added.len().to_string())
            .replace("{names}", &added.join(", "));
        if skipped > 0 {
            msg.push(' ');
            msg.push_str(
                &octa::i18n::t("sql.cloud_pick_skipped").replace("{n}", &skipped.to_string()),
            );
        }
        self.status_message = Some((msg, std::time::Instant::now()));
        Self::prune_inspector_cache(&mut self.tabs[self.active_tab]);
    }

    /// Read every downloaded file and union them, so `combine` produces one
    /// table. Uses the same planner the Union dialog does, so a column present
    /// in one file and missing in another comes back null rather than shifting
    /// the row.
    pub(super) fn union_cloud_files(
        files: &[(std::path::PathBuf, String)],
    ) -> anyhow::Result<octa::data::DataTable> {
        let registry = octa::formats::FormatRegistry::new();
        let mut tables: Vec<octa::data::DataTable> = Vec::new();
        for (path, _) in files {
            let reader = registry
                .reader_for_path(path)
                .ok_or_else(|| anyhow::anyhow!("no reader for {}", path.display()))?;
            tables.push(reader.read_file(path)?);
        }
        let schemas: Vec<&[octa::data::ColumnInfo]> =
            tables.iter().map(|t| t.columns.as_slice()).collect();
        let plan = octa::data::union::plan_union(&schemas, true);
        let refs: Vec<&octa::data::DataTable> = tables.iter().collect();
        octa::data::union::union_tables(&refs, &plan)
    }

    /// A workspace name based on `stem` that nothing is registered under yet.
    pub(super) fn free_ws_name(tab: &TabState, stem: &str) -> String {
        let ws = tab.sql_workspace.as_ref().expect("ensured");
        let existing: std::collections::HashSet<String> = ws
            .list_tables()
            .iter()
            .map(|t| t.sql_name.clone())
            .collect();
        octa::sql::dedupe_sql_name(&octa::sql::sanitize_sql_name(stem), |s| {
            existing.contains(s)
        })
    }

    pub(super) fn workspace_add_tables_via_picker(&mut self) {
        let paths = match rfd::FileDialog::new()
            .set_title("Add tables to SQL workspace")
            .pick_files()
        {
            Some(ps) if !ps.is_empty() => ps,
            _ => return,
        };
        let tab = &mut self.tabs[self.active_tab];
        ensure_workspace(tab);
        let mut errors: Vec<String> = Vec::new();
        let mut added: Vec<String> = Vec::new();
        for path in paths {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "table".to_string());
            let base = octa::sql::sanitize_sql_name(&stem);
            let ws = tab.sql_workspace.as_ref().expect("ensured");
            let existing: std::collections::HashSet<String> = ws
                .list_tables()
                .iter()
                .map(|t| t.sql_name.clone())
                .collect();
            let name = octa::sql::dedupe_sql_name(&base, |s| existing.contains(s));
            let ws = tab.sql_workspace.as_mut().expect("ensured");
            match ws.add_table_from_file(&path, None, &name) {
                Ok(_) => added.push(name),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        tab.sql_workspace_open = true;
        Self::prune_inspector_cache(tab);
        if !added.is_empty() {
            self.status_message = Some((
                format!("Added to SQL workspace: {}", added.join(", ")),
                std::time::Instant::now(),
            ));
        }
        if !errors.is_empty() {
            self.tabs[self.active_tab].sql_error = Some(errors.join("\n"));
        }
    }

    pub(super) fn workspace_attach_db_via_picker(&mut self) {
        let path = match rfd::FileDialog::new()
            .set_title("Attach database to SQL workspace")
            .add_filter("DuckDB / SQLite", &["duckdb", "ddb", "sqlite", "db"])
            .pick_file()
        {
            Some(p) => p,
            None => return,
        };
        let tab = &mut self.tabs[self.active_tab];
        ensure_workspace(tab);
        let kind = AttachKind::from_path(&path);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "db".to_string());
        let base = octa::sql::sanitize_sql_name(&stem);
        let ws_imm = tab.sql_workspace.as_ref().expect("ensured");
        let existing_aliases: std::collections::HashSet<String> = ws_imm
            .list_attached()
            .iter()
            .map(|a| a.alias.clone())
            .collect();
        let alias = octa::sql::dedupe_sql_name(&base, |s| existing_aliases.contains(s));
        let ws = tab.sql_workspace.as_mut().expect("ensured");
        match ws.attach(&path, &alias, kind) {
            Ok(att) => {
                tab.sql_workspace_open = true;
                let label = if att.native { "" } else { " (fallback)" };
                self.status_message = Some((
                    format!("Attached `{alias}` to SQL workspace{label}"),
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                tab.sql_error = Some(e.to_string());
            }
        }
        Self::prune_inspector_cache(tab);
    }

    pub(super) fn workspace_remove_table(&mut self, sql_name: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.remove_table(sql_name)
        {
            tab.sql_error = Some(e.to_string());
        }
        Self::prune_inspector_cache(tab);
    }

    pub(super) fn workspace_rename_table(&mut self, from: &str, to: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.rename_table(from, to)
        {
            // `{e:#}`: the reason (name taken, empty) is the inner context.
            tab.sql_error = Some(format!("{e:#}"));
        }
        Self::prune_inspector_cache(tab);
    }

    pub(super) fn workspace_detach(&mut self, alias: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.detach(alias)
        {
            tab.sql_error = Some(e.to_string());
        }
        Self::prune_inspector_cache(tab);
    }
}
