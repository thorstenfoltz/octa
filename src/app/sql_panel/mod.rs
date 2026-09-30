//! Render the SQL editor panel and apply the user's actions: run query,
//! clear result, export result, plus the multi-table workspace controls
//! (add table, attach DB, detach, refresh `data`, write result to DB).
//! The panel is only visible while the active tab is in Table view.

mod render;
mod run;
mod server;
mod workspace;

use eframe::egui;

use octa::data::ViewMode;
use octa::sql::{AttachKind, RegisteredTable, TableOrigin};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{InspectorCacheEntry, OctaApp, TabState};
use crate::view_modes;
use crate::view_modes::sql::{
    DbAttachEntry, DrillMenu, NodeListing, WorkspaceAttachment, WorkspaceRow,
};

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

/// Outcome of a finished "Run on server" query, written by the worker.
pub(crate) enum ServerQueryDone {
    /// A SELECT: the fetched rows (boxed: a `DataTable` inline would dwarf
    /// the other variants).
    Rows(Box<octa::data::DataTable>),
    /// A mutation: rows affected.
    Affected(u64),
    Failed(String),
}

/// One in-flight "Run on server" query (at most one at a time, app-wide).
pub(crate) struct SqlServerJob {
    /// The editor pane (`SqlPane.id`) the query was started from. Pane ids
    /// are unique across tabs, so the result finds its pane after tabs were
    /// reordered or other panes closed; the drain also re-checks that the
    /// pane's tab still targets the same connection.
    pub(crate) pane_id: u64,
    pub(crate) conn_id: String,
    pub(crate) query: String,
    /// When the query was sent, so the history can record what it cost. The
    /// wall-clock round trip is the number a user cares about here.
    pub(crate) started: std::time::Instant,
    pub(crate) result: std::sync::Arc<std::sync::Mutex<Option<ServerQueryDone>>>,
    /// Cancel handle delivered by the worker once the connection is up.
    pub(crate) cancel: SharedCancel,
}

/// Shared slot for a connector's thread-safe cancel closure.
pub(crate) type SharedCancel = std::sync::Arc<std::sync::Mutex<Option<Box<dyn Fn() + Send>>>>;

impl OctaApp {
    /// Analyse -> SQL and its shortcut: hide the panel, or open it the way
    /// [`Self::open_sql_panel`] does.
    pub(crate) fn toggle_sql_panel(&mut self, idx: usize) {
        let tab = &mut self.tabs[idx];
        if tab.sql_panel_open {
            tab.sql_panel_open = false;
            tab.sql_maximised = false;
        } else {
            self.open_sql_panel(idx);
        }
    }

    /// Show the SQL panel on tab `idx`, editor focused. An empty tab has no
    /// table for the panel to sit beside, so there it fills the window; the
    /// header's Restore button docks it again.
    pub(crate) fn open_sql_panel(&mut self, idx: usize) {
        let tab = &mut self.tabs[idx];
        tab.sql_panel_open = true;
        tab.sql.focus_pending = true;
        let blank = tab.table.col_count() == 0
            && tab.table.source_path.is_none()
            && tab.raw_content.as_deref().is_none_or(str::is_empty);
        if blank {
            tab.sql_maximised = true;
        }
    }
}

/// One sidebar listing as the attach menu needs it. Which level the names
/// belong to is the node path's depth, exactly as the sidebar's worker
/// decides it, so the three list variants collapse into one.
pub(super) fn node_listing(state: &super::db_browser::DbListState) -> NodeListing {
    use super::db_browser::DbListState;
    match state {
        DbListState::Loading => NodeListing::Loading,
        DbListState::Catalogs(v) | DbListState::Schemas(v) | DbListState::Tables(v) => {
            NodeListing::Ready(v.clone())
        }
        DbListState::Error(e) => NodeListing::Failed(e.clone()),
    }
}

/// Build a `WorkspaceRow` slice describing the tab's current SQL workspace.
/// Returns an empty pair when the workspace hasn't been instantiated yet
/// (the panel renders a placeholder "only `data`" header in that case).
pub(super) fn workspace_snapshot(tab: &TabState) -> (Vec<WorkspaceRow>, Vec<WorkspaceAttachment>) {
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
                    AttachKind::Postgres => "Postgres",
                    AttachKind::MySql => "MySQL",
                    AttachKind::Mssql => "MSSQL",
                },
                native: a.native,
                table_count,
                schemas,
            });
        }
    } else if tab.table.col_count() > 0 {
        // Stub row so the panel header reads "Workspace (only `data`)" even
        // before the user has triggered any SQL action. The actual workspace
        // is built on first action. An empty tab has no `data` at all.
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
/// surface through `tab.sql.error`.
/// Where a tab's queries run, as the history records it: the connection the
/// editor targets, else the database, cloud object or file the tab was opened
/// from, else one shared scratch source.
pub(crate) fn sql_history_scope(tab: &TabState) -> String {
    use octa::sql::history;
    if let Some(conn_id) = tab.sql_target.as_deref() {
        return history::db_scope(conn_id);
    }
    if let Some(origin) = tab.db_origin.as_ref() {
        return history::db_scope(&origin.conn_id);
    }
    if let Some(origin) = tab.cloud_origin.as_ref() {
        return history::cloud_scope(&origin.conn_id, &origin.key);
    }
    match tab.table.source_path.as_deref() {
        Some(path) => history::file_scope(path),
        None => "scratch".to_string(),
    }
}

/// The table a tab was opened from, but only while the editor targets that
/// same connection: the table name the hint, the autocomplete and Ask SQL
/// address. A tab pointed at some other connection has no table there.
impl OctaApp {
    /// **Format** (button or shortcut) on the active tab's SQL editor, in the
    /// dialect of wherever the query runs: DuckDB reads like Postgres, and
    /// SQL Server needs `[bracketed names]` kept whole.
    pub(crate) fn format_sql_editor(&mut self, ctx: &egui::Context) {
        use octa::db::DbEngine;
        use octa::sql::format::FormatDialect;
        let tab = &self.tabs[self.active_tab];
        let dialect = match tab
            .sql_target
            .as_deref()
            .and_then(|id| self.find_db_conn(id))
        {
            None => FormatDialect::Postgres,
            Some(c) => match c.engine {
                DbEngine::Postgres | DbEngine::Redshift => FormatDialect::Postgres,
                DbEngine::Mssql => FormatDialect::SqlServer,
                _ => FormatDialect::Generic,
            },
        };
        let opts = self.settings.sql_format.clone();
        let tab = &mut self.tabs[self.active_tab];
        crate::view_modes::sql::format_in_editor(ctx, &mut tab.sql, &opts, dialect);
    }
}

pub(crate) fn server_origin(tab: &TabState) -> Option<&super::state::DbOrigin> {
    tab.db_origin
        .as_ref()
        .filter(|o| tab.sql_target.as_deref() == Some(o.conn_id.as_str()))
}

/// Column names of live-server tables for the editor's autocomplete, keyed by
/// `(conn_id, schema, table)`. `None` while the fetch runs or after it failed,
/// so a table is asked about once per session.
pub(crate) type ServerColumns = std::sync::Arc<
    std::sync::Mutex<std::collections::HashMap<(String, String, String), Option<Vec<String>>>>,
>;

/// Record an executed query, in the app's list and in the persisted history.
/// Skips blank queries; a no-op when the user has history switched off.
///
/// `enabled` and `limit` are read from settings by the caller, because every
/// call site already holds the tab mutably.
pub(super) fn record_sql_history(
    history: &mut Vec<octa::sql::history::SqlHistoryEntry>,
    tab: &TabState,
    query: &str,
    started: std::time::Instant,
    rows: usize,
    enabled: bool,
    limit: usize,
) {
    if !enabled {
        return;
    }
    let entry = octa::sql::history::SqlHistoryEntry {
        query: query.trim().to_string(),
        at_unix: octa::sql::history::now_unix(),
        duration_ms: started.elapsed().as_millis() as u64,
        rows,
        source: sql_history_scope(tab),
    };
    // The in-memory copy is what the History menu draws; the store is what
    // survives a restart. Same fold, so the two never diverge.
    octa::sql::history::fold(history, entry.clone(), limit);
    octa::sql::history::record(entry, enabled, limit);
}

/// Mark the cells/rows that a SQL mutation changed (positional diff of the
/// pre/post tables via `compare_ordered`) so the user can see the effect, and
/// arm the timed clear. Cells that differ get a cell mark; rows that only exist
/// in the new table (inserts / longer result) get a row mark. No-op when the
/// feature is off or nothing changed.
pub(super) fn apply_sql_diff_highlight(
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

pub(super) fn ensure_workspace(tab: &mut TabState) {
    if tab.sql_workspace.is_some() {
        return;
    }
    match octa::sql::SqlWorkspace::new() {
        Ok(mut ws) => {
            let mut snapshot = tab.table.clone();
            snapshot.apply_edits();
            // An empty tab (no table open yet) still gets a workspace so the
            // user can ATTACH servers and query them directly; `data` is just
            // not registered (a zero-column table cannot be).
            if snapshot.col_count() > 0
                && let Err(e) = ws.set_active_table(&snapshot)
            {
                tab.sql.error = Some(e.to_string());
                return;
            }
            tab.sql_workspace = Some(ws);
        }
        Err(e) => {
            tab.sql.error = Some(format!("failed to start SQL workspace: {e}"));
        }
    }
}
