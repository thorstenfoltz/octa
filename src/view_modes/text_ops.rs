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
fn char_range_to_byte_range(s: &str, start: usize, end: usize) -> std::ops::Range<usize> {
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
/// selection — if nothing is selected the buffer is left untouched and the
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_lower_apply_helpers() {
        assert_eq!(CaseOp::Upper.apply("abc"), "ABC");
        assert_eq!(CaseOp::Lower.apply("XYZ"), "xyz");
    }

    #[test]
    fn byte_range_basic() {
        let s = "hello";
        let r = char_range_to_byte_range(s, 1, 4);
        assert_eq!(r, 1..4);
        assert_eq!(&s[r], "ell");
    }

    #[test]
    fn byte_range_unicode() {
        let s = "héllo";
        // chars: h, é, l, l, o
        let r = char_range_to_byte_range(s, 1, 4);
        // 'é' is 2 bytes in UTF-8.
        assert_eq!(&s[r], "éll");
    }

    #[test]
    fn byte_range_clamped_at_end() {
        let s = "abc";
        let r = char_range_to_byte_range(s, 0, 3);
        assert_eq!(&s[r], "abc");
    }
}
