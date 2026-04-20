use crate::TabState;
use crate::ui::settings::SqlPanelPosition;

use eframe::egui;
use octa::data::CellValue;

/// User actions emitted by the SQL view in a single frame.
#[derive(Debug, Clone, Default)]
pub struct SqlAction {
    pub run: bool,
    pub clear: bool,
    pub export: bool,
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

/// Render a split-pane SQL editor (top) and result table (bottom).
/// The current tab's table is exposed in queries as `data`.
/// `partial_rows` carries `(loaded, total)` when the table isn't fully loaded.
pub fn render_sql_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    autocomplete_enabled: bool,
    default_row_limit: usize,
    panel_position: SqlPanelPosition,
    partial_rows: Option<(usize, usize)>,
) -> SqlAction {
    let mut action = SqlAction::default();
    let editor_id = editor_id();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Query against `data`").strong());
        ui.add_space(8.0);
        if ui
            .button("Run (Ctrl+Enter)")
            .on_hover_text("Execute the query")
            .clicked()
        {
            action.run = true;
        }
        if ui.button("Clear result").clicked() {
            action.clear = true;
        }
        let has_result = tab.sql_result.as_ref().is_some_and(|t| t.col_count() > 0);
        ui.add_enabled_ui(has_result, |ui| {
            if ui
                .button("Export…")
                .on_hover_text("Save the result as CSV, Parquet, JSON, Excel, etc.")
                .clicked()
            {
                action.export = true;
            }
        });
        if let Some(rows) = tab.sql_result.as_ref().map(|t| t.row_count()) {
            ui.add_space(12.0);
            ui.label(format!(
                "{} result row{}",
                rows,
                if rows == 1 { "" } else { "s" }
            ));
        }
    });
    ui.add_space(4.0);

    // --- Compute autocomplete state BEFORE rendering the TextEdit so we can
    // intercept arrow / Enter / Tab / Escape keys while the popup is visible.
    let editor_focused = ui.ctx().memory(|m| m.focused() == Some(editor_id));
    let mut suggestions: Vec<String> = Vec::new();
    let mut prefix_start = 0usize;
    let mut prefix_len = 0usize;
    if autocomplete_enabled && editor_focused {
        let columns: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
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

    // Consume navigation keys while the popup is showing so the TextEdit
    // doesn't act on them (egui would otherwise move the caret on arrow keys
    // and insert a newline on Enter or a tab on Tab).
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
    // resize handle sits at its top edge — if the nested editor panel is also
    // docked at the top, its frame covers the outer resize strip and the user
    // can't drag the SQL panel taller from between the table and the box. To
    // avoid that collision, dock the *result* panel at the bottom in that
    // case and let the editor fill the remaining central area. For every
    // other outer position the top-docked editor split is fine.
    let total = ui.available_height();
    let default_editor_h = (total * 0.4).max(160.0).min((total - 80.0).max(120.0));
    let default_result_h = (total - default_editor_h).max(120.0);
    let mut editor_response: Option<egui::Response> = None;

    let render_result_area = |ui: &mut egui::Ui, tab: &TabState| {
        if let Some((loaded, total)) = partial_rows {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26a0} Result based on {loaded} of {total} rows currently loaded."
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
        if let Some(result) = &tab.sql_result {
            render_result_table(ui, result);
        } else if tab.sql_error.is_none() {
            ui.label(egui::RichText::new("Run a query to see results.").weak());
        }
    };

    if matches!(panel_position, SqlPanelPosition::Bottom) {
        egui::TopBottomPanel::bottom("sql_result_split")
            .resizable(true)
            .default_height(default_result_h)
            .min_height(80.0)
            .show_inside(ui, |ui| {
                render_result_area(ui, tab);
            });
        editor_response = Some(draw_sql_editor(
            ui,
            tab,
            editor_id,
            default_row_limit,
            &mut action,
        ));
    } else {
        egui::TopBottomPanel::top("sql_editor_split")
            .resizable(true)
            .default_height(default_editor_h)
            .min_height(80.0)
            .show_inside(ui, |ui| {
                editor_response = Some(draw_sql_editor(
                    ui,
                    tab,
                    editor_id,
                    default_row_limit,
                    &mut action,
                ));
            });
        ui.add_space(2.0);
    }
    let editor_response = editor_response.expect("editor panel always renders");

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
        let below = egui::AboveOrBelow::Below;
        egui::popup::popup_above_or_below_widget(
            ui,
            popup_id,
            &editor_response,
            below,
            egui::popup::PopupCloseBehavior::IgnoreClicks,
            |ui| {
                ui.set_min_width(220.0);
                for (idx, s) in suggestions.iter().enumerate() {
                    let selected = idx == tab.sql_ac_selected;
                    let resp = ui.selectable_label(selected, s);
                    if resp.clicked() {
                        apply_suggestion_later(tab, prefix_start, prefix_len, s, ui.ctx());
                        editor_response.request_focus();
                    }
                    if resp.hovered() {
                        tab.sql_ac_selected = idx;
                    }
                }
            },
        );
        // popup_above_or_below_widget only opens when memory is open, so we
        // force it open every frame while the popup is active.
        ui.memory_mut(|m| m.open_popup(popup_id));
    } else {
        let popup_id = ui.make_persistent_id("sql_autocomplete_popup");
        ui.memory_mut(|m| {
            if m.is_popup_open(popup_id) {
                m.close_popup();
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
) -> egui::Response {
    let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let hint = format!("SELECT * FROM data LIMIT {default_row_limit}");
    let weak = ui.visuals().weak_text_color();

    egui::ScrollArea::vertical()
        .id_salt("sql_editor_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let line_count = tab.sql_query.lines().count().max(1);
            let trailing = tab.sql_query.ends_with('\n');
            let effective = if trailing { line_count + 1 } else { line_count };
            let digits = effective.to_string().len().max(2);
            let desired_rows = 8.max(effective);

            let numbers: String = (1..=effective)
                .map(|n| format!("{n:>width$}", width = digits))
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

fn render_result_table(ui: &mut egui::Ui, table: &octa::data::DataTable) {
    use egui_extras::{Column, TableBuilder};

    if table.col_count() == 0 {
        ui.label(egui::RichText::new("Query returned no columns.").weak());
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
                                    ui.label(v.to_string());
                                });
                            }
                        });
                    }
                });
        });
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
