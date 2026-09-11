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
    /// Tab index the query was started from; the drain re-checks the tab
    /// still shows the same connection before applying the result.
    pub(crate) tab_idx: usize,
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

/// One sidebar listing as the attach menu needs it. Which level the names
/// belong to is the node path's depth, exactly as the sidebar's worker
/// decides it, so the three list variants collapse into one.
fn node_listing(state: &super::db_browser::DbListState) -> NodeListing {
    use super::db_browser::DbListState;
    match state {
        DbListState::Loading => NodeListing::Loading,
        DbListState::Catalogs(v) | DbListState::Schemas(v) | DbListState::Tables(v) => {
            NodeListing::Ready(v.clone())
        }
        DbListState::Error(e) => NodeListing::Failed(e.clone()),
    }
}

impl OctaApp {
    pub(crate) fn render_sql_panel(&mut self, parent_ui: &mut egui::Ui) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let sql_panel_visible = {
            let tab = &self.tabs[self.active_tab];
            // No col_count gate: an empty tab can still ATTACH saved
            // connections and query servers directly (no `data` table then).
            tab.sql_panel_open && tab.view_mode == ViewMode::Table
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

        // Server-mode context: the connection name behind the active tab's
        // db_origin (if it still exists), whether a server query is running,
        // and the saved connections for the attach menu.
        let server_conn_name: Option<String> =
            self.tabs[self.active_tab].db_origin.as_ref().and_then(|o| {
                self.settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == o.conn_id)
                    .map(|c| c.name.clone())
            });
        let server_running = self.sql_server_job.is_some();
        let chat_profile_available = !self.settings.chat_profiles.is_empty();
        let ask_profiles: Vec<(String, String)> = self
            .settings
            .chat_profiles
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        // Seed from the chat panel's active profile the first time, so the box
        // starts on the assistant the user already picked rather than blank.
        let active_profile = self.settings.chat_active_profile.clone();
        // An import connection's menu entry opens into its tree, so the menu
        // needs every node the user has walked into, not just the root. The
        // listings come off the sidebar's shared cache: the same worker, the
        // same result, no second network path.
        //
        // ponytail: snapshots each frame the panel is open rather than
        // holding the cache's lock across the draw. Only import connections
        // are copied, so the common Postgres/MySQL setup pays nothing; hand
        // the menu the `Arc` if a big expanded tree ever shows up in a frame
        // profile.
        let imports: std::collections::HashSet<&str> = self
            .settings
            .db_connections
            .iter()
            .filter(|c| !c.engine.duckdb_attachable())
            .map(|c| c.id.as_str())
            .collect();
        let mut node_cache: std::collections::HashMap<
            String,
            std::collections::HashMap<Vec<String>, NodeListing>,
        > = std::collections::HashMap::new();
        if !imports.is_empty()
            && let Ok(m) = self.db_browser.listings.lock()
        {
            for ((id, path), state) in m.iter() {
                if !imports.contains(id.as_str()) {
                    continue;
                }
                let parts = super::db_browser::split_path(path)
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                node_cache
                    .entry(id.clone())
                    .or_default()
                    .insert(parts, node_listing(state));
            }
        }
        let db_connections: Vec<DbAttachEntry> = self
            .settings
            .db_connections
            .iter()
            .map(|c| DbAttachEntry {
                id: c.id.clone(),
                name: c.name.clone(),
                drill: (!c.engine.duckdb_attachable()).then(|| DrillMenu {
                    has_catalogs: c.engine.has_catalogs(),
                    nodes: node_cache.get(&c.id).cloned().unwrap_or_default(),
                }),
            })
            .collect();
        let cloud_connections: Vec<(String, String)> = self
            .settings
            .cloud_connections
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();

        let tab = &mut self.tabs[self.active_tab];
        if tab.sql_ask_profile.is_empty() {
            tab.sql_ask_profile = active_profile;
        }
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
                    inspector_selection: inspector_selection_owned.as_ref(),
                    inspector_entry: inspector_entry_owned.as_ref(),
                    server_conn_name: server_conn_name.clone(),
                    server_running,
                    db_connections: db_connections.clone(),
                    cloud_connections: cloud_connections.clone(),
                    chat_profile_available,
                    ask_profiles: ask_profiles.clone(),
                },
            )
        };
        // Docked left or right the panel divides the width, top or bottom the
        // height, and either way it must leave the table something to live in
        // once the window is small. `panel_fit::clamp` hands back the same
        // numbers on a normal window.
        let side = matches!(
            position,
            ui::settings::SqlPanelPosition::Left | ui::settings::SqlPanelPosition::Right
        );
        let (available, want_default, want_min) = if side {
            (parent_ui.available_width(), 440.0, 280.0)
        } else {
            (parent_ui.available_height(), 280.0, 140.0)
        };
        let (default_size, min_size) = ui::panel_fit::clamp(available, want_default, want_min);
        let mut body = |ui: &mut egui::Ui| {
            sql_action = render(ui, tab, autocomplete, row_limit);
        };
        match position {
            ui::settings::SqlPanelPosition::Bottom => egui::Panel::bottom("sql_panel"),
            ui::settings::SqlPanelPosition::Top => egui::Panel::top("sql_panel"),
            ui::settings::SqlPanelPosition::Left => egui::Panel::left("sql_panel"),
            ui::settings::SqlPanelPosition::Right => egui::Panel::right("sql_panel"),
        }
        .resizable(true)
        .default_size(default_size)
        .min_size(min_size)
        .show(parent_ui, &mut body);
        if sql_action.clear {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_result = None;
            tab.sql_error = None;
        }
        if sql_action.run {
            let tab = &self.tabs[self.active_tab];
            if tab.db_origin.is_some() && tab.sql_run_on_server {
                self.run_server_query(ctx);
            } else {
                self.run_workspace_query(ctx);
            }
        }
        if sql_action.cancel_server {
            self.cancel_server_query();
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
        if let Some((conn_id, scope)) = sql_action.attach_db_connection {
            self.workspace_attach_db_connection(&conn_id, &scope);
        }
        if let Some((conn_id, parts)) = sql_action.list_db_node {
            let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
            let path = super::db_browser::join_path(&refs);
            self.ensure_db_listing(ctx, conn_id, path);
        }
        if let Some(conn_id) = sql_action.attach_cloud_connection {
            self.open_cloud_workspace_picker(&conn_id);
        }
        if let Some(name) = sql_action.remove_table {
            self.workspace_remove_table(&name);
        }
        if let Some((from, to)) = sql_action.rename_table {
            self.workspace_rename_table(&from, &to);
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
        if let Some(question) = sql_action.ask {
            self.start_ask_sql(ctx, question);
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
        if sql_action.clear_history {
            let tab = &mut self.tabs[self.active_tab];
            let scope = sql_history_scope(tab);
            tab.sql_history.clear();
            octa::sql::history::clear(&scope);
        }
        if sql_action.open_snippets_window {
            self.sql_snippets_window_open = !self.sql_snippets_window_open;
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

    /// Run the editor's query on the live database server the active tab was
    /// opened from, on a worker thread (network). One in-flight query at a
    /// time; the result lands via `drain_sql_server_job`.
    pub(crate) fn run_server_query(&mut self, ctx: &egui::Context) {
        if self.sql_server_job.is_some() {
            return;
        }
        let tab_idx = self.active_tab;
        let Some(origin) = self.tabs[tab_idx].db_origin.clone() else {
            return;
        };
        let query = self.tabs[tab_idx].sql_query.clone();
        if query.trim().is_empty() {
            return;
        }
        let Some(conn) = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == origin.conn_id)
            .cloned()
        else {
            self.tabs[tab_idx].sql_error = Some(octa::i18n::t("sql.server_conn_gone"));
            return;
        };
        self.tabs[tab_idx].sql_error = None;
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cancel = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.sql_server_job = Some(SqlServerJob {
            started: std::time::Instant::now(),
            tab_idx,
            conn_id: conn.id.clone(),
            query: query.clone(),
            result: result.clone(),
            cancel: cancel.clone(),
        });
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        let cache = self.db_conn_cache.clone();
        std::thread::spawn(move || {
            let done = (|| -> Result<ServerQueryDone, String> {
                let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                let ssh_secret =
                    octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                if octa::sql::is_mutation(&query) {
                    octa::db::ensure_write_allowed(&conn, Some(&query))
                        .map_err(|e| format!("{e:#}"))?;
                }
                // No `with_conn` auto-retry here: a user cancel surfaces as a
                // query error, and a retry would silently re-run the very
                // statement that was just cancelled. Single attempt; drop the
                // cached connector on failure so the next run reconnects.
                let (shared, _) = cache
                    .get_or_connect(&conn, secret.as_deref(), ssh_secret.as_deref())
                    .map_err(|e| format!("{e:#}"))?;
                let res = {
                    let mut c = super::db_conn_cache::lock_connector(&shared);
                    if let Ok(mut slot) = cancel.lock() {
                        *slot = c.cancel_handle();
                    }
                    if octa::sql::is_mutation(&query) {
                        c.execute(&query).map(ServerQueryDone::Affected)
                    } else {
                        c.query(&query).map(|t| ServerQueryDone::Rows(Box::new(t)))
                    }
                };
                res.map_err(|e| {
                    cache.invalidate(&conn.id);
                    format!("{e:#}")
                })
            })();
            let done = done.unwrap_or_else(ServerQueryDone::Failed);
            if let Ok(mut r) = result.lock() {
                *r = Some(done);
            }
            ctx.request_repaint();
        });
    }

    /// Best-effort cancel of the in-flight server query (Postgres only; the
    /// other engines have no cross-thread cancel).
    fn cancel_server_query(&mut self) {
        if let Some(job) = &self.sql_server_job
            && let Ok(c) = job.cancel.lock()
            && let Some(f) = c.as_ref()
        {
            f();
        }
    }

    /// Apply a finished server query to its tab. Called once per frame from
    /// the update loop.
    pub(crate) fn drain_sql_server_job(&mut self) {
        let Some(job) = &self.sql_server_job else {
            return;
        };
        let done = {
            let Ok(mut r) = job.result.lock() else {
                return;
            };
            r.take()
        };
        let Some(done) = done else {
            return;
        };
        let job = self.sql_server_job.take().expect("job checked above");
        // Read before the tab borrow: `record_sql_history` runs while the tab
        // is held mutably.
        let history_on = self.settings.sql_history_enabled;
        let history_limit = self.settings.sql_history_limit;
        // Only apply if the originating tab still shows the same connection
        // (tabs may have been closed/reordered while the query ran).
        let Some(tab) = self.tabs.get_mut(job.tab_idx).filter(|t| {
            t.db_origin
                .as_ref()
                .is_some_and(|o| o.conn_id == job.conn_id)
        }) else {
            self.status_message = Some((
                octa::i18n::t("sql.server_tab_gone"),
                std::time::Instant::now(),
            ));
            return;
        };
        tab.sql_last_duration_ms = Some(job.started.elapsed().as_millis() as u64);
        match done {
            ServerQueryDone::Rows(t) => {
                let rows = t.row_count();
                tab.sql_result = Some(*t);
                tab.sql_error = None;
                record_sql_history(
                    tab,
                    &job.query,
                    job.started,
                    rows,
                    history_on,
                    history_limit,
                );
                tab.sql_last_query = job.query;
            }
            ServerQueryDone::Affected(n) => {
                record_sql_history(
                    tab,
                    &job.query,
                    job.started,
                    n as usize,
                    history_on,
                    history_limit,
                );
                tab.sql_result = None;
                tab.sql_error = None;
                self.status_message = Some((
                    format!("SQL applied on server: {n} row(s) affected"),
                    std::time::Instant::now(),
                ));
            }
            ServerQueryDone::Failed(msg) => {
                tab.sql_error = Some(msg);
            }
        }
    }

    /// ATTACH a saved live-database connection into the active tab's SQL
    /// workspace. Synchronous: the DuckDB extension handshake runs on the UI
    /// thread, like the file ATTACH beside it.
    // ponytail: blocks the UI for the handshake (and the whole import for
    // SQL Server); move onto a worker if users attach slow servers.
    fn workspace_attach_db_connection(&mut self, conn_id: &str, scope: &octa::sql::AttachScope) {
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
        self.union_progress = Some(super::state::UnionProgress::new(
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
                match super::cloud_browser::fetch_object_to_temp(&conn, key, name, &settings) {
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
                p.push(super::cloud_browser::CloudOpenResult::WorkspaceReady {
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
    fn union_cloud_files(
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
    fn free_ws_name(tab: &TabState, stem: &str) -> String {
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

    fn run_workspace_query(&mut self, ctx: &egui::Context) {
        // Captured before the `tab` borrow; used by the post-mutation row-diff
        // highlight below.
        let diff_enabled = self.settings.sql_row_diff_highlight_enabled;
        let diff_secs = self.settings.sql_row_diff_highlight_secs;
        let readonly = self.is_readonly();
        let history_on = self.settings.sql_history_enabled;
        let history_limit = self.settings.sql_history_limit;
        let tab = &mut self.tabs[self.active_tab];
        let query = tab.sql_query.clone();
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
                tab.sql_error = Some(e.to_string());
                return;
            }
            started = std::time::Instant::now();
            ws.execute(&query)
        };
        // Before the match, so a failed query is timed too: the interesting
        // case is the one that ran for a minute and then errored.
        tab.sql_last_duration_ms = Some(started.elapsed().as_millis() as u64);
        match outcome {
            // CREATE TABLE / VIEW: the statement's table becomes a new tab,
            // named after it. Nothing folds back into this tab, so no
            // read-only refusal either: a new tab is not an edit, and this is
            // how a first table gets made from an empty tab with no `data`.
            Ok(qo) if qo.created.is_some() => {
                let name = qo.created.clone().unwrap_or_default();
                let rows = qo.table.row_count();
                record_sql_history(tab, &query, started, rows, history_on, history_limit);
                tab.sql_error = None;
                tab.sql_last_query = query;
                let mut new_tab = TabState::new(self.settings.default_search_mode);
                new_tab.table = qo.table;
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
            Ok(qo) => match qo.kind {
                octa::sql::QueryKind::Select => {
                    let rows = qo.table.row_count();
                    tab.sql_result = Some(qo.table);
                    tab.sql_error = None;
                    record_sql_history(tab, &query, started, rows, history_on, history_limit);
                    tab.sql_last_query = query;
                }
                octa::sql::QueryKind::Mutation => {
                    // Read-only (mode or a live-database tab): the mutation
                    // ran against the workspace temp table only; refuse to
                    // fold it back into the tab.
                    if readonly {
                        tab.sql_error = Some(octa::i18n::t("db.tab_readonly_note"));
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
            // Remote table lists can drift; refresh re-queries them too.
            ws.invalidate_attached_cache();
            if snapshot.col_count() == 0 {
                tab.sql_error = None;
            } else if let Err(e) = ws.set_active_table(&snapshot) {
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

    fn workspace_rename_table(&mut self, from: &str, to: &str) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(ws) = tab.sql_workspace.as_mut()
            && let Err(e) = ws.rename_table(from, to)
        {
            // `{e:#}`: the reason (name taken, empty) is the inner context.
            tab.sql_error = Some(format!("{e:#}"));
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
/// surface through `tab.sql_error`.
/// Which stored history a tab's queries belong to: the connection for a
/// database tab, the file for a file-backed workspace, and one shared scratch
/// list for a tab that has neither.
pub(crate) fn sql_history_scope(tab: &TabState) -> String {
    if let Some(origin) = tab.db_origin.as_ref() {
        return octa::sql::history::db_scope(&origin.conn_id);
    }
    match tab.table.source_path.as_deref() {
        Some(path) => octa::sql::history::file_scope(path),
        None => "scratch".to_string(),
    }
}

/// Record an executed query, in the tab's list and in the persisted history.
/// Skips blank queries; a no-op when the user has history switched off.
///
/// `enabled` and `limit` are read from settings by the caller, because every
/// call site already holds the tab mutably.
fn record_sql_history(
    tab: &mut TabState,
    query: &str,
    started: std::time::Instant,
    rows: usize,
    enabled: bool,
    limit: usize,
) {
    let entry = octa::sql::history::SqlHistoryEntry {
        query: query.trim().to_string(),
        at_unix: octa::sql::history::now_unix(),
        duration_ms: started.elapsed().as_millis() as u64,
        rows,
    };
    let scope = sql_history_scope(tab);
    // The tab's copy is what the History menu draws; the store is what survives
    // closing it. Same fold, so the two never diverge.
    octa::sql::history::fold(&mut tab.sql_history, entry.clone(), limit);
    octa::sql::history::record(&scope, entry, enabled, limit);
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
    // First use of this tab's workspace: pull in whatever was recorded against
    // its connection or file previously, so the History menu is useful from the
    // moment the panel opens rather than only after this session's first run.
    if tab.sql_history.is_empty() {
        tab.sql_history = octa::sql::history::load(&sql_history_scope(tab));
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
