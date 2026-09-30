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

    /// Run `query` in the active tab's local workspace, showing the running
    /// state first. A local query blocks this thread until DuckDB returns, so
    /// it is parked and started by [`Self::run_pending_sql`] only once a frame
    /// with the running state has been painted; otherwise the last result sat
    /// there frozen and looked like the answer.
    pub(super) fn queue_local_query(&mut self, ctx: &egui::Context, query: String) {
        if query.trim().is_empty() {
            return;
        }
        let pane = &mut self.tabs[self.active_tab].sql;
        pane.start_run();
        pane.pending_run = Some((query, ctx.cumulative_frame_nr()));
        ctx.request_repaint();
    }

    /// Per frame: start the active pane's parked local query once the frame
    /// after it was parked (the one showing the running state) is on screen.
    pub(crate) fn run_pending_sql(&mut self, ctx: &egui::Context) {
        let pane = &mut self.tabs[self.active_tab].sql;
        match pane.pending_run {
            Some((_, parked)) if ctx.cumulative_frame_nr() >= parked + 2 => {
                let (query, _) = pane.pending_run.take().expect("matched above");
                self.run_workspace_query(ctx, query);
            }
            Some(_) => ctx.request_repaint(),
            None => {}
        }
    }

    pub(super) fn run_workspace_query(&mut self, ctx: &egui::Context, query: String) {
        // Captured before the `tab` borrow; used by the post-mutation row-diff
        // highlight below.
        let diff_enabled = self.settings.sql_row_diff_highlight_enabled;
        let diff_secs = self.settings.sql_row_diff_highlight_secs;
        let readonly = self.is_readonly();
        let history_on = self.settings.sql_history_enabled;
        let history_limit = self.settings.sql_history_limit;
        let page_rows = self.settings.sql_result_page_rows;
        let tab = &mut self.tabs[self.active_tab];
        // Synchronous from here on: nothing is painted until it returns, so
        // the running state can end now and every exit below is covered.
        tab.sql.running_since = None;
        // Refresh `data` from the live edited table on every run so the
        // user's in-memory edits are visible to the next query.
        let mut snapshot = tab.table.clone();
        snapshot.apply_edits();
        ensure_workspace(tab);
        let started;
        let outcome = {
            let ws = tab.sql_workspace.as_mut().expect("workspace just ensured");
            // An empty tab registers no `data` (zero columns); attached
            // servers stay queryable.
            if snapshot.col_count() > 0
                && let Err(e) = ws.set_active_table(&snapshot)
            {
                tab.sql.error = Some(e.to_string());
                return;
            }
            started = std::time::Instant::now();
            ws.execute_paged(&query, page_rows, tab.sql.id)
        };
        // Split the page count off so every arm below keeps working on a
        // plain `QueryOutcome`.
        let mut result_total = None;
        let outcome = outcome.map(|paged| {
            result_total = paged.total_rows;
            paged.outcome
        });
        // Before the match, so a failed query is timed too: the interesting
        // case is the one that ran for a minute and then errored.
        tab.sql.last_duration_ms = Some(started.elapsed().as_millis() as u64);
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
                }
                octa::sql::QueryKind::Mutation => {
                    // Read-only (mode or a live-database tab): the mutation
                    // ran against the workspace temp table only; refuse to
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
                    apply_sql_diff_highlight(tab, &snapshot, &mut mutated, diff_enabled, diff_secs);
                    tab.table = mutated;
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
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                        self.tabs[self.active_tab].title_display(),
                    ));
                }
            },
            Err(e) => {
                tab.sql.error = Some(e.to_string());
            }
        }
    }

    pub(super) fn refresh_active_table_in_workspace(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let mut snapshot = tab.table.clone();
        snapshot.apply_edits();
        ensure_workspace(tab);
        if let Some(ws) = tab.sql_workspace.as_mut() {
            // Remote table lists can drift; refresh re-queries them too.
            ws.invalidate_attached_cache();
            if snapshot.col_count() == 0 {
                tab.sql.error = None;
            } else if let Err(e) = ws.set_active_table(&snapshot) {
                tab.sql.error = Some(e.to_string());
            } else {
                tab.sql.error = None;
            }
        }
        Self::invalidate_inspector_for_data(tab);
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
