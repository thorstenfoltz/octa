//! Shared text-editor helpers used by raw text and SQL views.

use eframe::egui;

/// Which kind of case conversion to apply to the selected text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOp {
    Upper,
    Lower,
}

impl CaseOp {
    fn apply(self, text: &str) -> String {
        match self {
            CaseOp::Upper => text.to_uppercase(),
            CaseOp::Lower => text.to_lowercase(),
        }
    }
}

/// Translate a character range from egui's cursor into a byte range.
pub(crate) fn char_range_to_byte_range(
    s: &str,
    start: usize,
    end: usize,
) -> std::ops::Range<usize> {
    let mut byte_start = s.len();
    let mut byte_end = s.len();
    for (char_idx, (byte_idx, _)) in s.char_indices().enumerate() {
        if char_idx == start {
            byte_start = byte_idx;
        }
        if char_idx == end {
            byte_end = byte_idx;
            return byte_start..byte_end;
        }
    }
    if start >= s.chars().count() {
        byte_start = s.len();
    }
    byte_start..byte_end
}

/// Convert the currently selected text in the TextEdit identified by
/// `text_edit_id` to upper or lower case. Only operates on a non-empty
/// selection - if nothing is selected the buffer is left untouched and the
/// function returns `false`.
pub fn apply_case_to_selection(
    ctx: &egui::Context,
    text_edit_id: egui::Id,
    buffer: &mut String,
    op: CaseOp,
) -> bool {
    let state = egui::TextEdit::load_state(ctx, text_edit_id);
    let range = state.as_ref().and_then(|s| s.cursor.char_range()).map(|r| {
        let a = r.primary.index;
        let b = r.secondary.index;
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        start..end
    });

    let Some(r) = range else { return false };
    if r.start >= r.end {
        return false;
    }
    let byte_range = char_range_to_byte_range(buffer, r.start, r.end);
    if byte_range.start >= buffer.len() {
        return false;
    }
    let selected = &buffer[byte_range.clone()];
    let replaced = op.apply(selected);
    if replaced == selected {
        return false;
    }
    buffer.replace_range(byte_range, &replaced);
    true
}

/// Returns the substring currently selected in the TextEdit identified by
/// `text_edit_id`. Returns `None` when there is no selection or when the
/// selection is empty.
pub fn selected_text(ctx: &egui::Context, text_edit_id: egui::Id, buffer: &str) -> Option<String> {
    let state = egui::TextEdit::load_state(ctx, text_edit_id)?;
    let range = state.cursor.char_range()?;
    let a = range.primary.index;
    let b = range.secondary.index;
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    if start >= end {
        return None;
    }
    let byte_range = char_range_to_byte_range(buffer, start, end);
    if byte_range.start >= byte_range.end || byte_range.end > buffer.len() {
        return None;
    }
    Some(buffer[byte_range].to_string())
}

#[cfg(test)]
#[path = "text_ops_tests.rs"]
mod tests;

/// How close to the edge of the visible area the pointer has to get before the
/// view starts scrolling itself, in points.
const AUTOSCROLL_MARGIN: f32 = 24.0;
/// Fastest the view scrolls itself, in points per frame.
const AUTOSCROLL_MAX_SPEED: f32 = 24.0;

/// Scroll speed for a pointer `overshoot` points into (or past) the edge zone.
/// Ramps up with distance so a small overshoot creeps and dragging well outside
/// the window moves quickly, the way a text editor or spreadsheet behaves.
fn autoscroll_speed(overshoot: f32) -> f32 {
    (overshoot * 0.5).clamp(1.0, AUTOSCROLL_MAX_SPEED)
}

/// Keep scrolling while the user drag-selects text past the edge of the view.
///
/// egui's `TextEdit` extends its selection to wherever the pointer is, but it
/// never scrolls the surrounding `ScrollArea`, so a selection could not be
/// dragged beyond the rows that happened to be on screen: you had to select
/// what was visible, act on it, scroll, and start again. This nudges the
/// enclosing scroll area whenever the pointer nears (or leaves) its edge during
/// a drag, so the selection can run past the bottom of the screen.
///
/// Call this **inside** the `ScrollArea`'s closure, passing the editor's
/// response; `ui.clip_rect()` there is the scroll area's visible viewport.
pub fn autoscroll_while_selecting(ui: &egui::Ui, response: &egui::Response) {
    if !response.dragged() {
        return;
    }
    // `pointer_latest_pos` (not `interact_pos`) so we keep tracking once the
    // pointer has left the widget, which is exactly when scrolling is wanted.
    let Some(pos) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    let view = ui.clip_rect();
    if !view.is_positive() {
        return;
    }

    // egui applies `scroll_with_delta` inverted: a negative y scrolls *down*
    // (towards later lines), matching the mouse-wheel convention.
    let mut delta = egui::Vec2::ZERO;
    let past_bottom = pos.y - (view.bottom() - AUTOSCROLL_MARGIN);
    let past_top = (view.top() + AUTOSCROLL_MARGIN) - pos.y;
    if past_bottom > 0.0 {
        delta.y = -autoscroll_speed(past_bottom);
    } else if past_top > 0.0 {
        delta.y = autoscroll_speed(past_top);
    }

    let past_right = pos.x - (view.right() - AUTOSCROLL_MARGIN);
    let past_left = (view.left() + AUTOSCROLL_MARGIN) - pos.x;
    if past_right > 0.0 {
        delta.x = -autoscroll_speed(past_right);
    } else if past_left > 0.0 {
        delta.x = autoscroll_speed(past_left);
    }

    if delta != egui::Vec2::ZERO {
        ui.scroll_with_delta(delta);
        // The pointer may not move again, so ask for the next frame ourselves;
        // otherwise the scroll would stall as soon as the user holds still.
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod autoscroll_tests {
    use super::*;

    #[test]
    fn speed_ramps_with_distance_and_is_capped() {
        // Just inside the edge zone: a slow creep, never zero (holding the
        // pointer on the boundary must still make progress).
        assert!(autoscroll_speed(0.5) >= 1.0);
        // Further out is faster.
        assert!(autoscroll_speed(20.0) > autoscroll_speed(4.0));
        // Dragging far outside the window does not fling the view.
        assert_eq!(autoscroll_speed(10_000.0), AUTOSCROLL_MAX_SPEED);
    }
}
