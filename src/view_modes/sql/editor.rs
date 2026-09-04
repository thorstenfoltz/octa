//! The SQL text editor: font selection, the autocomplete popup and the
//! deferred suggestion application.
//!
//! Split out of `view_modes/sql.rs` (1,555 lines). Code moved unchanged.

use super::*;

/// Render the editor body: a vertical ScrollArea containing a left-hand
/// line-number gutter and the `TextEdit::multiline` SQL editor. Returns the
/// TextEdit's Response so the caller can anchor the autocomplete popup.
pub(super) fn draw_sql_editor(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    editor_id: egui::Id,
    default_row_limit: usize,
    action: &mut SqlAction,
    editor_font: octa::ui::settings::SqlEditorFont,
) -> egui::Response {
    let mono = egui::FontId::new(13.0, sql_font_family(editor_font, ui));
    // Server mode queries the live table, not the local `data` snapshot, so
    // the template names the tab's origin table instead.
    let hint = match tab.db_origin.as_ref().filter(|_| tab.sql_run_on_server) {
        Some(o) => {
            // Catalog engines (Snowflake/Databricks/BigQuery) need the full
            // three-part name; two-level engines just schema.table.
            let name = match &o.catalog {
                Some(cat) => format!("{cat}.{}.{}", o.schema, o.table),
                None => format!("{}.{}", o.schema, o.table),
            };
            format!("SELECT * FROM {name} LIMIT {default_row_limit}")
        }
        None => format!("SELECT * FROM data LIMIT {default_row_limit}"),
    };
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
                // Follow a selection dragged past the edge of the editor.
                crate::view_modes::text_ops::autoscroll_while_selecting(ui, &resp);
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

pub(super) fn apply_suggestion_later(
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
