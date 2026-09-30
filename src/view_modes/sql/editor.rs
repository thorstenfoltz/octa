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
    // the template names the tab's origin table instead. A connection this
    // tab has no table on gets no template: nothing there is known to exist.
    let hint = match (
        tab.sql_target.is_some(),
        crate::app::sql_panel::server_origin(tab),
    ) {
        (true, None) => String::new(),
        (_, Some(o)) => {
            // Catalog engines (Snowflake/Databricks/BigQuery) need the full
            // three-part name; two-level engines just schema.table.
            let name = match &o.catalog {
                Some(cat) => format!("{cat}.{}.{}", o.schema, o.table),
                None => format!("{}.{}", o.schema, o.table),
            };
            format!("SELECT * FROM {name} LIMIT {default_row_limit}")
        }
        (false, None) => format!("SELECT * FROM data LIMIT {default_row_limit}"),
    };
    let weak = ui.visuals().weak_text_color();
    // `--` comments are drawn faded, so what will run stands out from what
    // will not. SQL buffers are small, so the job is rebuilt per frame.
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
        let text = text.as_str();
        let normal = ui.visuals().text_color();
        let faded = normal.gamma_multiply(0.4);
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        let mut at = 0;
        for r in line_comment_ranges(text) {
            job.append(
                &text[at..r.start],
                0.0,
                egui::TextFormat::simple(mono.clone(), normal),
            );
            job.append(
                &text[r.clone()],
                0.0,
                egui::TextFormat::simple(mono.clone(), faded),
            );
            at = r.end;
        }
        job.append(
            &text[at..],
            0.0,
            egui::TextFormat::simple(mono.clone(), normal),
        );
        ui.fonts_mut(|f| f.layout_job(job))
    };

    egui::ScrollArea::vertical()
        .id_salt(("sql_editor_scroll", tab.sql.id))
        .auto_shrink([false; 2])
        // Shrink into the slot it is handed, all the way. egui's default
        // floor is 64px, and a slot shorter than that had the editor drawn
        // over the result pane below it.
        .min_scrolled_height(0.0)
        .show(ui, |ui| {
            let line_count = tab.sql.query.lines().count().max(1);
            let trailing = tab.sql.query.ends_with('\n');
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
                // Tab in an empty editor takes the template as real text, caret
                // at the end, so Ctrl+Enter runs it. Consumed before the
                // TextEdit sees it, which would otherwise insert a tab.
                if !hint.is_empty()
                    && tab.sql.query.is_empty()
                    && ui.memory(|m| m.focused() == Some(editor_id))
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                {
                    apply_suggestion_later(tab, 0, 0, &hint, ui.ctx());
                }
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut tab.sql.query)
                        .id(editor_id)
                        .font(mono.clone())
                        .desired_width(f32::INFINITY)
                        .desired_rows(desired_rows)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .hint_text(hint.as_str()),
                );
                // Follow a selection dragged past the edge of the editor.
                crate::view_modes::text_ops::autoscroll_while_selecting(ui, &resp);
                // Grab keyboard focus the frame after the panel opens so the
                // user can start typing immediately without clicking first.
                if tab.sql.focus_pending {
                    resp.request_focus();
                    tab.sql.focus_pending = false;
                }
                if resp.has_focus()
                    && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
                {
                    action.run = true;
                }
                if resp.changed() {
                    tab.sql.ac_visible = true;
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
    if end > tab.sql.query.len() {
        return;
    }
    tab.sql.query.replace_range(prefix_start..end, suggestion);
    let id = editor_id(tab.sql.id);
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let new_char_idx = tab.sql.query[..prefix_start + suggestion.len()]
            .chars()
            .count();
        let ccursor = egui::text::CCursor::new(new_char_idx);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
        state.store(ctx, id);
    }
}
