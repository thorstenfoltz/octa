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
}

/// Render a split-pane SQL editor (top) and result table (bottom).
/// The current tab's table is exposed in queries as `data`.
/// `partial_rows` carries `(loaded, total)` when the table isn't fully loaded.
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
    } = ctx_args;
    let mut action = SqlAction::default();
    let editor_id = editor_id();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(octa::i18n::t("sql.query_against_data")).strong());
        ui.add_space(8.0);
        if ui
            .button(octa::i18n::t("sql.run"))
            .on_hover_text(octa::i18n::t("sql.run_hint"))
            .clicked()
        {
            action.run = true;
        }
        if ui.button(octa::i18n::t("sql.clear_result")).clicked() {
            action.clear = true;
        }

        // History: recent queries run in this tab (session-only).
        if !tab.sql_history.is_empty() {
            ui.menu_button(octa::i18n::t("sql.history"), |ui| {
                ui.set_min_width(220.0);
                for q in &tab.sql_history {
                    // One-line preview; full query on hover.
                    let preview = q.replace('\n', " ");
                    let preview = if preview.len() > 60 {
                        format!("{}...", &preview[..57])
                    } else {
                        preview
                    };
                    if ui.button(preview).on_hover_text(q).clicked() {
                        action.recall_query = Some(q.clone());
                        ui.close();
                    }
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
            ui.label(format!("{} {}", rows, octa::i18n::t("sql.result_rows")));
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
        workspace_tables,
        workspace_attachments,
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
                let char_idx = r.primary.index;
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
            .show_inside(ui, |ui| {
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
            .show_inside(ui, |ui| {
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
    if !editor_focused && let Some((r, c)) = tab.sql_result_selected {
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

/// Render the editor body: a vertical ScrollArea containing a left-hand
/// line-number gutter and the `TextEdit::multiline` SQL editor. Returns the
/// TextEdit's Response so the caller can anchor the autocomplete popup.
fn draw_sql_editor(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    editor_id: egui::Id,
    default_row_limit: usize,
    action: &mut SqlAction,
    editor_font: octa::ui::settings::SqlEditorFont,
) -> egui::Response {
    let mono = egui::FontId::new(13.0, sql_font_family(editor_font, ui));
    let hint = format!("SELECT * FROM data LIMIT {default_row_limit}");
    let weak = ui.visuals().weak_text_color();

    egui::ScrollArea::vertical()
        .id_salt("sql_editor_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let line_count = tab.sql_query.lines().count().max(1);
            let trailing = tab.sql_query.ends_with('\n');
            let effective = if trailing { line_count + 1 } else { line_count };
            let digits = format_number(effective).len().max(2);
            let desired_rows = 8.max(effective);

            let numbers: String = (1..=effective)
                .map(|n| format!("{:>width$}", format_number(n), width = digits))
                .collect::<Vec<_>>()
                .join("\n");

            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(
                    egui::Label::new(egui::RichText::new(numbers).font(mono.clone()).color(weak))
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .selectable(false),
                );
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut tab.sql_query)
                        .id(editor_id)
                        .font(mono.clone())
                        .desired_width(f32::INFINITY)
                        .desired_rows(desired_rows)
                        .lock_focus(true)
                        .hint_text(hint.as_str()),
                );
                // Grab keyboard focus the frame after the panel opens so the
                // user can start typing immediately without clicking first.
                if tab.sql_editor_focus_pending {
                    resp.request_focus();
                    tab.sql_editor_focus_pending = false;
                }
                if resp.has_focus()
                    && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
                {
                    action.run = true;
                }
                if resp.changed() {
                    tab.sql_ac_visible = true;
                }
                resp
            })
            .inner
        })
        .inner
}

fn apply_suggestion_later(
    tab: &mut TabState,
    prefix_start: usize,
    prefix_len: usize,
    suggestion: &str,
    ctx: &egui::Context,
) {
    let end = prefix_start + prefix_len;
    if end > tab.sql_query.len() {
        return;
    }
    tab.sql_query.replace_range(prefix_start..end, suggestion);
    let id = editor_id();
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let new_char_idx = tab.sql_query[..prefix_start + suggestion.len()]
            .chars()
            .count();
        let ccursor = egui::text::CCursor::new(new_char_idx);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
        state.store(ctx, id);
    }
}

fn render_workspace_section(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    tables: &[WorkspaceRow],
    attachments: &[WorkspaceAttachment],
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    inspector_entry: Option<&crate::app::state::InspectorCacheEntry>,
    action: &mut SqlAction,
) {
    let extras = tables.iter().filter(|t| !t.is_active).count();
    let attached = attachments.len();
    let summary = if extras == 0 && attached == 0 {
        octa::i18n::t("sql.ws_only_data")
    } else {
        format!(
            "{} ({} {}, {} {})",
            octa::i18n::t("sql.workspace"),
            extras,
            octa::i18n::t("sql.extra_tables"),
            attached,
            octa::i18n::t("sql.attached_dbs"),
        )
    };
    // `CollapsingHeader` paints its own triangle via egui's drawing primitives,
    // so the glyph always renders even when the bundled font lacks the
    // geometric-shapes block (where `\u{25be}` / `\u{25b8}` live).
    let resp = egui::CollapsingHeader::new(egui::RichText::new(summary).strong())
        .id_salt("sql_workspace_section")
        .default_open(tab.sql_workspace_open)
        .show(ui, |ui| {
            // Two independent Resize widgets stacked vertically. Each gets its
            // own bottom-edge handle, so the user can grow the tree without
            // touching the inspector or the editor, and vice versa. The
            // editor's existing top-split handle stays independent of both.
            egui::Resize::default()
                .id_salt("sql_workspace_tree_resize")
                .resizable([false, true])
                .min_height(80.0)
                .default_height(140.0)
                .show(ui, |ui| {
                    render_workspace_list(
                        ui,
                        tab,
                        tables,
                        attachments,
                        inspector_selection,
                        action,
                    );
                });
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(2.0);
            egui::Resize::default()
                .id_salt("sql_workspace_inspector_resize")
                .resizable([false, true])
                .min_height(120.0)
                .default_height(240.0)
                .show(ui, |ui| {
                    render_workspace_inspector(ui, inspector_selection, inspector_entry, action);
                });
        });
    tab.sql_workspace_open = resp.openness > 0.5;
    ui.add_space(2.0);
    ui.separator();
}

fn render_workspace_list(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    tables: &[WorkspaceRow],
    attachments: &[WorkspaceAttachment],
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    egui::ScrollArea::vertical()
        .id_salt("sql_workspace_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in tables {
                let target = crate::app::sql_panel::InspectorTarget::RegisteredTable {
                    sql_name: row.sql_name.clone(),
                };
                let selected = inspector_selection == Some(&target);
                ui.horizontal(|ui| {
                    let label = egui::RichText::new(&row.sql_name).strong();
                    if ui.selectable_label(selected, label).clicked() {
                        action.select_inspector = Some(Some(target.clone()));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "({} {})",
                            row.row_count,
                            octa::i18n::t("sql.rows")
                        ))
                        .small()
                        .color(weak),
                    )
                    .on_hover_text(row.origin.clone());
                    if row.is_active {
                        if ui
                            .small_button(octa::i18n::t("sql.refresh"))
                            .on_hover_text(octa::i18n::t("sql.refresh_hint"))
                            .clicked()
                        {
                            action.refresh_active = true;
                        }
                    } else if ui
                        .small_button("\u{00d7}")
                        .on_hover_text(octa::i18n::t("sql.remove_table_hint"))
                        .clicked()
                    {
                        action.remove_table = Some(row.sql_name.clone());
                    }
                });
            }
            for att in attachments {
                let alias_key = att.alias.clone();
                let alias_open = tab.sql_workspace_tree_expanded.contains(&alias_key);
                ui.horizontal(|ui| {
                    let tri_resp = collapsing_triangle(ui, alias_open)
                        .on_hover_text(octa::i18n::t("sql.toggle_attached_hint"));
                    let label_resp = ui.add(
                        egui::Label::new(egui::RichText::new(&att.alias).strong())
                            .sense(egui::Sense::click()),
                    );
                    if tri_resp.clicked() || label_resp.clicked() {
                        action.toggle_tree_key = Some(alias_key.clone());
                    }
                    ui.label(
                        egui::RichText::new(format!("[{}]", att.kind_label))
                            .small()
                            .color(weak),
                    );
                    if !att.native {
                        ui.label(
                            egui::RichText::new(octa::i18n::t("sql.fallback"))
                                .small()
                                .color(weak)
                                .italics(),
                        )
                        .on_hover_text(octa::i18n::t("sql.fallback_hint"));
                    }
                    ui.label(
                        egui::RichText::new(format!("| {} tbl", att.table_count))
                            .small()
                            .color(weak),
                    )
                    .on_hover_text(att.source.clone());
                    if ui
                        .small_button(octa::i18n::t("sql.detach"))
                        .on_hover_text(octa::i18n::t("sql.detach_hint"))
                        .clicked()
                    {
                        action.detach_alias = Some(att.alias.clone());
                    }
                });
                if alias_open {
                    render_attached_tree(ui, tab, att, inspector_selection, action);
                }
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("sql.add_table"))
                    .on_hover_text(octa::i18n::t("sql.add_table_hint"))
                    .clicked()
                {
                    action.add_tables = true;
                }
                if ui
                    .button(octa::i18n::t("sql.attach_db"))
                    .on_hover_text(octa::i18n::t("sql.attach_db_hint"))
                    .clicked()
                {
                    action.attach_db = true;
                }
            });
        });
}

/// Paint a small collapsing triangle as a clickable widget. Replaces the
/// unicode `\u{25b8}` / `\u{25be}` glyphs the workspace tree used to draw -
/// the bundled font doesn't ship the geometric-shapes block, so users saw
/// tofu squares instead of arrows. Drawing the triangle directly via
/// `egui::Painter` is font-independent.
fn collapsing_triangle(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let size = egui::vec2(12.0, ui.spacing().interact_size.y.min(16.0));
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let stroke = ui.style().interact(&resp).fg_stroke;
        let center = rect.center();
        let r = 4.0;
        let points = if open {
            // Down-pointing triangle (▾)
            vec![
                center + egui::vec2(-r, -r * 0.5),
                center + egui::vec2(r, -r * 0.5),
                center + egui::vec2(0.0, r * 0.8),
            ]
        } else {
            // Right-pointing triangle (▸)
            vec![
                center + egui::vec2(-r * 0.5, -r),
                center + egui::vec2(-r * 0.5, r),
                center + egui::vec2(r * 0.8, 0.0),
            ]
        };
        ui.painter()
            .add(egui::Shape::convex_polygon(points, stroke.color, stroke));
    }
    resp
}

fn render_attached_tree(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    att: &WorkspaceAttachment,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    if att.schemas.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("(no tables visible - fallback attachment)")
                    .small()
                    .color(weak)
                    .italics(),
            );
        });
        return;
    }
    for schema in &att.schemas {
        let schema_key = format!("{}::{}", att.alias, schema.schema);
        let schema_open = tab.sql_workspace_tree_expanded.contains(&schema_key);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            let tri_resp = collapsing_triangle(ui, schema_open);
            let label_resp = ui.add(
                egui::Label::new(format!("{} ({})", schema.schema, schema.tables.len()))
                    .sense(egui::Sense::click()),
            );
            if tri_resp.clicked() || label_resp.clicked() {
                action.toggle_tree_key = Some(schema_key.clone());
            }
        });
        if schema_open {
            for t in &schema.tables {
                let target = crate::app::sql_panel::InspectorTarget::AttachedTable {
                    alias: att.alias.clone(),
                    schema: t.schema.clone(),
                    table: t.table.clone(),
                };
                let selected = inspector_selection == Some(&target);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    let label = egui::RichText::new(&t.table);
                    if ui.selectable_label(selected, label).clicked() {
                        action.select_inspector = Some(Some(target.clone()));
                    }
                    if let Some(n) = t.row_count {
                        ui.label(
                            egui::RichText::new(format!("({n} rows)"))
                                .small()
                                .color(weak),
                        );
                    }
                });
            }
        }
    }
}

fn render_workspace_inspector(
    ui: &mut egui::Ui,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    inspector_entry: Option<&crate::app::state::InspectorCacheEntry>,
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    let strong = ui.visuals().strong_text_color();
    let target = match inspector_selection {
        Some(t) => t,
        None => {
            ui.label(
                egui::RichText::new(octa::i18n::t("sql.inspector"))
                    .strong()
                    .color(strong),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new(octa::i18n::t("sql.inspector_empty")).weak());
            return;
        }
    };
    let qualified = target.qualified_sql();
    // Top header bar - qualified name + clear-selection button. Docked so it
    // stays visible no matter how short the inspector pane is.
    egui::Panel::top("sql_inspector_header")
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&qualified)
                        .strong()
                        .monospace()
                        .color(strong),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("\u{00d7}")
                        .on_hover_text(octa::i18n::t("sql.clear_inspector_hint"))
                        .clicked()
                    {
                        action.select_inspector = Some(None);
                    }
                });
            });
            ui.separator();
        });
    // Bottom action bar - Copy / Insert / Run. Docked so it never scrolls
    // out of view even when the column list is long.
    egui::Panel::bottom("sql_inspector_actions")
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .small_button(octa::i18n::t("sql.copy_name"))
                    .on_hover_text(octa::i18n::t("sql.copy_name_hint"))
                    .clicked()
                {
                    action.copy_qualified = Some(qualified.clone());
                }
                if ui
                    .small_button(octa::i18n::t("sql.insert"))
                    .on_hover_text(format!(
                        "{} `SELECT * FROM {qualified} LIMIT 100;`",
                        octa::i18n::t("sql.insert_hint")
                    ))
                    .clicked()
                {
                    action.insert_qualified = Some(qualified.clone());
                }
                if ui
                    .small_button(octa::i18n::t("sql.run_table"))
                    .on_hover_text(format!(
                        "{} `SELECT * FROM {qualified} LIMIT 100`",
                        octa::i18n::t("sql.run_table_hint")
                    ))
                    .clicked()
                {
                    action.run_qualified = Some(qualified.clone());
                }
            });
            ui.add_space(2.0);
        });
    // Central body - columns grid + sample table inside a scroll area that
    // fills whatever vertical room is between the header and the action bar.
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| {
            let entry = match inspector_entry {
                Some(e) => e,
                None => {
                    ui.label(egui::RichText::new(octa::i18n::t("sql.loading")).weak());
                    return;
                }
            };
            let inspection = match &entry.result {
                Ok(i) => i,
                Err(msg) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 80, 80),
                        format!("Error: {msg}"),
                    );
                    return;
                }
            };
            let row_count_str = inspection
                .row_count
                .map(format_number)
                .unwrap_or_else(|| "?".to_string());
            ui.label(
                egui::RichText::new(format!(
                    "{} column{} | {} row{}",
                    inspection.columns.len(),
                    if inspection.columns.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    row_count_str,
                    if inspection.row_count == Some(1) {
                        ""
                    } else {
                        "s"
                    },
                ))
                .small()
                .color(weak),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("sql_inspector_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("sql_inspector_columns")
                        .num_columns(2)
                        .spacing(egui::vec2(10.0, 2.0))
                        .show(ui, |ui| {
                            for col in &inspection.columns {
                                ui.label(egui::RichText::new(&col.name).monospace());
                                ui.label(
                                    egui::RichText::new(&col.data_type)
                                        .monospace()
                                        .small()
                                        .color(weak),
                                );
                                ui.end_row();
                            }
                        });
                    if !inspection.sample_rows.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "Sample (first {}):",
                                inspection.sample_rows.len()
                            ))
                            .small()
                            .color(weak),
                        );
                        ui.add_space(2.0);
                        use egui_extras::{Column, TableBuilder};
                        let mut builder = TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
                        for _ in &inspection.columns {
                            builder = builder.column(Column::auto().at_least(60.0).resizable(true));
                        }
                        builder
                            .header(18.0, |mut header| {
                                for col in &inspection.columns {
                                    header.col(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&col.name).small().monospace(),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Extend),
                                        );
                                    });
                                }
                            })
                            .body(|mut body| {
                                for row in &inspection.sample_rows {
                                    body.row(16.0, |mut r| {
                                        for cell in row {
                                            r.col(|ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(cell)
                                                            .small()
                                                            .monospace(),
                                                    )
                                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                                );
                                            });
                                        }
                                    });
                                }
                            });
                    }
                });
        });
}

fn render_result_table(
    ui: &mut egui::Ui,
    table: &octa::data::DataTable,
    selected: &mut Option<(usize, usize)>,
) {
    use egui_extras::{Column, TableBuilder};

    if table.col_count() == 0 {
        ui.label(egui::RichText::new(octa::i18n::t("sql.no_columns")).weak());
        return;
    }

    egui::ScrollArea::horizontal()
        .id_salt("sql_result_scroll")
        .show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for _ in &table.columns {
                builder = builder.column(Column::auto().at_least(80.0).resizable(true));
            }
            builder
                .header(22.0, |mut header| {
                    for col in &table.columns {
                        header.col(|ui| {
                            ui.strong(&col.name);
                        });
                    }
                })
                .body(|mut body| {
                    for r in 0..table.row_count() {
                        body.row(20.0, |mut row| {
                            for c in 0..table.col_count() {
                                row.col(|ui| {
                                    let v = table.get(r, c).cloned().unwrap_or(CellValue::Null);
                                    let text = v.to_string();
                                    // Click selects the whole cell (highlighted)
                                    // like the main table, so Ctrl+C - handled in
                                    // `render_sql_view` - copies exactly it. The
                                    // context menu adds Copy cell / Copy all.
                                    let is_sel = *selected == Some((r, c));
                                    let resp = ui.selectable_label(is_sel, &text);
                                    if resp.clicked() {
                                        *selected = Some((r, c));
                                        // Take keyboard focus from the editor so
                                        // its TextEdit doesn't swallow Ctrl+C.
                                        resp.request_focus();
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button(octa::i18n::t("header.copy")).clicked() {
                                            ui.ctx().copy_text(text.clone());
                                            ui.close();
                                        }
                                        if ui.button(octa::i18n::t("view.copy_all")).clicked() {
                                            ui.ctx().copy_text(result_to_tsv(table));
                                            ui.close();
                                        }
                                    });
                                });
                            }
                        });
                    }
                });
        });
}

/// Serialise a result table to TSV (header row + one row per record, cells
/// joined by tabs, rows by newlines) for the result table's "Copy all" action.
fn result_to_tsv(table: &octa::data::DataTable) -> String {
    let mut out = String::new();
    out.push_str(
        &table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join("\t"),
    );
    out.push('\n');
    for r in 0..table.row_count() {
        let line = (0..table.col_count())
            .map(|c| {
                table
                    .get(r, c)
                    .cloned()
                    .unwrap_or(CellValue::Null)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\t");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_picks_up_word_before_cursor() {
        let s = "SELECT na";
        let (start, pfx) = current_prefix_at(s, s.len());
        assert_eq!(pfx, "na");
        assert_eq!(start, 7);
    }

    #[test]
    fn prefix_is_empty_after_whitespace() {
        let s = "SELECT ";
        let (start, pfx) = current_prefix_at(s, s.len());
        assert_eq!(pfx, "");
        assert_eq!(start, s.len());
    }

    #[test]
    fn suggestions_match_columns_and_keywords() {
        let cols = vec!["name".to_string(), "age".to_string()];
        let out = collect_suggestions("n", &cols, 8);
        assert!(out.contains(&"name".to_string()));
        assert!(out.contains(&"NOT".to_string()));
    }

    #[test]
    fn suggestions_respect_limit() {
        let cols: Vec<String> = (0..20).map(|i| format!("col_{i}")).collect();
        let out = collect_suggestions("col", &cols, 5);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn empty_prefix_yields_no_suggestions() {
        let cols = vec!["name".to_string()];
        let out = collect_suggestions("", &cols, 8);
        assert!(out.is_empty());
    }
}
