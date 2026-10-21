//! Running a query against the local DuckDB workspace, and keeping the inspector cache honest.

use super::*;

impl OctaApp {
    /// Open `table` as a new tab labelled `name` and switch to it. Always a
    /// new tab, never `push_result_tab`'s blank-tab reuse: the active tab
    /// owns the SQL workspace, attachments included.
    fn push_sql_table_tab(
        &mut self,
        ctx: &egui::Context,
        table: octa::data::DataTable,
        name: String,
    ) {
        let rows = table.row_count();
        let mut new_tab = TabState::new(self.settings.default_search_mode);
        new_tab.table = table;
        new_tab.table.structural_changes = true;
        new_tab.filter_dirty = true;
        new_tab.custom_tab_label = Some(name.clone());
        if rows > 0 {
            new_tab.table_state.selected_cell = Some((0, 0));
        }
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.status_message = Some((
            octa::i18n::t("sql.created_tab")
                .replace("{name}", &name)
                .replace("{n}", &rows.to_string()),
            std::time::Instant::now(),
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.tabs[self.active_tab].title_display(),
        ));
    }

    /// **Open result as tab...**: the current result, every row of it, as a
    /// new tab. A paged result holds only the pages scrolled so far, so the
    /// rest is fetched first.
    pub(super) fn open_sql_result_as_tab(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active_tab];
        let Some(mut table) = tab.sql.result.clone() else {
            return;
        };
        if let (Some(total), Some(ws)) = (tab.sql.result_total, tab.sql_workspace.as_ref()) {
            let loaded = table.row_count();
            if loaded < total {
                match ws.result_page(tab.sql.id, loaded, total - loaded) {
                    Ok(page) => table.rows.extend(page.rows),
                    Err(e) => {
                        tab.sql.error = Some(format!("{e:#}"));
                        return;
                    }
                }
            }
        }
        self.push_sql_table_tab(ctx, table, octa::i18n::t("sql.result_tab_name"));
    }

    /// Run `query` in the active tab's local workspace, on a worker
    /// ([`job`](super::job)): `data` follows the live table, edits included,
    /// and is copied first when it changed. The result lands in
    /// [`Self::land_sql_query`].
    pub(super) fn queue_local_query(&mut self, ctx: &egui::Context, query: String) {
        if query.trim().is_empty() {
            return;
        }
        self.tabs[self.active_tab].sql.start_run();
        self.start_sql_job(self.active_tab, Some(query));
        ctx.request_repaint();
    }

    /// Put a local query's outcome into tab `tab_idx`, whose active editor is
    /// the one that ran it. `readonly`: the tab was read-only when it started.
    pub(super) fn land_sql_query(
        &mut self,
        ctx: &egui::Context,
        tab_idx: usize,
        query: String,
        started: std::time::Instant,
        readonly: bool,
        outcome: Result<octa::sql::PagedResult, String>,
    ) {
        let diff_enabled = self.settings.sql_row_diff_highlight_enabled;
        let diff_secs = self.settings.sql_row_diff_highlight_secs;
        let history_on = self.settings.sql_history_enabled;
        let history_limit = self.settings.sql_history_limit;
        let is_active = tab_idx == self.active_tab;
        let tab = &mut self.tabs[tab_idx];
        // A writing statement may have changed `data` itself, and a refused
        // fold-back leaves it different from the tab.
        if octa::sql::is_mutation(&query) {
            tab.sql_data_stamp = None;
        }
        // Split the page count off so every arm below keeps working on a
        // plain `QueryOutcome`.
        let mut result_total = None;
        let outcome = outcome.map(|paged| {
            result_total = paged.total_rows;
            paged.outcome
        });
        match outcome {
            // CREATE TABLE / VIEW: the statement's table becomes a new tab,
            // named after it. Nothing folds back into this tab, so no
            // read-only refusal either: a new tab is not an edit, and this is
            // how a first table gets made from an empty tab with no `data`.
            Ok(qo) if qo.created.is_some() => {
                let name = qo.created.clone().unwrap_or_default();
                let rows = qo.table.row_count();
                record_sql_history(
                    &mut self.sql_history,
                    tab,
                    &query,
                    started,
                    rows,
                    history_on,
                    history_limit,
                );
                tab.sql.error = None;
                tab.sql.last_query = query;
                tab.sql.last_query_on_server = false;
                self.push_sql_table_tab(ctx, qo.table, name);
            }
            Ok(qo) => match qo.kind {
                octa::sql::QueryKind::Select => {
                    let rows = result_total.unwrap_or_else(|| qo.table.row_count());
                    tab.sql.result = Some(qo.table);
                    tab.sql.result_sel = Default::default();
                    tab.sql.result_total = result_total;
                    tab.sql.error = None;
                    record_sql_history(
                        &mut self.sql_history,
                        tab,
                        &query,
                        started,
                        rows,
                        history_on,
                        history_limit,
                    );
                    tab.sql.last_query = query;
                    tab.sql.last_query_on_server = false;
                }
                octa::sql::QueryKind::Mutation => {
                    // Read-only (mode or a live-database tab): the mutation
                    // ran against the workspace's copy only; refuse to
                    // fold it back into the tab.
                    if readonly {
                        tab.sql.error = Some(octa::i18n::t("db.tab_readonly_note"));
                        return;
                    }
                    // Apply the mutation to the base table directly so
                    // INSERT / UPDATE / DELETE affect the data, not just
                    // a result set. Selection / widths / per-tab UI state
                    // are reset because row/column identity may have changed.
                    let mut mutated = qo.table;
                    if mutated.columns.len() == tab.table.columns.len() {
                        mutated.columns = tab.table.columns.clone();
                    }
                    mutated.source_path = tab.table.source_path.clone();
                    mutated.format_name = tab.table.format_name.clone();
                    mutated.structural_changes = true;
                    if tab.db_origin.is_some() {
                        // A local DuckDB rewrite destroys server row identity.
                        // Rebuilding all-None tags would turn the next Save
                        // into DELETE-all + INSERT-all on the server; dropping
                        // the meta makes Save report "row identity lost"
                        // instead.
                        mutated.db_meta = None;
                    } else if let Some(meta) = tab.table.db_meta.as_ref() {
                        let row_count = mutated.row_count();
                        mutated.db_meta = Some(octa::data::DbRowMeta {
                            table_name: meta.table_name.clone(),
                            schema: meta.schema.clone(),
                            row_tags: vec![None; row_count],
                            original: meta.original.clone(),
                            original_columns: meta.original_columns.clone(),
                        });
                    }
                    record_sql_history(
                        &mut self.sql_history,
                        tab,
                        &query,
                        started,
                        qo.affected.unwrap_or(0),
                        history_on,
                        history_limit,
                    );
                    // Briefly highlight the cells/rows the mutation changed.
                    // Folding back needs an editable tab, whose `data` was
                    // always a copy of its rows.
                    let before = std::mem::replace(&mut tab.table, mutated);
                    apply_sql_diff_highlight(tab, &before, diff_enabled, diff_secs);
                    tab.table_state = TableViewState::default();
                    tab.filter_dirty = true;
                    tab.sql.result = None;
                    tab.sql.result_total = None;
                    tab.sql.error = None;
                    tab.sql.last_query = String::new();
                    let rows = tab.table.row_count();
                    let affected = qo.affected.unwrap_or(0);
                    self.status_message = Some((
                        format!(
                            "SQL applied: {affected} row(s) affected - table now {rows} row(s)"
                        ),
                        std::time::Instant::now(),
                    ));
                    if is_active {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                            self.tabs[tab_idx].title_display(),
                        ));
                    }
                }
            },
            Err(e) => {
                tab.sql.error = Some(e);
            }
        }
    }

    pub(super) fn refresh_active_table_in_workspace(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        ensure_workspace(tab);
        if let Some(ws) = tab.sql_workspace.as_mut() {
            // Remote table lists can drift; refresh re-queries them too.
            ws.invalidate_attached_cache();
            // Asked for by name, so copied even when the table looks unchanged.
            tab.sql_data_stamp = None;
            tab.sql.error = None;
        }
        Self::invalidate_inspector_for_data(tab);
        self.start_sql_job(self.active_tab, None);
    }

    /// Drop the cached inspection for `data` so the next selection click
    /// re-fetches the live schema after a refresh.
    pub(super) fn invalidate_inspector_for_data(tab: &mut TabState) {
        let key = InspectorTarget::RegisteredTable {
            sql_name: "data".to_string(),
        };
        tab.sql_inspector_cache.remove(&key);
    }

    /// Drop cached inspections that no longer correspond to a workspace
    /// entry (e.g. after detaching an attachment or removing a table).
    pub(super) fn prune_inspector_cache(tab: &mut TabState) {
        let registered: std::collections::HashSet<String> = tab
            .sql_workspace
            .as_ref()
            .map(|ws| {
                ws.list_tables()
                    .iter()
                    .map(|t| t.sql_name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let attached: std::collections::HashSet<String> = tab
            .sql_workspace
            .as_ref()
            .map(|ws| ws.list_attached().iter().map(|a| a.alias.clone()).collect())
            .unwrap_or_default();
        tab.sql_inspector_cache.retain(|k, _| match k {
            InspectorTarget::RegisteredTable { sql_name } => registered.contains(sql_name),
            InspectorTarget::AttachedTable { alias, .. } => attached.contains(alias),
        });
        if let Some(sel) = tab.sql_inspector_selection.clone() {
            let still_valid = match &sel {
                InspectorTarget::RegisteredTable { sql_name } => registered.contains(sql_name),
                InspectorTarget::AttachedTable { alias, .. } => attached.contains(alias),
            };
            if !still_valid {
                tab.sql_inspector_selection = None;
            }
        }
    }
}
