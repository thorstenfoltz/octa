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
