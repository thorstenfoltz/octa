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
        // A copy still running for an older set of tabs is stopped: its
        // feeds go with it, and the worker leaves what it had not finished.
        self.tabs[active].sql_other_tabs = None;
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
        // Copying the rows is a job: the window hands each tab over a slice
        // per frame (`drive_other_tab_copies`), a worker writes them.
        let mut feeds = Vec::new();
        let mut copies = Vec::new();
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
            taken.insert(name.clone());
            // The copy reads cells through `DataTable::get`, edits included.
            let rows = crate::app::slice_feed::slice_rows(tab.table.col_count());
            let (feed, rx) = crate::app::slice_feed::SliceFeed::new(&tab.table, None, None, rows);
            feeds.push((
                name.clone(),
                octa::sql::DataFeed {
                    columns: tab.table.columns.clone(),
                    rows: rx,
                },
            ));
            copies.push(OtherTabFeed {
                tab: i,
                title: tab.title_display(),
                stamp: crate::app::slice_feed::RowsStamp::of(&tab.table),
                feed,
            });
        }
        let task = (!feeds.is_empty())
            .then(|| crate::app::pushdown::spawn_local_task(ws.copy_tables_job(feeds)));
        let tab = &mut self.tabs[active];
        tab.sql_workspace = Some(ws);
        tab.sql_auto_register_sig = sig;
        tab.sql_other_tabs = task.map(|task| OtherTabsCopy { copies, task });
    }

    /// Once per frame: feed every copy of other tabs into a workspace, and
    /// list the tables of every copy that finished.
    pub(crate) fn drive_other_tab_copies(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let Some(mut job) = self.tabs[idx].sql_other_tabs.take() else {
                continue;
            };
            for copy in &mut job.copies {
                match self.tabs.get(copy.tab) {
                    // The tab is still the one being copied: hand over more.
                    Some(t) if crate::app::slice_feed::RowsStamp::of(&t.table) == copy.stamp => {
                        copy.feed.feed(&t.table)
                    }
                    // Closed, moved or changed: that table is left out.
                    _ => copy.feed.tx = None,
                }
            }
            let Some(outcome) = job.task.poll_result() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                self.tabs[idx].sql_other_tabs = Some(job);
                continue;
            };
            let tab = &mut self.tabs[idx];
            let Some(ws) = tab.sql_workspace.as_mut() else {
                continue;
            };
            for ((name, copied), copy) in outcome.unwrap_or_default().into_iter().zip(&job.copies) {
                if let Ok(rows) = copied {
                    let origin = octa::sql::TableOrigin::TabClone(copy.title.clone());
                    ws.note_table(&name, origin, rows);
                    tab.sql_auto_registered.push(name);
                }
            }
        }
    }

    /// ATTACH a saved live-database connection into the active tab's SQL
    /// workspace, on a worker, with Cancel in the status bar. PostgreSQL,
    /// Redshift and MySQL attach natively (token command, extension install
    /// and handshake on the workspace's runner connection); every other
    /// engine imports its tables one SELECT each, with no cap on how many.
    pub(super) fn workspace_attach_db_connection(
        &mut self,
        ctx: &egui::Context,
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
        if conn.engine.is_file() {
            match octa::db::file_db::file_path(&conn) {
                Ok(path) => {
                    let kind = if conn.engine == octa::db::DbEngine::DuckDb {
                        AttachKind::DuckDb
                    } else {
                        AttachKind::Sqlite
                    };
                    self.workspace_attach_file(ctx, path, kind, &conn.name);
                }
                Err(e) => self.tabs[self.active_tab].sql.error = Some(format!("{e:#}")),
            }
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        if tab.sql_import.is_some() {
            tab.sql.error = Some(octa::i18n::t("sql.import_busy"));
            return;
        }
        ensure_workspace(tab);
        let Some(ws) = tab.sql_workspace.as_ref() else {
            return;
        };
        let progress = std::sync::Arc::new([
            std::sync::atomic::AtomicUsize::new(0),
            std::sync::atomic::AtomicUsize::new(0),
        ]);
        let task = if conn.engine.duckdb_attachable() {
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &self.settings);
            let ssh_secret =
                octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &self.settings);
            // Only a native ATTACH gets an alias, and it is always the whole
            // server (the menu hands those an empty scope), so the connection
            // name is the alias. An import registers plain workspace tables
            // and names them after themselves.
            let base = octa::sql::sanitize_sql_name(&conn.name);
            let existing: std::collections::HashSet<String> =
                ws.list_attached().iter().map(|a| a.alias.clone()).collect();
            let alias = octa::sql::dedupe_sql_name(&base, |s| existing.contains(s));
            let interrupt = ws.runner_interrupt();
            let job = ws.native_attach_job(&conn, secret.as_deref(), ssh_secret.as_deref(), &alias);
            let (job, interrupt) = match (job, interrupt) {
                (Ok(job), Ok(interrupt)) => (job, interrupt),
                (Err(e), _) | (_, Err(e)) => {
                    tab.sql.error = Some(format!("{e:#}"));
                    return;
                }
            };
            let task = crate::app::pushdown::spawn_local_fallible(move || {
                job().map(SqlAttached::Attached)
            });
            // The status bar's Cancel interrupts the handshake.
            if let Ok(mut c) = task.load.cancel.lock() {
                *c = Some(Box::new(move || interrupt.interrupt()));
            }
            task
        } else {
            let mut sink = ws.import_sink();
            let counts = progress.clone();
            let db_conn = conn.clone();
            let scope = scope.clone();
            self.spawn_server_task(conn.clone(), String::new(), move |c, stop| {
                octa::sql::import_scope(c, &db_conn, &scope, &mut sink, stop, |done, total| {
                    counts[0].store(done, std::sync::atomic::Ordering::Relaxed);
                    counts[1].store(total, std::sync::atomic::Ordering::Relaxed);
                })
                .map(SqlAttached::Imported)
            })
        };
        self.tabs[self.active_tab].sql_import = Some(SqlImport {
            task,
            conn_name: conn.name,
            progress,
            files: false,
        });
        ctx.request_repaint();
    }

    /// Once per frame: keep each attach's status-bar text current and list
    /// what every finished one brought in.
    pub(crate) fn drive_sql_imports(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let tab = &mut self.tabs[idx];
            let Some(import) = tab.sql_import.as_mut() else {
                continue;
            };
            let [done, total] = &*import.progress;
            let total = total.load(std::sync::atomic::Ordering::Relaxed);
            import.task.load.hint = if import.files {
                octa::i18n::t("sql.adding_files")
                    .replace(
                        "{done}",
                        &done.load(std::sync::atomic::Ordering::Relaxed).to_string(),
                    )
                    .replace("{total}", &total.to_string())
            } else if total == 0 {
                octa::i18n::t("sql.attaching").replace("{name}", &import.conn_name)
            } else {
                octa::i18n::t("sql.importing")
                    .replace("{name}", &import.conn_name)
                    .replace(
                        "{done}",
                        &done.load(std::sync::atomic::Ordering::Relaxed).to_string(),
                    )
                    .replace("{total}", &total.to_string())
            };
            let Some(outcome) = import.task.poll_result() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                continue;
            };
            tab.sql_import = None;
            let Some(ws) = tab.sql_workspace.as_mut() else {
                continue;
            };
            let message = match outcome {
                Ok(SqlAttached::Imported(imported)) => {
                    let names = ws.note_imported(imported);
                    format!("Added to SQL workspace: {}", names.join(", "))
                }
                Ok(SqlAttached::Attached(attachment)) => {
                    let alias = attachment.alias.clone();
                    ws.note_attachment(attachment);
                    format!("Attached `{alias}` to SQL workspace")
                }
                Ok(SqlAttached::File(done)) => {
                    let attachment = ws.note_file_attach(done);
                    let label = if attachment.native { "" } else { " (fallback)" };
                    format!("Attached `{}` to SQL workspace{label}", attachment.alias)
                }
                Ok(SqlAttached::Files {
                    imported,
                    errors,
                    cloud_skipped,
                }) => {
                    let names = ws.note_imported(imported);
                    if !errors.is_empty() {
                        tab.sql.error = Some(errors.join("\n"));
                    }
                    match cloud_skipped {
                        Some(skipped) => {
                            let mut msg = octa::i18n::t("sql.cloud_pick_added")
                                .replace("{n}", &names.len().to_string())
                                .replace("{names}", &names.join(", "));
                            if skipped > 0 {
                                msg.push(' ');
                                msg.push_str(
                                    &octa::i18n::t("sql.cloud_pick_skipped")
                                        .replace("{n}", &skipped.to_string()),
                                );
                            }
                            msg
                        }
                        None if names.is_empty() => {
                            Self::prune_inspector_cache(tab);
                            continue;
                        }
                        None => format!("Added to SQL workspace: {}", names.join(", ")),
                    }
                }
                Err(e) => {
                    // `{e:#}` upstream: the server's own reason, not only the
                    // outermost "ATTACHing ...".
                    tab.sql.error = Some(e);
                    continue;
                }
            };
            tab.sql_workspace_open = true;
            Self::prune_inspector_cache(tab);
            self.status_message = Some((message, std::time::Instant::now()));
        }
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
        ctx: &egui::Context,
        files: Vec<(std::path::PathBuf, String)>,
        combine: bool,
        skipped: usize,
    ) {
        if files.is_empty() {
            self.tabs[self.active_tab].sql.error = Some(octa::i18n::t("sql.cloud_pick_all_failed"));
            return;
        }
        self.start_workspace_files(ctx, files, combine, Some(skipped));
    }

    /// Read `files` (path, name stem) and register each as a workspace table
    /// of the active tab, on a worker: a big file is seconds of reading.
    /// `combine` unions them into one table first. The status bar counts the
    /// files and its Cancel stops between two, taking back what was added.
    /// `cloud_skipped` is set for the cloud picker: downloads that failed.
    fn start_workspace_files(
        &mut self,
        ctx: &egui::Context,
        files: Vec<(std::path::PathBuf, String)>,
        combine: bool,
        cloud_skipped: Option<usize>,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.sql_import.is_some() {
            tab.sql.error = Some(octa::i18n::t("sql.import_busy"));
            return;
        }
        ensure_workspace(tab);
        let Some(ws) = tab.sql_workspace.as_ref() else {
            return;
        };
        let mut sink = ws.import_sink();
        let total = if combine { 1 } else { files.len() };
        let progress = std::sync::Arc::new([
            std::sync::atomic::AtomicUsize::new(0),
            std::sync::atomic::AtomicUsize::new(total),
        ]);
        let counts = progress.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let cloud_origin = octa::i18n::t("sql.cloud_pick_origin");
        let task = crate::app::pushdown::spawn_local_fallible(move || {
            let mut imported = Vec::new();
            let mut errors = Vec::new();
            if combine {
                let table = OctaApp::union_cloud_files(&files)?;
                let name = sink.add(&files[0].1, &table)?;
                imported.push(octa::sql::ImportedTable {
                    name,
                    origin: TableOrigin::Db(cloud_origin),
                    rows: table.row_count(),
                });
            } else {
                let registry = octa::formats::FormatRegistry::new();
                for (path, stem) in &files {
                    if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                        sink.roll_back();
                        anyhow::bail!("adding the files was stopped; nothing was added");
                    }
                    let added = registry
                        .reader_for_path(path)
                        .ok_or_else(|| anyhow::anyhow!("no reader for {}", path.display()))
                        .and_then(|reader| reader.read_file(path))
                        .and_then(|table| {
                            let name = sink.add(stem, &table)?;
                            Ok(octa::sql::ImportedTable {
                                name,
                                origin: TableOrigin::File {
                                    path: path.clone(),
                                    inner_table: None,
                                },
                                rows: table.row_count(),
                            })
                        });
                    match added {
                        Ok(t) => imported.push(t),
                        Err(e) => errors.push(format!("{}: {e:#}", path.display())),
                    }
                    counts[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Ok(SqlAttached::Files {
                imported,
                errors,
                cloud_skipped,
            })
        });
        // The status bar's Cancel stops between two files.
        if let Ok(mut c) = task.load.cancel.lock() {
            *c = Some(Box::new(move || {
                stop.store(true, std::sync::atomic::Ordering::Relaxed)
            }));
        }
        self.tabs[self.active_tab].sql_import = Some(SqlImport {
            task,
            conn_name: String::new(),
            progress,
            files: true,
        });
        ctx.request_repaint();
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

    pub(super) fn workspace_add_tables_via_picker(&mut self, ctx: &egui::Context) {
        let paths = match rfd::FileDialog::new()
            .set_title("Add tables to SQL workspace")
            .pick_files()
        {
            Some(ps) if !ps.is_empty() => ps,
            _ => return,
        };
        let files = paths
            .into_iter()
            .map(|path| {
                let stem = path
                    .file_stem()
                    .map(|s| octa::sql::sanitize_sql_name(&s.to_string_lossy()))
                    .unwrap_or_else(|| "table".to_string());
                (path, stem)
            })
            .collect();
        self.start_workspace_files(ctx, files, false, None);
    }

    /// **Attach database...**: pick a DuckDB or SQLite file and attach it.
    pub(super) fn workspace_attach_db_via_picker(&mut self, ctx: &egui::Context) {
        let path = match rfd::FileDialog::new()
            .set_title("Attach database to SQL workspace")
            .add_filter("DuckDB / SQLite", &["duckdb", "ddb", "sqlite", "db"])
            .pick_file()
        {
            Some(p) => p,
            None => return,
        };
        let kind = AttachKind::from_path(&path);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "db".to_string());
        self.workspace_attach_file(ctx, path, kind, &stem);
    }

    /// Attach a DuckDB or SQLite file (picked, or a saved connection's file)
    /// on a worker. A DuckDB file attaches at once; SQLite needs DuckDB's
    /// sqlite extension (downloaded on first use) or, failing that, a copy of
    /// every table, which the status bar's Cancel stops. The caller says which
    /// engine and what to call it.
    pub(super) fn workspace_attach_file(
        &mut self,
        ctx: &egui::Context,
        path: std::path::PathBuf,
        kind: AttachKind,
        name: &str,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.sql_import.is_some() {
            tab.sql.error = Some(octa::i18n::t("sql.import_busy"));
            return;
        }
        ensure_workspace(tab);
        let Some(ws) = tab.sql_workspace.as_ref() else {
            return;
        };
        let base = octa::sql::sanitize_sql_name(name);
        let existing: std::collections::HashSet<String> =
            ws.list_attached().iter().map(|a| a.alias.clone()).collect();
        let alias = octa::sql::dedupe_sql_name(&base, |s| existing.contains(s));
        let job = match ws.file_attach_job(&path, &alias, kind) {
            Ok(job) => job,
            Err(e) => {
                tab.sql.error = Some(format!("{e:#}"));
                return;
            }
        };
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let task = crate::app::pushdown::spawn_local_fallible(move || {
            job(&stopped).map(SqlAttached::File)
        });
        // The status bar's Cancel stops a SQLite copy between two tables.
        if let Ok(mut c) = task.load.cancel.lock() {
            *c = Some(Box::new(move || {
                stop.store(true, std::sync::atomic::Ordering::Relaxed)
            }));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.tabs[self.active_tab].sql_import = Some(SqlImport {
            task,
            conn_name: name,
            progress: Default::default(),
            files: false,
        });
        ctx.request_repaint();
    }

    pub(super) fn workspace_remove_table(&mut self, sql_name: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.remove_table(sql_name)
        {
            tab.sql.error = Some(format!("{e:#}"));
        }
        Self::prune_inspector_cache(tab);
    }

    pub(super) fn workspace_rename_table(&mut self, from: &str, to: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.rename_table(from, to)
        {
            // `{e:#}`: the reason (name taken, empty) is the inner context.
            tab.sql.error = Some(format!("{e:#}"));
        }
        Self::prune_inspector_cache(tab);
    }

    pub(super) fn workspace_detach(&mut self, alias: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.detach(alias)
        {
            tab.sql.error = Some(format!("{e:#}"));
        }
        Self::prune_inspector_cache(tab);
    }
}

/// What a background attach brought into the workspace.
pub(crate) enum SqlAttached {
    /// A native ATTACH (PostgreSQL, Redshift, MySQL).
    Attached(octa::sql::Attachment),
    /// A DuckDB or SQLite file (**Attach database...**).
    File(octa::sql::FileAttached),
    /// Tables imported one SELECT each (every other engine).
    Imported(Vec<octa::sql::ImportedTable>),
    /// Files added from disk or the cloud picker, each on its own: the ones
    /// that worked, why the others did not, and (cloud picker) how many
    /// downloads failed before.
    Files {
        imported: Vec<octa::sql::ImportedTable>,
        errors: Vec<String>,
        cloud_skipped: Option<usize>,
    },
}

/// A live-database attach into a tab's SQL workspace, running on a worker.
/// Dropping it (tab closed) stops the worker; an import then takes back the
/// tables it already brought in.
pub(crate) struct SqlImport {
    pub(crate) task: crate::app::pushdown::ServerTask<SqlAttached>,
    conn_name: String,
    /// Tables done, tables in all.
    progress: std::sync::Arc<[std::sync::atomic::AtomicUsize; 2]>,
    /// Files being added, not a database: the status bar says so.
    files: bool,
}

/// One other tab being copied into a workspace.
struct OtherTabFeed {
    /// Its index when the copy started; the stamp says if it is still it.
    tab: usize,
    title: String,
    stamp: crate::app::slice_feed::RowsStamp,
    feed: crate::app::slice_feed::SliceFeed,
}

/// The other open tabs being copied into a tab's SQL workspace, so a query
/// can join them (`sync_open_tabs_into_workspace`).
pub(crate) struct OtherTabsCopy {
    copies: Vec<OtherTabFeed>,
    task: crate::app::pushdown::ServerTask<Vec<(String, Result<usize, String>)>>,
}
