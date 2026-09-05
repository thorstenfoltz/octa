use crate::app::state::TabState;
use crate::ui::settings::SqlPanelPosition;

use eframe::egui;
use octa::data::CellValue;
use octa::ui::settings::SqlEditorFont;
use octa::ui::status_bar::format_number;

/// Resolve the configured `SqlEditorFont` into an `egui::FontFamily`. The
/// `JetBrainsMono` variant points at the bundled named family registered in
/// `apply_fonts`; `MatchUiFont` falls back to the active style's body font;
/// `SystemMonospace` uses egui's built-in mono family.
fn sql_font_family(font: SqlEditorFont, ui: &egui::Ui) -> egui::FontFamily {
    match font {
        SqlEditorFont::JetBrainsMono => egui::FontFamily::Name(std::sync::Arc::from("sql_mono")),
        SqlEditorFont::SystemMonospace => egui::FontFamily::Monospace,
        SqlEditorFont::MatchUiFont => ui.style().text_styles[&egui::TextStyle::Body]
            .family
            .clone(),
    }
}

/// User actions emitted by the SQL view in a single frame. The fields beyond
/// `run` / `clear` / `export` / `close` drive the per-tab SQL workspace:
/// adding extra tables, ATTACHing databases, removing them, and writing
/// query results back to a DuckDB or SQLite file.
#[derive(Debug, Clone, Default)]
pub struct SqlAction {
    pub run: bool,
    pub clear: bool,
    /// User chose **Clear history** in the History menu: forget every recorded
    /// query for this tab's connection or file.
    pub clear_history: bool,
    pub export: bool,
    /// User clicked the × button in the panel header. The caller flips
    /// `tab.sql_panel_open` to false, hiding the panel until the user
    /// reopens it from **Analyse -> SQL**.
    pub close: bool,
    /// User clicked **+ Add table...**. Opens a multi-file picker that
    /// registers every chosen file under a sanitised, de-duplicated name.
    pub add_tables: bool,
    /// User clicked **Attach database...**. Opens a single-file picker
    /// (DuckDB / SQLite) and ATTACHes the file under a default alias.
    pub attach_db: bool,
    /// User clicked **[refresh]** next to `data` (or wants the workspace
    /// to re-register the active tab's table from the live edited state).
    pub refresh_active: bool,
    /// User clicked **[×]** next to a registered table.
    pub remove_table: Option<String>,
    /// User clicked **[detach]** next to an ATTACH-ed database.
    pub detach_alias: Option<String>,
    /// User clicked **Write result to DB...**. The panel opens the write-back
    /// dialog which composes the actual `WriteTarget`.
    pub open_write_back: bool,
    /// User pressed Ask with a question in the plain-language box. The panel
    /// fires one request; the answer is spliced into the editor, never run.
    pub ask: Option<String>,
    /// User selected a new entry in the workspace tree (or cleared the
    /// selection). The panel updates `tab.sql_inspector_selection` and
    /// triggers a fresh introspection fetch (cached on `TabState` so the
    /// next frame doesn't re-query).
    pub select_inspector: Option<Option<crate::app::sql_panel::InspectorTarget>>,
    /// User clicked **Insert** in the inspector. The panel appends a SELECT
    /// statement (`SELECT * FROM <qualified> LIMIT 100;`) into the editor.
    pub insert_qualified: Option<String>,
    /// User clicked **Run** in the inspector. The panel replaces the editor
    /// content with `SELECT * FROM <qualified> LIMIT N` and runs it.
    pub run_qualified: Option<String>,
    /// User clicked **Copy name**. The panel copies the qualified name to
    /// the system clipboard.
    pub copy_qualified: Option<String>,
    /// User toggled the open/closed state of an attached schema group.
    /// String is the tree key (`alias` or `alias::schema`).
    pub toggle_tree_key: Option<String>,
    /// User picked a recent query from the History dropdown (the query text).
    pub recall_query: Option<String>,
    /// User picked a saved snippet (the snippet's query text) to load.
    pub insert_snippet: Option<String>,
    /// User clicked **Save current query as snippet...**.
    pub save_snippet: bool,
    /// User deleted a saved snippet by name.
    pub delete_snippet: Option<String>,
    /// User clicked the **Snippets** button to open the snippet manager window.
    pub open_snippets_window: bool,
    /// User clicked **Cancel** on an in-flight "Run on server" query.
    pub cancel_server: bool,
    /// User picked a saved live-database connection to ATTACH (its id).
    pub attach_db_connection: Option<String>,
}

/// Persistent id of the SQL editor TextEdit. Exposed so the global keyboard
/// handler in `main.rs` can tell whether the editor currently has focus.
pub fn editor_id() -> egui::Id {
    egui::Id::new("sql_editor")
}

/// SQL keywords offered by the autocomplete dropdown.
pub const SQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP BY",
    "ORDER BY",
    "LIMIT",
    "OFFSET",
    "HAVING",
    "DISTINCT",
    "JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "INNER JOIN",
    "OUTER JOIN",
    "FULL JOIN",
    "CROSS JOIN",
    "ON",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IS",
    "NULL",
    "IN",
    "BETWEEN",
    "LIKE",
    "ILIKE",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "UNION",
    "UNION ALL",
    "INTERSECT",
    "EXCEPT",
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "TABLE",
    "DROP",
    "ALTER",
    "ADD",
    "COLUMN",
    "WITH",
    "ASC",
    "DESC",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "CAST",
    "COALESCE",
    "TRUE",
    "FALSE",
    "data",
];

/// Extract the partial token to the left of the cursor so it can be used as an
/// autocomplete prefix. Tokens are sequences of word characters; anything else
/// terminates the prefix.
pub fn current_prefix_at(text: &str, cursor_byte: usize) -> (usize, &str) {
    let cursor = cursor_byte.min(text.len());
    let bytes = text.as_bytes();
    let mut start = cursor;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    (start, &text[start..cursor])
}

/// Filter keywords + column names by a case-insensitive prefix match. Column
/// names win ties over keywords. Returns at most `max` entries.
pub fn collect_suggestions(prefix: &str, columns: &[String], max: usize) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let pfx = prefix.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for col in columns {
        if col.to_lowercase().starts_with(&pfx) {
            out.push(col.clone());
        }
    }
    for kw in SQL_KEYWORDS {
        if kw.to_lowercase().starts_with(&pfx) && !out.iter().any(|s| s == kw) {
            out.push((*kw).to_string());
        }
    }
    out.truncate(max);
    out
}

/// Lightweight view of one registered workspace table, passed to the SQL
/// panel renderer so it can list the tab's current workspace without
/// borrowing the `SqlWorkspace` directly (the workspace is consumed by
/// the parent loop's match on the returned `SqlAction`).
#[derive(Debug, Clone)]
pub struct WorkspaceRow {
    pub sql_name: String,
    pub origin: String,
    pub row_count: usize,
    /// `true` for the conventional `data` table; the panel renders it
    /// with a [refresh] affordance instead of a remove button.
    pub is_active: bool,
}

/// Lightweight view of one ATTACH-ed database, passed alongside
/// [`WorkspaceRow`]s for the workspace section.
#[derive(Debug, Clone)]
pub struct WorkspaceAttachment {
    pub alias: String,
    pub source: String,
    pub kind_label: &'static str,
    /// Native ATTACH versus fallback-loaded-as-tables.
    pub native: bool,
    pub table_count: usize,
    /// Per-schema groupings of the attached tables. Empty for fallback
    /// attachments. Pre-computed in `workspace_snapshot` so the renderer
    /// doesn't talk to the workspace directly.
    pub schemas: Vec<WorkspaceAttachmentSchema>,
}

/// Inner-table grouping inside a [`WorkspaceAttachment`].
#[derive(Debug, Clone)]
pub struct WorkspaceAttachmentSchema {
    pub schema: String,
    pub tables: Vec<WorkspaceAttachmentTable>,
}

/// Single attached-database table shown in the workspace tree.
#[derive(Debug, Clone)]
pub struct WorkspaceAttachmentTable {
    pub schema: String,
    pub table: String,
    pub row_count: Option<usize>,
}

/// Bundle of every per-call parameter that doesn't fit on the renderer's
/// natural argument list. Avoids the clippy lint on a 9-argument function
/// without losing the GUI / library separation (the workspace itself
/// stays in the panel, the renderer only sees this passive view).
pub struct SqlViewContext<'a> {
    pub autocomplete_enabled: bool,
    pub default_row_limit: usize,
    pub panel_position: SqlPanelPosition,
    pub partial_rows: Option<(usize, usize)>,
    pub editor_font: octa::ui::settings::SqlEditorFont,
    pub workspace_tables: &'a [WorkspaceRow],
    pub workspace_attachments: &'a [WorkspaceAttachment],
    /// Currently selected inspector target. Drives both the highlight in the
    /// workspace tree on the left and the detail pane on the right.
    pub inspector_selection: Option<&'a crate::app::sql_panel::InspectorTarget>,
    /// Cached introspection result for the current selection. `None` while
    /// the panel waits for the first fetch; `Some(Ok)` or `Some(Err)`
    /// otherwise.
    pub inspector_entry: Option<&'a crate::app::state::InspectorCacheEntry>,
    /// Name of the live-database connection the tab was opened from, when
    /// any: enables the "Run on: server | local" toggle.
    pub server_conn_name: Option<String>,
    /// Whether a "Run on server" query is currently in flight.
    pub server_running: bool,
    /// Saved live-database connections as (id, name), for the attach menu.
    pub db_connections: Vec<(String, String)>,
    /// Whether at least one chat profile is configured, so the Ask box can be
    /// offered. Passed in because the view layer does not read settings.
    pub chat_profile_available: bool,
}

/// Render a split-pane SQL editor (top) and result table (bottom).
/// The current tab's table is exposed in queries as `data`.
/// `partial_rows` carries `(loaded, total)` when the table isn't fully loaded.
/// Row-counter text for a result grid. Fetches stop exactly at the
/// initial-load row cap, so a result sitting on the cap means truncation.
///
/// `took_ms` appends how long the query ran. It is spelled in `ms` / `s`
/// rather than a translated phrase: both are SI symbols, so the line needs no
/// thirty-second locale key to say "in".
fn result_rows_label(rows: usize, took_ms: Option<u64>) -> String {
    let counted = if rows >= octa::formats::initial_load_rows() {
        format!("{} {}", rows, octa::i18n::t("sql.result_rows_capped"))
    } else {
        format!("{} {}", rows, octa::i18n::t("sql.result_rows"))
    };
    match took_ms {
        Some(ms) => format!("{counted} ({})", format_duration(ms)),
        None => counted,
    }
}

/// A query duration a person can read at a glance: milliseconds while they
/// stay small, seconds once they do not.
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

pub fn render_sql_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    ctx_args: SqlViewContext<'_>,
) -> SqlAction {
    let SqlViewContext {
        autocomplete_enabled,
        default_row_limit,
        panel_position,
        partial_rows,
        editor_font,
        workspace_tables,
        workspace_attachments,
        inspector_selection,
        inspector_entry,
        server_conn_name,
        server_running,
        db_connections,
        chat_profile_available,
    } = ctx_args;
    let mut action = SqlAction::default();
    let editor_id = editor_id();

    // Calm hover feedback for the whole panel: several themes paint hovered
    // widgets 1-3px larger (`widgets.hovered.expansion`), which in this dense
    // grid of cells, rows and small buttons reads as everything twitching
    // under the pointer. Zeroing it here is local to the SQL panel; hover
    // colours still change, nothing grows.
    {
        let style = ui.style_mut();
        style.visuals.widgets.hovered.expansion = 0.0;
        style.visuals.widgets.active.expansion = 0.0;
    }

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(octa::i18n::t("sql.query_against_data")).strong());
        ui.add_space(8.0);
        // Live-database tab: pick where the query runs (server default).
        if let Some(conn_name) = &server_conn_name {
            ui.label(octa::i18n::t("sql.run_on"));
            if ui
                .selectable_label(tab.sql_run_on_server, conn_name)
                .on_hover_text(octa::i18n::t("sql.run_on_server_hint"))
                .clicked()
            {
                tab.sql_run_on_server = true;
            }
            if ui
                .selectable_label(!tab.sql_run_on_server, octa::i18n::t("sql.run_local"))
                .on_hover_text(octa::i18n::t("sql.run_local_hint"))
                .clicked()
            {
                tab.sql_run_on_server = false;
            }
            ui.add_space(4.0);
        }
        if server_running {
            ui.add(egui::Spinner::new().size(12.0));
            if ui.button(octa::i18n::t("common.cancel")).clicked() {
                action.cancel_server = true;
            }
        } else if ui
            .button(octa::i18n::t("sql.run"))
            .on_hover_text(octa::i18n::t("sql.run_hint"))
            .clicked()
        {
            action.run = true;
        }
        if ui.button(octa::i18n::t("sql.clear_result")).clicked() {
            action.clear = true;
        }

        // History: queries run against this connection or file, kept between
        // sessions. Each row carries what the run cost, which is what makes the
        // list worth reading rather than only re-runnable.
        if !tab.sql_history.is_empty() {
            ui.menu_button(octa::i18n::t("sql.history"), |ui| {
                ui.set_min_width(280.0);
                for h in &tab.sql_history {
                    // One-line preview; full query on hover. Counted in chars,
                    // not bytes: `&preview[..57]` panicked on any query
                    // holding a multi-byte character (a city called Muenchen
                    // spelled properly was enough).
                    let flat = h.query.replace('\n', " ");
                    let preview = if flat.chars().count() > 60 {
                        format!("{}...", flat.chars().take(57).collect::<String>())
                    } else {
                        flat
                    };
                    let cost = format!(
                        "{} {} - {} ms",
                        octa::ui::status_bar::format_number(h.rows),
                        octa::i18n::t("sql.history_rows"),
                        octa::ui::status_bar::format_number(h.duration_ms as usize)
                    );
                    ui.horizontal(|ui| {
                        if ui.button(preview).on_hover_text(&h.query).clicked() {
                            action.recall_query = Some(h.query.clone());
                            ui.close();
                        }
                        // `weak` rather than a theme colour: this view has no
                        // ThemeColors in scope, and weak text is exactly the
                        // "secondary detail" role wanted here.
                        ui.label(egui::RichText::new(cost).size(11.0).weak());
                    });
                }
                ui.separator();
                if ui
                    .button(octa::i18n::t("sql.history_clear"))
                    .on_hover_text(octa::i18n::t("sql.history_clear_hint"))
                    .clicked()
                {
                    action.clear_history = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text(octa::i18n::t("sql.history_hint"));
        }

        // Snippets: opens the persistent named-query manager window.
        if ui
            .button(octa::i18n::t("sql.snippets"))
            .on_hover_text(octa::i18n::t("sql.snippets_hint"))
            .clicked()
        {
            action.open_snippets_window = true;
        }

        // Ask: plain language in, one SELECT out, into the editor at the
        // cursor. Never runs. Disabled with a reason rather than hidden, so
        // the control explains itself.
        let has_columns = tab.table.col_count() > 0;
        let ask_enabled = chat_profile_available && has_columns;
        let ask_reason = if !chat_profile_available {
            octa::i18n::t("sql.ask_needs_profile")
        } else if !has_columns {
            octa::i18n::t("sql.ask_no_columns")
        } else {
            octa::i18n::t("sql.ask_hint")
        };
        ui.add_enabled_ui(ask_enabled, |ui| {
            let box_resp = ui
                .add(
                    egui::TextEdit::singleline(&mut tab.sql_ask_input)
                        .desired_width(220.0)
                        .hint_text(octa::i18n::t("sql.ask_placeholder")),
                )
                .on_hover_text(ask_reason.clone())
                .on_disabled_hover_text(ask_reason.clone());
            let submitted = box_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui
                .button(octa::i18n::t("sql.ask"))
                .on_hover_text(ask_reason.clone())
                .on_disabled_hover_text(ask_reason)
                .clicked();
            if (submitted || clicked) && !tab.sql_ask_input.trim().is_empty() {
                action.ask = Some(tab.sql_ask_input.clone());
            }
        });

        let has_result = tab.sql_result.as_ref().is_some_and(|t| t.col_count() > 0);
        ui.add_enabled_ui(has_result, |ui| {
            if ui
                .button(octa::i18n::t("sql.export"))
                .on_hover_text(octa::i18n::t("sql.export_hint"))
                .clicked()
            {
                action.export = true;
            }
            if ui
                .button(octa::i18n::t("sql.write_to_db"))
                .on_hover_text(octa::i18n::t("sql.write_to_db_hint"))
                .clicked()
            {
                action.open_write_back = true;
            }
        });
        if let Some(rows) = tab.sql_result.as_ref().map(|t| t.row_count()) {
            ui.add_space(12.0);
            ui.label(result_rows_label(rows, tab.sql_last_duration_ms));
        }
        // Close (×) button on the right - flips `sql_panel_open` to false.
        // The Analyse dropdown is two clicks away, so without an in-panel
        // close the user has to fiddle to dismiss it.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui::RichText::new("\u{00d7}").size(16.0).strong())
                .on_hover_text(octa::i18n::t("sql.close_hint"))
                .clicked()
            {
                action.close = true;
            }
        });
    });
    ui.add_space(4.0);

    render_workspace_section(
        ui,
        tab,
        &WorkspaceData {
            tables: workspace_tables,
            attachments: workspace_attachments,
            db_connections: &db_connections,
        },
        inspector_selection,
        inspector_entry,
        &mut action,
    );
    ui.add_space(4.0);

    // --- Compute autocomplete state BEFORE rendering the TextEdit so we can
    // intercept arrow / Enter / Tab / Escape keys while the popup is visible.
    let editor_focused = ui.ctx().memory(|m| m.focused() == Some(editor_id));
    let mut suggestions: Vec<String> = Vec::new();
    let mut prefix_start = 0usize;
    let mut prefix_len = 0usize;
    if autocomplete_enabled && editor_focused {
        // Suggestions draw from every identifier the SQL workspace can see -
        // not just the active `data` table's columns. The workspace's
        // `information_schema` query covers registered table names,
        // attachment aliases, attached-database tables, and every column of
        // every visible table; we merge the active tab's column list on top
        // so freshly-added columns surface even before the user clicks the
        // workspace [refresh] button.
        let mut idents: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        if let Some(ws) = tab.sql_workspace.as_ref() {
            idents.extend(ws.collect_autocomplete_identifiers());
        }
        idents.sort();
        idents.dedup();
        let columns = idents;
        let cursor_byte = egui::TextEdit::load_state(ui.ctx(), editor_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let char_idx = r.primary.index.0;
                tab.sql_query
                    .char_indices()
                    .nth(char_idx)
                    .map(|(i, _)| i)
                    .unwrap_or_else(|| tab.sql_query.len())
            })
            .unwrap_or(tab.sql_query.len());
        let (pstart, pstr) = current_prefix_at(&tab.sql_query, cursor_byte);
        prefix_start = pstart;
        prefix_len = pstr.len();
        if !pstr.is_empty() {
            suggestions = collect_suggestions(pstr, &columns, 8);
        }
    }

    // Clamp selection against the live list.
    if !suggestions.is_empty() {
        if tab.sql_ac_selected >= suggestions.len() {
            tab.sql_ac_selected = 0;
        }
    } else {
        tab.sql_ac_selected = 0;
    }

    // Consume the popup-specific keys *only while the popup is visible*
    // (`popup_active`): Up/Down move the selection, Enter or Tab accepts the
    // highlighted suggestion, Escape dismisses. Because consumption is gated on
    // `popup_active`, these keys keep their normal editor behaviour whenever the
    // popup is closed - Enter inserts a newline, arrows move the caret. The
    // popup only opens after the user types (`resp.changed()` sets
    // `sql_ac_visible`) and closes on Escape, so plain typing never loses keys.
    let popup_active = editor_focused && tab.sql_ac_visible && !suggestions.is_empty();
    let mut apply_suggestion: Option<String> = None;
    if popup_active {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                tab.sql_ac_selected = (tab.sql_ac_selected + 1) % suggestions.len();
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                tab.sql_ac_selected = if tab.sql_ac_selected == 0 {
                    suggestions.len() - 1
                } else {
                    tab.sql_ac_selected - 1
                };
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
            {
                apply_suggestion = suggestions.get(tab.sql_ac_selected).cloned();
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                tab.sql_ac_visible = false;
            }
        });
    }

    // Editor vs. result split. For outer Bottom docking the outer panel's
    // resize handle sits at its top edge - if the nested editor panel is also
    // docked at the top, its frame covers the outer resize strip and the user
    // can't drag the SQL panel taller from between the table and the box. To
    // avoid that collision, dock the *result* panel at the bottom in that
    // case and let the editor fill the remaining central area. For every
    // other outer position the top-docked editor split is fine.
    let total = ui.available_height();
    let default_editor_h = (total * 0.4).max(160.0).min((total - 80.0).max(120.0));
    let default_result_h = (total - default_editor_h).max(120.0);
    let mut editor_response: Option<egui::Response> = None;

    let render_result_area = |ui: &mut egui::Ui, tab: &mut TabState| {
        if let Some((loaded, total)) = partial_rows {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26a0} Result based on {} of {} rows currently loaded.",
                        format_number(loaded),
                        format_number(total),
                    ))
                    .small()
                    .color(egui::Color32::from_rgb(200, 160, 50)),
                );
            });
            ui.add_space(2.0);
        }
        if let Some(err) = &tab.sql_error {
            ui.colored_label(
                egui::Color32::from_rgb(220, 80, 80),
                format!("Error: {err}"),
            );
            ui.add_space(4.0);
        }
        if let Some(result) = tab.sql_result.as_ref() {
            // Row counter directly at the result, so the count is always in
            // view without adding COUNT(*) to the query or exporting.
            ui.label(
                egui::RichText::new(result_rows_label(
                    result.row_count(),
                    tab.sql_last_duration_ms,
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
            // Disjoint field borrows: the result table (read) + its selection
            // (write).
            render_result_table(ui, result, &mut tab.sql_result_selected);
        } else if tab.sql_error.is_none() {
            ui.label(egui::RichText::new(octa::i18n::t("sql.run_to_see")).weak());
        }
    };

    if matches!(panel_position, SqlPanelPosition::Bottom) {
        egui::Panel::bottom("sql_result_split")
            .resizable(true)
            .default_size(default_result_h)
            .min_size(80.0)
            .show(ui, |ui| {
                render_result_area(ui, tab);
            });
        editor_response = Some(draw_sql_editor(
            ui,
            tab,
            editor_id,
            default_row_limit,
            &mut action,
            editor_font,
        ));
    } else {
        egui::Panel::top("sql_editor_split")
            .resizable(true)
            .default_size(default_editor_h)
            .min_size(80.0)
            .show(ui, |ui| {
                editor_response = Some(draw_sql_editor(
                    ui,
                    tab,
                    editor_id,
                    default_row_limit,
                    &mut action,
                    editor_font,
                ));
            });
        ui.add_space(2.0);
    }
    let editor_response = editor_response.expect("editor panel always renders");

    // Right-click context menu on the SQL editor: selection-aware Copy +
    // whole-buffer Copy All.
    {
        let buffer = tab.sql_query.clone();
        editor_response.clone().context_menu(|ui| {
            let selection = super::text_ops::selected_text(ui.ctx(), editor_id, &buffer);
            let copy_label = if selection.is_some() {
                octa::i18n::t("header.copy")
            } else {
                octa::i18n::t("sql.copy_no_selection")
            };
            let copy_btn = ui.add_enabled(selection.is_some(), egui::Button::new(copy_label));
            if copy_btn.clicked() {
                if let Some(s) = selection {
                    ui.ctx().copy_text(s);
                }
                ui.close();
            }
            if ui.button(octa::i18n::t("view.copy_all")).clicked() {
                ui.ctx().copy_text(buffer.clone());
                ui.close();
            }
        });
    }

    // Apply the chosen suggestion: replace the current prefix, move the caret
    // to the end of the inserted text, refocus the editor.
    if let Some(sugg) = apply_suggestion {
        let end = prefix_start + prefix_len;
        if end <= tab.sql_query.len() {
            tab.sql_query.replace_range(prefix_start..end, &sugg);
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), editor_id) {
                let new_char_idx = tab.sql_query[..prefix_start + sugg.len()].chars().count();
                let ccursor = egui::text::CCursor::new(new_char_idx);
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                state.store(ui.ctx(), editor_id);
            }
            editor_response.request_focus();
        }
    }

    // --- Autocomplete popup ---
    if popup_active {
        let popup_id = ui.make_persistent_id("sql_autocomplete_popup");
        egui::Popup::from_response(&editor_response)
            .id(popup_id)
            .open(true)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                ui.set_min_width(220.0);
                // Force a high-contrast text color on the selected chip so the
                // variable name stays readable against the translucent selection
                // tint that some dark themes use for `selection.bg_fill`. egui's
                // default selectable_label inherits `widgets.inactive.fg_stroke`
                // for selected items, which produced barely-visible text in
                // Dark / Nord / Dracula / Gruvbox / DeepSea / Gentleman.
                let strong_color = if ui.visuals().dark_mode {
                    egui::Color32::WHITE
                } else {
                    ui.visuals().strong_text_color()
                };
                for (idx, s) in suggestions.iter().enumerate() {
                    let selected = idx == tab.sql_ac_selected;
                    let label = if selected {
                        egui::RichText::new(s).color(strong_color).strong()
                    } else {
                        egui::RichText::new(s)
                    };
                    let resp = ui.selectable_label(selected, label);
                    if resp.clicked() {
                        apply_suggestion_later(tab, prefix_start, prefix_len, s, ui.ctx());
                        editor_response.request_focus();
                    }
                    if resp.hovered() {
                        tab.sql_ac_selected = idx;
                    }
                }
            });
    }

    // For Bottom docking the result already rendered inside the bottom nested
    // panel above; for every other position the result fills whatever space
    // remains under the editor split.
    if !matches!(panel_position, SqlPanelPosition::Bottom) {
        ui.separator();
        render_result_area(ui, tab);
    }

    // Ctrl+C on a selected result cell. The editor only consumes the Copy event
    // while it is focused; clicking a result cell moved focus to that cell, so
    // here the event survives - copy the cell and consume it. When the editor is
    // focused instead, it handled its own copy and there's nothing to do.
    let editor_focused = ui.ctx().memory(|m| m.focused()) == Some(editor_id);
    let text_marked = octa::ui::text_selection::has_active_selection(ui.ctx());
    if !editor_focused
        && !text_marked
        && let Some((r, c)) = tab.sql_result_selected
    {
        let want_copy = ui.input_mut(|i| {
            let had = i
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut));
            i.events
                .retain(|e| !matches!(e, egui::Event::Copy | egui::Event::Cut));
            had
        });
        if want_copy
            && let Some(result) = &tab.sql_result
            && let Some(v) = result.get(r, c)
        {
            ui.ctx().copy_text(v.to_string());
        }
    }

    action
}

mod editor;
mod result;
mod workspace;

use editor::{apply_suggestion_later, draw_sql_editor};
use result::render_result_table;
use workspace::{WorkspaceData, render_workspace_section};

#[cfg(test)]
#[path = "../sql_tests.rs"]
mod tests;
