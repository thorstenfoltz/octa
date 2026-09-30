//! The SQL panel UI: editor, workspace list, inspector, results.

use super::*;

impl OctaApp {
    /// Whether the active tab shows the SQL panel at all.
    pub(crate) fn sql_panel_visible(&self) -> bool {
        let tab = &self.tabs[self.active_tab];
        // No col_count gate: an empty tab can still ATTACH saved connections
        // and query servers directly (no `data` table then).
        tab.sql_panel_open && tab.view_mode == ViewMode::Table
    }

    /// The docked panel. A maximised one is drawn by the central panel
    /// instead ([`Self::draw_sql_panel`] with `maximised`), so the chat and
    /// side panels rendered after this still get their space.
    pub(crate) fn render_sql_panel(&mut self, parent_ui: &mut egui::Ui) {
        if !self.sql_panel_visible() || self.tabs[self.active_tab].sql_maximised {
            return;
        }
        self.draw_sql_panel(parent_ui, false);
    }

    /// Draw the panel: docked at the configured edge, or straight into
    /// `parent_ui` (the central area) when `maximised`.
    pub(crate) fn draw_sql_panel(&mut self, parent_ui: &mut egui::Ui, maximised: bool) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        // Switched off in Settings: the file is already gone, the menu follows.
        if !self.settings.sql_history_enabled {
            self.sql_history.clear();
        }
        let position = self.settings.sql_panel_position;
        let mut sql_action = view_modes::SqlAction::default();
        let editor_font = self.settings.sql_editor_font;
        let autocomplete = self.settings.sql_autocomplete;
        let row_limit = self.settings.sql_default_row_limit;

        // The other open tabs become queryable tables before the snapshot is
        // taken, so the workspace listing shows them the frame the panel opens.
        ensure_workspace(&mut self.tabs[self.active_tab]);
        self.sync_open_tabs_into_workspace();

        // Build a lightweight workspace snapshot up front so the renderer
        // doesn't need to borrow the workspace mutably while drawing.
        let (workspace_rows, workspace_attachments) = {
            let tab = &self.tabs[self.active_tab];
            workspace_snapshot(tab)
        };

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
                let parts = crate::app::db_browser::split_path(path)
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
        let server_identifiers = self.server_autocomplete_identifiers(ctx);
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
        let auto_registered = tab.sql_auto_registered.clone();
        let show_auto_register_notice = self.settings.show_sql_auto_register_notice;
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
                    partial_rows,
                    editor_font,
                    workspace_tables: &workspace_rows,
                    workspace_attachments: &workspace_attachments,
                    inspector_selection: inspector_selection_owned.as_ref(),
                    inspector_entry: inspector_entry_owned.as_ref(),
                    history: &self.sql_history,
                    server_running,
                    db_connections: db_connections.clone(),
                    cloud_connections: cloud_connections.clone(),
                    auto_registered: &auto_registered,
                    show_auto_register_notice,
                    chat_profile_available,
                    ask_profiles: ask_profiles.clone(),
                    extra_identifiers: &server_identifiers,
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
        // One id per position. `PanelState` is keyed on the id alone and
        // stores a Rect, so a single shared "sql_panel" meant the width
        // dragged while docked Left came back as the height when docked
        // Bottom.
        if maximised {
            body(parent_ui);
        } else {
            match position {
                ui::settings::SqlPanelPosition::Bottom => egui::Panel::bottom("sql_panel_bottom"),
                ui::settings::SqlPanelPosition::Top => egui::Panel::top("sql_panel_top"),
                ui::settings::SqlPanelPosition::Left => egui::Panel::left("sql_panel_left"),
                ui::settings::SqlPanelPosition::Right => egui::Panel::right("sql_panel_right"),
            }
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .show(parent_ui, &mut body);
        }
        if sql_action.format {
            self.format_sql_editor(ctx);
        }
        if sql_action.clear {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql.result = None;
            tab.sql.result_total = None;
            tab.sql.error = None;
        }
        if sql_action.run {
            let tab = &self.tabs[self.active_tab];
            let query = view_modes::sql::query_to_run(ctx, &tab.sql);
            if tab.sql_target.is_some() {
                self.run_server_query(ctx, query);
            } else {
                self.queue_local_query(ctx, query);
            }
        }
        if sql_action.add_pane {
            self.tabs[self.active_tab].add_sql_pane();
        }
        if let Some(i) = sql_action.close_pane {
            self.tabs[self.active_tab].close_sql_pane(i);
        }
        if sql_action.toggle_maximise {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_maximised = !tab.sql_maximised;
        }
        if sql_action.result_to_tab {
            self.open_sql_result_as_tab(ctx);
        }
        if sql_action.cancel_server {
            self.cancel_server_query();
        }
        if sql_action.export {
            self.export_sql_result();
        }
        if let Some(pane_id) = sql_action.load_more_rows {
            self.load_more_sql_result_rows(pane_id);
        }
        if sql_action.dismiss_auto_register_notice {
            self.settings.show_sql_auto_register_notice = false;
            self.settings.save();
        }
        if sql_action.close {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_panel_open = false;
            tab.sql_maximised = false;
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
            let path = crate::app::db_browser::join_path(&refs);
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
            self.copy_to_clipboard(ctx, q);
        }
        if let Some(q) = sql_action.insert_qualified {
            self.insert_select_into_editor(&q);
        }
        if let Some(q) = sql_action.run_qualified {
            self.run_select_for_inspector(&q, ctx);
        }
        if let Some(q) = sql_action.recall_query {
            self.tabs[self.active_tab].sql.query = q;
            self.tabs[self.active_tab].sql.focus_pending = true;
        }
        if let Some(question) = sql_action.ask {
            self.start_ask_sql(ctx, question);
        }
        if let Some(q) = sql_action.insert_snippet {
            self.tabs[self.active_tab].sql.query = q;
            self.tabs[self.active_tab].sql.focus_pending = true;
        }
        if sql_action.save_snippet {
            let query = self.tabs[self.active_tab].sql.query.trim().to_string();
            if !query.is_empty() {
                self.sql_snippet_save = Some(crate::app::state::SqlSnippetDraft {
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
            crate::app::sql_snippets::save(&self.sql_snippets);
        }
        if sql_action.clear_history {
            self.sql_history.clear();
            octa::sql::history::forget_everything();
        }
        if sql_action.open_snippets_window {
            self.sql_snippets_window_open = !self.sql_snippets_window_open;
        }
    }

    /// Catalog, schema, table and column names on the live server the editor
    /// targets, for its autocomplete. Read from the sidebar's listing cache
    /// and [`ServerColumns`]; whatever is missing is fetched in the background,
    /// so the names arrive a moment later rather than the interface waiting on
    /// the network.
    ///
    /// What gets fetched follows the query text: every dotted name whose
    /// parts are known (`sales.` lists the `sales` schema), and the columns of
    /// every listed table the query mentions. A table the query does not name
    /// costs nothing.
    fn server_autocomplete_identifiers(&mut self, ctx: &egui::Context) -> Vec<String> {
        use crate::app::db_browser::{DbListState, join_path};
        let tab = &self.tabs[self.active_tab];
        let Some(conn) = tab
            .sql_target
            .as_deref()
            .and_then(|id| self.find_db_conn(id))
        else {
            return Vec::new();
        };
        let origin = server_origin(tab).cloned();
        // Quotes stripped first, so `"sales"."orders"` reads as one dotted
        // name rather than three fragments.
        let text: String = tab
            .sql
            .query
            .chars()
            .filter(|c| !matches!(c, '"' | '`' | '[' | ']'))
            .collect();
        let dotted: Vec<Vec<&str>> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .map(|t| t.split('.').filter(|p| !p.is_empty()).collect::<Vec<_>>())
            .filter(|p: &Vec<&str>| !p.is_empty())
            .collect();
        let words: std::collections::HashSet<String> =
            dotted.iter().flatten().map(|w| w.to_lowercase()).collect();

        // Ready listings of this connection, path -> (names, holds tables).
        let listed: std::collections::HashMap<String, (Vec<String>, bool)> = self
            .db_browser
            .listings
            .lock()
            .map(|m| {
                m.iter()
                    .filter(|((id, _), _)| *id == conn.id)
                    .filter_map(|((_, path), state)| match state {
                        DbListState::Catalogs(v) | DbListState::Schemas(v) => {
                            Some((path.clone(), (v.clone(), false)))
                        }
                        DbListState::Tables(v) => Some((path.clone(), (v.clone(), true))),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut want = vec![String::new()];
        if let Some(o) = &origin {
            let mut parts: Vec<&str> = o.catalog.iter().map(String::as_str).collect();
            if !parts.is_empty() {
                want.push(join_path(&parts));
            }
            parts.push(&o.schema);
            want.push(join_path(&parts));
        }
        // Parts a table path has: catalog + schema, or just schema.
        let depth = if conn.engine.has_catalogs() { 2 } else { 1 };
        for parts in &dotted {
            let mut path: Vec<String> = Vec::new();
            for part in parts.iter().take(depth) {
                let parent = join_path(&path.iter().map(String::as_str).collect::<Vec<_>>());
                let Some(name) = listed.get(&parent).and_then(|(names, _)| {
                    names.iter().find(|n| n.eq_ignore_ascii_case(part)).cloned()
                }) else {
                    break;
                };
                path.push(name);
                want.push(join_path(
                    &path.iter().map(String::as_str).collect::<Vec<_>>(),
                ));
            }
        }
        for path in want {
            self.ensure_db_listing(ctx, conn.id.clone(), path);
        }

        let mut out: Vec<String> = Vec::new();
        if let Some(o) = &origin {
            out.push(o.schema.clone());
            out.push(o.table.clone());
            out.extend(o.catalog.clone());
        }
        for (path, (names, tables)) in &listed {
            out.extend(names.iter().cloned());
            if !tables {
                continue;
            }
            for table in names.iter().filter(|n| words.contains(&n.to_lowercase())) {
                let key = (conn.id.clone(), path.clone(), table.clone());
                let cached = self
                    .server_columns
                    .lock()
                    .ok()
                    .map(|m| m.get(&key).cloned());
                match cached {
                    Some(Some(Some(cols))) => out.extend(cols),
                    Some(Some(None)) | None => {}
                    Some(None) => self.start_server_columns(ctx, &conn, key),
                }
            }
        }
        out
    }

    /// Fetch one server table's column names on a worker, into
    /// [`ServerColumns`]. The slot is claimed first (`None`), so a table is
    /// asked about once; a failure leaves it at `None` for the session rather
    /// than retrying every frame.
    fn start_server_columns(
        &self,
        ctx: &egui::Context,
        conn: &octa::db::DbConnection,
        key: (String, String, String),
    ) {
        if let Ok(mut m) = self.server_columns.lock() {
            m.insert(key.clone(), None);
        } else {
            return;
        }
        let conn = conn.clone();
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let slot = self.server_columns.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let (_, path, table) = &key;
            let parts = crate::app::db_browser::split_path(path);
            let (catalog, schema) = match parts.as_slice() {
                [cat, sch] => (Some(*cat), *sch),
                [sch] => (None, *sch),
                _ => return,
            };
            let sql = octa::db::table_metadata_sql(conn.engine, catalog, schema, table);
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
            let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
            let cols = cache
                .with_conn(&conn, secret.as_deref(), ssh.as_deref(), |c| c.query(&sql))
                .map(|t| octa::db::column_names_from_metadata(&t));
            if let Ok(cols) = cols
                && let Ok(mut m) = slot.lock()
            {
                m.insert(key, Some(cols));
            }
            ctx.request_repaint();
        });
    }

    pub(super) fn workspace_select_inspector(&mut self, sel: Option<InspectorTarget>) {
        let tab = &mut self.tabs[self.active_tab];
        tab.sql_inspector_selection = sel;
        self.workspace_refill_inspector_cache();
    }

    pub(super) fn workspace_refill_inspector_cache(&mut self) {
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

    pub(super) fn copy_to_clipboard(&mut self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text.clone());
        self.status_message = Some((format!("Copied `{text}`"), std::time::Instant::now()));
    }

    pub(super) fn insert_select_into_editor(&mut self, qualified: &str) {
        // The user's own default limit, not a hardcoded 100: the setting says
        // how many rows they want a browse query to ask for.
        let limit = self.settings.sql_default_row_limit;
        let tab = &mut self.tabs[self.active_tab];
        let snippet = format!("SELECT * FROM {qualified} LIMIT {limit};");
        if tab.sql.query.is_empty() {
            tab.sql.query = snippet;
        } else {
            if !tab.sql.query.ends_with('\n') {
                tab.sql.query.push('\n');
            }
            tab.sql.query.push_str(&snippet);
        }
    }

    pub(super) fn run_select_for_inspector(&mut self, qualified: &str, ctx: &egui::Context) {
        let limit = self.settings.sql_default_row_limit;
        let query = format!("SELECT * FROM {qualified} LIMIT {limit}");
        self.tabs[self.active_tab].sql.query = query.clone();
        self.queue_local_query(ctx, query);
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
}
