//! Find and replace in the SQL editor: the shared find and replace box
//! (`view_modes::find_box`) over the top right of each editor, opened by
//! Ctrl+F / Ctrl+H while the editor has focus. The toolbar's search box
//! belongs to the table, so the editor gets a find of its own.

use std::ops::Range;

use super::*;
use crate::app::state::SqlFind;
use crate::view_modes::find_box;

/// Match backgrounds, the ones the Markdown editor paints.
const HL: egui::Color32 = egui::Color32::from_rgba_premultiplied(150, 130, 0, 90);
const HL_CURRENT: egui::Color32 = egui::Color32::from_rgba_premultiplied(170, 100, 0, 150);

/// The ids of one editor's box derive from this.
fn box_id(pane: u64) -> egui::Id {
    egui::Id::new(("sql_find_box", pane))
}

pub(crate) fn find_field_id(pane: u64) -> egui::Id {
    find_box::find_field_id(box_id(pane))
}

pub(crate) fn replace_field_id(pane: u64) -> egui::Id {
    find_box::replace_field_id(box_id(pane))
}

/// The open box's matches in `text` and the index of the current one
/// (clamped, so it survives matches disappearing). Empty when closed.
pub(super) fn matches_in(find: Option<&SqlFind>, text: &str) -> (Vec<Range<usize>>, usize) {
    let Some(find) = find.filter(|f| !f.input.query.is_empty()) else {
        return (Vec::new(), 0);
    };
    let ranges = find_box::matcher(&find.input).find_ranges(text);
    let current = find.current.min(ranges.len().saturating_sub(1));
    (ranges, current)
}

/// Paint the matches into the editor's layout job.
pub(super) fn highlight(job: &mut egui::text::LayoutJob, find: Option<&SqlFind>, text: &str) {
    let (ranges, current) = matches_in(find, text);
    octa::ui::search_highlight::apply_highlight(job, &ranges, ranges.get(current), HL, HL_CURRENT);
}

/// Open the box on the active editor (or keep it open), with the keyboard
/// in its find field. `replace` also shows the replace row. Text marked on
/// one line in the editor becomes the search.
pub(crate) fn open(tab: &mut TabState, ctx: &egui::Context, replace: bool) {
    let marked =
        crate::view_modes::text_ops::selected_text(ctx, editor_id(tab.sql.id), &tab.sql.query)
            .filter(|s| !s.contains('\n'));
    let find = tab.sql.find.get_or_insert_with(SqlFind::default);
    if let Some(s) = marked {
        find.input.query = s;
        find.current = 0;
    }
    find.input.replacing |= replace;
    find.input.focus = true;
}

/// Is the keyboard in this pane's editor or its find box? Ctrl+F / Ctrl+H
/// there mean the editor's text, not the table.
pub(crate) fn owns_keyboard(ctx: &egui::Context, pane: u64) -> bool {
    ctx.memory(|m| m.focused()).is_some_and(|f| {
        [editor_id(pane), find_field_id(pane), replace_field_id(pane)].contains(&f)
    })
}

/// Draw the box over the editor just drawn, when open. `corner` is the
/// editor's top right.
pub(super) fn draw_find_box(ui: &mut egui::Ui, tab: &mut TabState, corner: egui::Pos2) {
    let Some(mut find) = tab.sql.find.take() else {
        return;
    };
    let (ranges, current) = matches_in(Some(&find), &tab.sql.query);
    let texts = find_box::FindBoxTexts {
        placeholder: octa::i18n::t("sql.find_placeholder"),
        find_hint: octa::i18n::t("sql.find_hint"),
        replace_all_hint: octa::i18n::t("sql.replace_all_hint"),
    };
    let out = find_box::draw(
        ui,
        corner,
        box_id(tab.sql.id),
        &mut find.input,
        (ranges.len(), current),
        None,
        &texts,
    );
    if out.changed {
        find.current = 0;
        find.jump = true;
    }
    if out.prev || out.next {
        step(&mut find, ranges.len(), out.prev);
    }
    let matcher = find_box::matcher(&find.input);
    if out.replace_one && !ranges.is_empty() {
        // The next match moves into the replaced one's place, so the index
        // stays and now names it.
        tab.sql.query =
            matcher.replace_range(&tab.sql.query, ranges[current].clone(), &find.input.replace);
        find.jump = true;
    }
    if out.replace_all && !ranges.is_empty() {
        tab.sql.query = matcher.replace_all(&tab.sql.query, &find.input.replace);
        find.current = 0;
    }
    if out.close {
        tab.sql.focus_pending = true;
    } else {
        tab.sql.find = Some(find);
    }
}

/// Move the current match one forward (or back), wrapping, and have the
/// editor show it.
fn step(find: &mut SqlFind, count: usize, back: bool) {
    if count == 0 {
        return;
    }
    let at = find.current.min(count - 1);
    find.current = if back {
        (at + count - 1) % count
    } else {
        (at + 1) % count
    };
    find.jump = true;
}

/// After the editor is laid out: scroll the current match into view and put
/// the editor's caret after it, once per step. The keyboard stays in the
/// box, and egui keeps no selection in an editor without focus (it
/// collapses to the caret), so the highlight shows the match and the caret
/// is where typing goes on after Escape.
pub(super) fn show_current(
    ui: &egui::Ui,
    tab: &mut TabState,
    output: &mut egui::text_edit::TextEditOutput,
) {
    let Some(find) = tab.sql.find.as_mut().filter(|f| f.jump) else {
        return;
    };
    find.jump = false;
    let (ranges, current) = matches_in(Some(find), &tab.sql.query);
    let Some(r) = ranges.get(current) else {
        return;
    };
    let text = &tab.sql.query;
    let start = egui::text::CCursor::new(text[..r.start].chars().count());
    let end = egui::text::CCursor::new(text[..r.end].chars().count());
    output
        .state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(end)));
    output.state.clone().store(ui.ctx(), output.response.id);
    let at = output
        .galley
        .pos_from_cursor(start)
        .translate(output.galley_pos.to_vec2());
    ui.scroll_to_rect(at, Some(egui::Align::Center));
}

#[cfg(test)]
#[path = "find_tests.rs"]
mod tests;
