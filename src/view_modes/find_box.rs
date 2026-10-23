//! The find and replace box, laid out the way VS Code lays out its own: a
//! find row (toggle, find field, match count, previous, next, close) with the
//! replace row (replace field, Replace, All) below it, floating over the top
//! right corner of what it searches. The SQL editor keeps one per editor, the
//! tab views (table, text, trees, notebook) one per tab. The toolbar's search
//! box is a separate thing and only ever searches.

use eframe::egui;
use octa::data::search::RowMatcher;

/// What the user typed into a box. Its owner holds `None` while it is closed.
#[derive(Debug, Clone, Default)]
pub(crate) struct FindInput {
    pub(crate) query: String,
    pub(crate) replace: String,
    /// The replace row is showing.
    pub(crate) replacing: bool,
    /// VS Code's three switches beside the find field.
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) regex: bool,
    /// Put the keyboard in the find field on the next frame.
    pub(crate) focus: bool,
}

/// What a click or key in the box asked for this frame.
#[derive(Debug, Default)]
pub(crate) struct FindBoxOut {
    /// The find text changed.
    pub(crate) changed: bool,
    pub(crate) prev: bool,
    pub(crate) next: bool,
    pub(crate) replace_one: bool,
    pub(crate) replace_all: bool,
    pub(crate) close: bool,
}

/// The texts that differ between the SQL editor's box and a tab's.
pub(crate) struct FindBoxTexts {
    pub(crate) placeholder: String,
    pub(crate) find_hint: String,
    pub(crate) replace_all_hint: String,
}

/// What the box finds: plain text in any case unless its switches say
/// otherwise. A regular expression's `$1` in the replacement inserts what
/// the group matched; plain text goes in as typed.
pub(crate) fn matcher(input: &FindInput) -> RowMatcher {
    let mode = if input.regex {
        octa::data::SearchMode::Regex
    } else {
        octa::data::SearchMode::Plain
    };
    RowMatcher::with_options(&input.query, mode, input.case_sensitive, input.whole_word)
}

/// The find field of the box whose ids derive from `base`.
pub(crate) fn find_field_id(base: egui::Id) -> egui::Id {
    base.with("find_box_find")
}

pub(crate) fn replace_field_id(base: egui::Id) -> egui::Id {
    base.with("find_box_replace")
}

/// Width of the find and replace fields, and how far they shrink when the
/// view is narrow.
const FIELD_WIDTH: f32 = 200.0;
const MIN_FIELD_WIDTH: f32 = 60.0;
/// Room left at the right for the view's own scrollbar.
const RIGHT_GAP: f32 = 18.0;

/// Draw the box with its top right corner at `corner`, the top right of the
/// view it searches; every id in it derives from `base`. Draw it AFTER that
/// view: it lies on top in the same layer, so its widgets take the clicks
/// they cover. `count` matches, the `current` one shown; `replace_block`
/// says why replacing is off, if it is.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    corner: egui::Pos2,
    base: egui::Id,
    input: &mut FindInput,
    (count, current): (usize, usize),
    replace_block: Option<&str>,
    texts: &FindBoxTexts,
) -> FindBoxOut {
    use crate::ui::control_row::control_line;
    use octa::i18n::t;

    let mut out = FindBoxOut::default();
    // Right-aligned by last frame's size, the way egui's own pivoted areas
    // are: a box is only measured once it is laid out. In a narrow view the
    // fields give up what the box lacks, as VS Code's do.
    let size_id = base.with("find_box_size");
    let (size, last_field) = ui
        .ctx()
        .data(|d| d.get_temp::<(egui::Vec2, f32)>(size_id))
        .unwrap_or((egui::vec2(480.0, 40.0), FIELD_WIDTH));
    let room = corner.x - RIGHT_GAP - ui.max_rect().left();
    let field_width = (room - (size.x - last_field)).clamp(MIN_FIELD_WIDTH, FIELD_WIDTH);
    let left = (corner.x - RIGHT_GAP - size.x).max(ui.max_rect().left());
    let at = egui::Rect::from_min_size(egui::pos2(left, corner.y + 2.0), size);
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(at.min, ui.max_rect().max))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    // Themes paint a hovered widget up to 3px larger, which over a box this
    // small reads as twitching (the SQL panel turns it off for the same
    // reason).
    let widgets = &mut ui.style_mut().visuals.widgets;
    widgets.hovered.expansion = 0.0;
    widgets.active.expansion = 0.0;
    // Clicks on the box's own background must not reach the view beneath.
    ui.interact(at, base.with("find_box_bg"), egui::Sense::click_and_drag());

    let frame = egui::Frame::popup(ui.style()).show(&mut ui, |ui| {
        let any = count > 0;
        control_line(ui, |ui| {
            let h = crate::ui::control_row::control_height(ui);
            let (_, toggle) = ui.allocate_exact_size(egui::vec2(14.0, h), egui::Sense::click());
            let toggle = toggle.on_hover_text(t("sql.find_replace_toggle_hint"));
            let openness = if input.replacing { 1.0 } else { 0.0 };
            egui::collapsing_header::paint_default_icon(ui, openness, &toggle);
            if toggle.clicked() {
                input.replacing = !input.replacing;
            }

            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut input.query)
                        .id(find_field_id(base))
                        .desired_width(field_width)
                        .hint_text(&texts.placeholder),
                )
                .on_hover_text(&texts.find_hint);
            if input.focus {
                field.request_focus();
                input.focus = false;
            }
            out.changed = field.changed();
            for (on, label, hint) in [
                (&mut input.case_sensitive, "Aa", t("search.case_sensitive")),
                (&mut input.whole_word, "W", t("search.whole_word")),
                (&mut input.regex, ".*", t("search.find_box_regex_hint")),
            ] {
                if ui
                    .selectable_label(*on, label)
                    .on_hover_text(hint)
                    .clicked()
                {
                    *on = !*on;
                    out.changed = true;
                }
            }
            // Enter steps through the matches and keeps the keyboard here;
            // Escape closes the box. Shift+Enter does not end a one-line
            // field's input, so the field still has the keyboard then.
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                out.close = true;
            } else if enter && (field.lost_focus() || field.has_focus()) {
                if ui.input(|i| i.modifiers.shift) {
                    out.prev = true;
                } else {
                    out.next = true;
                }
                field.request_focus();
            }

            let label = if input.query.is_empty() {
                String::new()
            } else if any {
                format!("{} / {}", current + 1, count)
            } else {
                t("search.no_matches")
            };
            ui.label(egui::RichText::new(label).small().weak());
            out.prev |= ui
                .add_enabled(any, egui::Button::new("<"))
                .on_hover_text(t("search.prev_match"))
                .clicked();
            out.next |= ui
                .add_enabled(any, egui::Button::new(">"))
                .on_hover_text(t("search.next_match"))
                .clicked();
            out.close |= crate::ui::dialog_chrome::caption_button(
                ui,
                crate::ui::dialog_chrome::Caption::Close,
                false,
                egui::vec2(h, h),
            )
            .on_hover_text(t("sql.find_close_hint"))
            .clicked();
        });

        if input.replacing {
            control_line(ui, |ui| {
                // Under the find field, past the toggle.
                ui.add_space(14.0 + ui.spacing().item_spacing.x);
                let field = ui
                    .add(
                        egui::TextEdit::singleline(&mut input.replace)
                            .id(replace_field_id(base))
                            .desired_width(field_width)
                            .hint_text(format!("{}...", t("transform.replace_with"))),
                    )
                    .on_hover_text(t("sql.replace_field_hint"));
                let can = any && replace_block.is_none();
                // Enter replaces the current match and keeps the keyboard
                // here, as in VS Code; Escape closes the box.
                if field.lost_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        out.close = true;
                    } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        out.replace_one = can;
                        field.request_focus();
                    }
                }
                let why_not = replace_block
                    .map(str::to_owned)
                    .unwrap_or_else(|| t("search.no_matches"));
                out.replace_one |= ui
                    .add_enabled(can, egui::Button::new(t("sql.find_replace_toggle")))
                    .on_hover_text(t("sql.replace_next_hint"))
                    .on_disabled_hover_text(&why_not)
                    .clicked();
                out.replace_all = ui
                    .add_enabled(can, egui::Button::new(t("search.replace_all")))
                    .on_hover_text(&texts.replace_all_hint)
                    .on_disabled_hover_text(&why_not)
                    .clicked();
            });
        }
    });

    let measured = frame.response.rect.size();
    if (measured - size).length() > 0.5 || field_width != last_field {
        ui.ctx()
            .data_mut(|d| d.insert_temp(size_id, (measured, field_width)));
        ui.ctx().request_repaint();
    }
    out
}

#[cfg(test)]
#[path = "find_box_tests.rs"]
mod tests;
