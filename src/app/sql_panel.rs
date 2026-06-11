//! Render the SQL editor panel and apply the user's actions: run query,
//! clear result, export result, plus the multi-table workspace controls
//! (add table, attach DB, detach, refresh `data`, write result to DB).
//! The panel is only visible while the active tab is in Table view.

use eframe::egui;

use octa::data::ViewMode;
use octa::sql::{AttachKind, RegisteredTable, TableOrigin};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{InspectorCacheEntry, OctaApp, TabState};
use crate::view_modes;
use crate::view_modes::sql::{WorkspaceAttachment, WorkspaceRow};

/// Identity of the entry currently selected in the workspace tree. Drives
/// the inspector pane and the cache key for fetched introspection results.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum InspectorTarget {
    /// A workspace table registered under `sql_name` (the active `data`
    /// table, an `--sql-table` extra, etc.).
    RegisteredTable { sql_name: String },
    /// A table inside an ATTACH-ed (native) database.
    AttachedTable {
        alias: String,
        schema: String,
        table: String,
    },
}

impl InspectorTarget {
    /// Fully qualified SQL identifier the user would type to reference this
    /// entry. Used by the inspector's Copy / Insert / Run buttons.
    pub fn qualified_sql(&self) -> String {
        match self {
            InspectorTarget::RegisteredTable { sql_name } => sql_name.clone(),
            InspectorTarget::AttachedTable {
                alias,
                schema,
                table,
            } => format!("{alias}.{schema}.{table}"),
        }
    }
}

impl OctaApp {
    pub(crate) fn render_sql_panel(&mut self, parent_ui: &mut egui::Ui) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let sql_panel_visible = {
            let tab = &self.tabs[self.active_tab];
            tab.sql_panel_open && tab.table.col_count() > 0 && tab.view_mode == ViewMode::Table
        };
        if !sql_panel_visible {
            return;
        }
        let position = self.settings.sql_panel_position;
        let mut sql_action = view_modes::SqlAction::default();
        let editor_font = self.settings.sql_editor_font;
        let autocomplete = self.settings.sql_autocomplete;
        let row_limit = self.settings.sql_default_row_limit;

        // Build a lightweight workspace snapshot up front so the renderer
        // doesn't need to borrow the workspace mutably while drawing.
        let (workspace_rows, workspace_attachments) = {
            let tab = &self.tabs[self.active_tab];
            workspace_snapshot(tab)
        };
        // Cloned up front so the render closure (which mutably borrows `tab`)
        // can hold a reference without fighting the `&mut self.tabs` borrow.
        let sql_snippets_owned = self.sql_snippets.clone();

        let tab = &mut self.tabs[self.active_tab];
        let partial_rows = tab.table.total_rows.and_then(|total| {
            let loaded = tab.table.row_count();
            if loaded < total {
                Some((loaded, total))
            } else {
                None
            }
        });
        // Clone the inspector selection + cached entry up front so the
        // immutable-by-reference SqlViewContext doesn't fight the mutable
        // borrow of `tab` taken inside the render closure. Both are cheap:
        // selection is a small enum of strings, entry holds at most a 5-row
        // sample.
        let inspector_selection_owned: Option<InspectorTarget> =
            tab.sql_inspector_selection.clone();
        let inspector_entry_owned: Option<InspectorCacheEntry> = inspector_selection_owned
            .as_ref()
            .and_then(|t| tab.sql_inspector_cache.get(t).cloned());
        let render = |ui: &mut egui::Ui,
                      tab: &mut TabState,
                      autocomplete: bool,
                      row_limit: usize|
         -> view_modes::SqlAction {
            view_modes::render_sql_view(
                ui,
                tab,
                view_modes::SqlViewContext {
                    autocomplete_enabled: autocomplete,
                    default_row_limit: row_limit,
                    panel_position: position,
                    partial_rows,
                    editor_font,
                    workspace_tables: &workspace_rows,
                    workspace_attachments: &workspace_attachments,
                    sql_snippets: &sql_snippets_owned,
                    inspector_selection: inspector_selection_owned.as_ref(),
                    inspector_entry: inspector_entry_owned.as_ref(),
                },
            )
        };
        match position {
            ui::settings::SqlPanelPosition::Bottom => {
                egui::Panel::bottom("sql_panel")
                    .resizable(true)
                    .default_size(280.0)
                    .min_size(140.0)
                    .show_inside(parent_ui, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Top => {
                egui::Panel::top("sql_panel")
                    .resizable(true)
                    .default_size(280.0)
                    .min_size(140.0)
                    .show_inside(parent_ui, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Left => {
                egui::Panel::left("sql_panel")
                    .resizable(true)
                    .default_size(440.0)
                    .min_size(280.0)
                    .show_inside(parent_ui, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Right => {
                egui::Panel::right("sql_panel")
                    .resizable(true)
                    .default_size(440.0)
                    .min_size(280.0)
                    .show_inside(parent_ui, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
        }
        if sql_action.clear {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_result = None;
            tab.sql_error = None;
        }
        if sql_action.run {
            self.run_workspace_query(ctx);
        }
        if sql_action.export {
            self.export_sql_result();
        }
        if sql_action.close {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_panel_open = false;
        }
        if sql_action.refresh_active {
            self.refresh_active_table_in_workspace();
        }
        if sql_action.add_tables {
            self.workspace_add_tables_via_picker();
        }
        if sql_action.attach_db {
            self.workspace_attach_db_via_picker();
        }
        if let Some(name) = sql_action.remove_table {
            self.workspace_remove_table(&name);
        }
        if let Some(alias) = sql_action.detach_alias {
            self.workspace_detach(&alias);
        }
        if sql_action.open_write_back {
            self.open_sql_write_back_dialog();
        }
        if let Some(key) = sql_action.toggle_tree_key {
            let tab = &mut self.tabs[self.active_tab];
            if !tab.sql_workspace_tree_expanded.remove(&key) {
                tab.sql_workspace_tree_expanded.insert(key);
            }
        }
        if let Some(sel) = sql_action.select_inspector {
            self.workspace_select_inspector(sel);
        } else {
            // Even with no selection change, make sure the cache is populated
            // for the current selection (handles workspace refreshes that
            // invalidate the cache).
            self.workspace_refill_inspector_cache();
        }
        if let Some(q) = sql_action.copy_qualified {
            self.copy_to_clipboard(q);
        }
        if let Some(q) = sql_action.insert_qualified {
            self.insert_select_into_editor(&q);
        }
        if let Some(q) = sql_action.run_qualified {
            self.run_select_for_inspector(&q, ctx);
        }
        if let Some(q) = sql_action.recall_query {
            self.tabs[self.active_tab].sql_query = q;
            self.tabs[self.active_tab].sql_editor_focus_pending = true;
        }
        if let Some(q) = sql_action.insert_snippet {
            self.tabs[self.active_tab].sql_query = q;
            self.tabs[self.active_tab].sql_editor_focus_pending = true;
        }
        if sql_action.save_snippet {
            let query = self.tabs[self.active_tab].sql_query.trim().to_string();
            if !query.is_empty() {
                self.sql_snippet_save = Some(super::state::SqlSnippetDraft {
                    name: String::new(),
                    description: String::new(),
                    query,
                });
            } else {
                self.status_message = Some((
                    octa::i18n::t("sql.snippet_empty"),
                    std::time::Instant::now(),
                ));
            }
        }
        if let Some(name) = sql_action.delete_snippet {
            self.sql_snippets.retain(|s| s.name != name);
            super::sql_snippets::save(&self.sql_snippets);
        }
    }

    fn workspace_select_inspector(&mut self, sel: Option<InspectorTarget>) {
        let tab = &mut self.tabs[self.active_tab];
        tab.sql_inspector_selection = sel;
        self.workspace_refill_inspector_cache();
    }

    fn workspace_refill_inspector_cache(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let target = match tab.sql_inspector_selection.clone() {
            Some(t) => t,
            None => return,
        };
        if tab.sql_inspector_cache.contains_key(&target) {
            return;
        }
        // Build the workspace lazily so the inspector works even on the
        // first interaction with a freshly opened tab.
        ensure_workspace(tab);
        let ws = match tab.sql_workspace.as_ref() {
            Some(w) => w,
            None => return,
        };
        let inspection = match &target {
            InspectorTarget::RegisteredTable { sql_name } => {
                ws.inspect_registered_table(sql_name, 5)
            }
            InspectorTarget::AttachedTable {
                alias,
                schema,
                table,
            } => ws.inspect_attached_table(alias, schema, table, 5),
        };
        tab.sql_inspector_cache.insert(
            target,
            InspectorCacheEntry {
                result: inspection.map_err(|e| e.to_string()),
            },
        );
    }

    fn copy_to_clipboard(&mut self, text: String) {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text.clone());
        }
        self.status_message = Some((format!("Copied `{text}`"), std::time::Instant::now()));
    }

    fn insert_select_into_editor(&mut self, qualified: &str) {
        let tab = &mut self.tabs[self.active_tab];
        let snippet = format!("SELECT * FROM {qualified} LIMIT 100;");
        if tab.sql_query.is_empty() {
            tab.sql_query = snippet;
        } else {
            if !tab.sql_query.ends_with('\n') {
                tab.sql_query.push('\n');
            }
            tab.sql_query.push_str(&snippet);
        }
    }

    fn run_select_for_inspector(&mut self, qualified: &str, ctx: &egui::Context) {
        self.tabs[self.active_tab].sql_query = format!("SELECT * FROM {qualified} LIMIT 100");
        self.run_workspace_query(ctx);
    }

    /// Per-frame: clear any post-mutation row-diff highlight whose timer has
    /// elapsed, and keep repainting while one is active so it fades on time.
    pub(crate) fn expire_sql_diff_highlights(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        let mut any_active = false;
        for tab in &mut self.tabs {
            let Some(until) = tab.sql_diff_highlight_until else {
                continue;
            };
            if now >= until {
                for key in tab.sql_diff_marks.drain(..) {
                    tab.table.clear_mark(key);
                }
                tab.sql_diff_highlight_until = None;
            } else {
                any_active = true;
            }
        }
        if any_active {
            // Ensure the timer fires even without other input.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn run_workspace_query(&mut self, ctx: &egui::Context) {
        // Captured before the `tab` borrow; used by the post-mutation row-diff
        // highlight below.
        let diff_enabled = self.settings.sql_row_diff_highlight_enabled;
        let diff_secs = self.settings.sql_row_diff_highlight_secs;
        let tab = &mut self.tabs[self.active_tab];
        let query = tab.sql_query.clone();
        // Refresh `data` from the live edited table on every run so the
        // user's in-memory edits are visible to the next query.
        let mut snapshot = tab.table.clone();
        snapshot.apply_edits();
        ensure_workspace(tab);
        let outcome = {
            let ws = tab.sql_workspace.as_mut().expect("workspace just ensured");
            if let Err(e) = ws.set_active_table(&snapshot) {
                tab.sql_error = Some(e.to_string());
                return;
            }
            ws.execute(&query)
        };
        match outcome {
            Ok(qo) => match qo.kind {
                octa::sql::QueryKind::Select => {
                    tab.sql_result = Some(qo.table);
                    tab.sql_error = None;
                    record_sql_history(tab, &query);
                    tab.sql_last_query = query;
                }
                octa::sql::QueryKind::Mutation => {
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
                    if let Some(meta) = tab.table.db_meta.as_ref() {
                        let row_count = mutated.row_count();
                        mutated.db_meta = Some(octa::data::DbRowMeta {
                            table_name: meta.table_name.clone(),
                            schema: meta.schema.clone(),
                            row_tags: vec![None; row_count],
                            original: meta.original.clone(),
                            original_columns: meta.original_columns.clone(),
                        });
                    }
                    record_sql_history(tab, &query);
                    // Briefly highlight the cells/rows the mutation changed.
                    apply_sql_diff_highlight(tab, &snapshot, &mut mutated, diff_enabled, diff_secs);
                    tab.table = mutated;
                    tab.table_state = TableViewState::default();
                    tab.filter_dirty = true;
                    tab.sql_result = None;
                    tab.sql_error = None;
                    tab.sql_last_query = String::new();
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
                tab.sql_error = Some(e.to_string());
            }
        }
    }

    fn refresh_active_table_in_workspace(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let mut snapshot = tab.table.clone();
        snapshot.apply_edits();
        ensure_workspace(tab);
        if let Some(ws) = tab.sql_workspace.as_mut() {
            if let Err(e) = ws.set_active_table(&snapshot) {
                tab.sql_error = Some(e.to_string());
            } else {
                tab.sql_error = None;
            }
        }
        Self::invalidate_inspector_for_data(tab);
    }

    /// Drop the cached inspection for `data` so the next selection click
    /// re-fetches the live schema after a refresh.
    fn invalidate_inspector_for_data(tab: &mut TabState) {
        let key = InspectorTarget::RegisteredTable {
            sql_name: "data".to_string(),
        };
        tab.sql_inspector_cache.remove(&key);
    }

    /// Drop cached inspections that no longer correspond to a workspace
    /// entry (e.g. after detaching an attachment or removing a table).
    fn prune_inspector_cache(tab: &mut TabState) {
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

    fn workspace_add_tables_via_picker(&mut self) {
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

    fn workspace_attach_db_via_picker(&mut self) {
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

    fn workspace_remove_table(&mut self, sql_name: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.remove_table(sql_name)
        {
            tab.sql_error = Some(e.to_string());
        }
        Self::prune_inspector_cache(tab);
    }

    fn workspace_detach(&mut self, alias: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.detach(alias)
        {
            tab.sql_error = Some(e.to_string());
        }
        Self::prune_inspector_cache(tab);
    }
}

/// Build a `WorkspaceRow` slice describing the tab's current SQL workspace.
/// Returns an empty pair when the workspace hasn't been instantiated yet
/// (the panel renders a placeholder "only `data`" header in that case).
fn workspace_snapshot(tab: &TabState) -> (Vec<WorkspaceRow>, Vec<WorkspaceAttachment>) {
    let mut tables: Vec<WorkspaceRow> = Vec::new();
    let mut attachments: Vec<WorkspaceAttachment> = Vec::new();
    if let Some(ws) = tab.sql_workspace.as_ref() {
        let mut rows: Vec<&RegisteredTable> = ws.list_tables();
        rows.sort_by(|a, b| {
            // Surface `data` first regardless of alphabetical order.
            let a_active = matches!(a.origin, TableOrigin::ActiveTab);
            let b_active = matches!(b.origin, TableOrigin::ActiveTab);
            b_active.cmp(&a_active).then(a.sql_name.cmp(&b.sql_name))
        });
        for r in rows {
            let is_active = matches!(r.origin, TableOrigin::ActiveTab);
            let origin_display = if is_active {
                match &tab.table.source_path {
                    Some(p) => format!("active tab | {p}"),
                    None => "active tab".to_string(),
                }
            } else {
                r.origin.display()
            };
            tables.push(WorkspaceRow {
                sql_name: r.sql_name.clone(),
                origin: origin_display,
                row_count: r.row_count,
                is_active,
            });
        }
        for a in ws.list_attached() {
            let (table_count, schemas) =
                if a.native {
                    let inner = ws.list_attached_tables(&a.alias).unwrap_or_default();
                    let count = inner.len();
                    let mut by_schema: std::collections::BTreeMap<
                        String,
                        Vec<crate::view_modes::sql::WorkspaceAttachmentTable>,
                    > = std::collections::BTreeMap::new();
                    for t in inner {
                        by_schema.entry(t.schema.clone()).or_default().push(
                            crate::view_modes::sql::WorkspaceAttachmentTable {
                                schema: t.schema,
                                table: t.table,
                                row_count: t.row_count,
                            },
                        );
                    }
                    let schemas: Vec<crate::view_modes::sql::WorkspaceAttachmentSchema> =
                        by_schema
                            .into_iter()
                            .map(|(schema, tables)| {
                                crate::view_modes::sql::WorkspaceAttachmentSchema { schema, tables }
                            })
                            .collect();
                    (count, schemas)
                } else {
                    (0, Vec::new())
                };
            attachments.push(WorkspaceAttachment {
                alias: a.alias.clone(),
                source: a.path.display().to_string(),
                kind_label: match a.kind {
                    AttachKind::DuckDb => "DuckDB",
                    AttachKind::Sqlite => "SQLite",
                },
                native: a.native,
                table_count,
                schemas,
            });
        }
    } else {
        // Stub row so the panel header reads "Workspace (only `data`)" even
        // before the user has triggered any SQL action. The actual workspace
        // is built on first action.
        let origin_display = match &tab.table.source_path {
            Some(p) => format!("active tab | {p}"),
            None => "active tab".to_string(),
        };
        tables.push(WorkspaceRow {
            sql_name: "data".to_string(),
            origin: origin_display,
            row_count: tab.table.row_count(),
            is_active: true,
        });
    }
    (tables, attachments)
}

/// Construct the per-tab SQL workspace on first use, registering the tab's
/// current table as `data`. Errors leave `tab.sql_workspace` as None and
/// surface through `tab.sql_error`.
/// Record an executed query in the tab's session history (most-recent first,
/// de-duplicated, capped). Skips blank queries.
fn record_sql_history(tab: &mut TabState, query: &str) {
    const SQL_HISTORY_CAP: usize = 30;
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    tab.sql_history.retain(|h| h != q);
    tab.sql_history.insert(0, q.to_string());
    tab.sql_history.truncate(SQL_HISTORY_CAP);
}

/// Mark the cells/rows that a SQL mutation changed (positional diff of the
/// pre/post tables via `compare_ordered`) so the user can see the effect, and
/// arm the timed clear. Cells that differ get a cell mark; rows that only exist
/// in the new table (inserts / longer result) get a row mark. No-op when the
/// feature is off or nothing changed.
fn apply_sql_diff_highlight(
    tab: &mut TabState,
    before: &octa::data::DataTable,
    after: &mut octa::data::DataTable,
    enabled: bool,
    secs: u32,
) {
    use octa::data::{MarkColor, MarkKey};
    // Clear any still-pending highlight from a previous mutation first.
    for key in tab.sql_diff_marks.drain(..) {
        after.clear_mark(key);
    }
    tab.sql_diff_highlight_until = None;
    if !enabled {
        return;
    }
    let result = octa::data::compare::compare_ordered(before, after);
    let mut keys: Vec<MarkKey> = Vec::new();
    for change in &result.changed {
        for name in &change.changed_columns {
            if let Some(col) = after.columns.iter().position(|c| &c.name == name) {
                keys.push(MarkKey::Cell(change.row_b, col));
            }
        }
    }
    for &row in &result.only_in_b {
        keys.push(MarkKey::Row(row));
    }
    if keys.is_empty() {
        return;
    }
    for key in &keys {
        after.set_mark(key.clone(), MarkColor::Green);
    }
    tab.sql_diff_marks = keys;
    tab.sql_diff_highlight_until =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(secs.max(1) as u64));
}

fn ensure_workspace(tab: &mut TabState) {
    if tab.sql_workspace.is_some() {
        return;
    }
    match octa::sql::SqlWorkspace::new() {
        Ok(mut ws) => {
            let mut snapshot = tab.table.clone();
            snapshot.apply_edits();
            if let Err(e) = ws.set_active_table(&snapshot) {
                tab.sql_error = Some(e.to_string());
                return;
            }
            tab.sql_workspace = Some(ws);
        }
        Err(e) => {
            tab.sql_error = Some(format!("failed to start SQL workspace: {e}"));
        }
    }
}
