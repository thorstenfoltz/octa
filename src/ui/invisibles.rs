//! **View -> Show invisible characters**: draw the whitespace inside a cell as
//! markers, so `Berlin ` next to `Berlin`, a tab where a space was expected, or
//! a non-breaking space pasted from a web page can be seen instead of guessed.
//!
//! Markers are plain ASCII on purpose: egui's bundled font draws the usual
//! `·` and `→` as empty boxes.

use std::ops::Range;

/// What a marked stretch of a cell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invisible {
    /// A space between words. Shown, but not flagged.
    Space,
    /// A space at the very start or end of the value: the usual reason two
    /// values that look equal do not match.
    EdgeSpace,
    Tab,
    /// A space character that is not the plain one: non-breaking, narrow,
    /// em / en, ideographic.
    OddSpace,
    /// A character with no width at all: zero-width space and joiners, the
    /// byte order mark, the word joiner.
    ZeroWidth,
    CarriageReturn,
}

impl Invisible {
    /// The ASCII drawn in place of the character.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Space | Self::EdgeSpace => ".",
            Self::Tab => "->",
            Self::OddSpace => "_",
            Self::ZeroWidth => "|",
            Self::CarriageReturn => "CR",
        }
    }

    /// Whether the marker gets a warning background: everything except a
    /// plain space between words and a tab, which are only shown.
    pub fn is_suspicious(self) -> bool {
        !matches!(self, Self::Space | Self::Tab)
    }
}

fn classify(c: char) -> Option<Invisible> {
    match c {
        ' ' => Some(Invisible::Space),
        '\t' => Some(Invisible::Tab),
        '\r' => Some(Invisible::CarriageReturn),
        '\u{00A0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}' => Some(Invisible::OddSpace),
        '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}' => Some(Invisible::ZeroWidth),
        _ => None,
    }
}

/// Whether `text` holds anything this view would mark. Cheap enough to call
/// for every visible cell each frame.
pub fn has_invisibles(text: &str) -> bool {
    text.chars().any(|c| classify(c).is_some())
}

/// `text` cut into runs: `(byte range, None)` for ordinary text and
/// `(byte range of one character, Some(kind))` for each marked one.
pub fn split_invisibles(text: &str) -> Vec<(Range<usize>, Option<Invisible>)> {
    let first = text.find(|c: char| c != ' ').unwrap_or(text.len());
    let last = text.rfind(|c: char| c != ' ').map_or(0, |i| i + 1);
    let mut out = Vec::new();
    let mut plain_start = 0;
    for (i, c) in text.char_indices() {
        let Some(mut kind) = classify(c) else {
            continue;
        };
        if kind == Invisible::Space && (i < first || i >= last) {
            kind = Invisible::EdgeSpace;
        }
        if plain_start < i {
            out.push((plain_start..i, None));
        }
        out.push((i..i + c.len_utf8(), Some(kind)));
        plain_start = i + c.len_utf8();
    }
    if plain_start < text.len() {
        out.push((plain_start..text.len(), None));
    }
    out
}

/// The cell's text as a layout job with every invisible drawn as its marker:
/// markers in `marker_color`, suspicious ones on `warn_bg`.
pub fn invisibles_job(
    text: &str,
    font: egui::FontId,
    text_color: egui::Color32,
    marker_color: egui::Color32,
    warn_bg: egui::Color32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (range, kind) in split_invisibles(text) {
        match kind {
            None => job.append(
                &text[range],
                0.0,
                egui::TextFormat::simple(font.clone(), text_color),
            ),
            Some(k) => job.append(
                k.marker(),
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: marker_color,
                    background: if k.is_suspicious() {
                        warn_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    },
                    ..Default::default()
                },
            ),
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Option<Invisible>> {
        split_invisibles(text).into_iter().map(|(_, k)| k).collect()
    }

    #[test]
    fn edge_spaces_are_flagged_inner_ones_are_not() {
        use Invisible::*;
        assert_eq!(
            kinds(" a b "),
            vec![Some(EdgeSpace), None, Some(Space), None, Some(EdgeSpace)]
        );
    }

    #[test]
    fn tab_and_space_gaps_look_different() {
        let spaces: Vec<_> = split_invisibles("a  b")
            .iter()
            .filter_map(|(_, k)| k.map(Invisible::marker))
            .collect();
        let tab: Vec<_> = split_invisibles("a\tb")
            .iter()
            .filter_map(|(_, k)| k.map(Invisible::marker))
            .collect();
        assert_eq!(spaces, vec![".", "."]);
        assert_eq!(tab, vec!["->"]);
    }

    #[test]
    fn odd_and_zero_width_characters_are_suspicious() {
        let found: Vec<_> = split_invisibles("a\u{00A0}b\u{200B}c\r")
            .into_iter()
            .filter_map(|(_, k)| k)
            .collect();
        assert_eq!(
            found,
            vec![
                Invisible::OddSpace,
                Invisible::ZeroWidth,
                Invisible::CarriageReturn
            ]
        );
        assert!(found.iter().all(|k| k.is_suspicious()));
    }

    #[test]
    fn ranges_cover_the_text_and_plain_text_has_none() {
        let text = "x\u{00A0}yz ";
        let total: usize = split_invisibles(text).iter().map(|(r, _)| r.len()).sum();
        assert_eq!(total, text.len());
        assert!(!has_invisibles("plain"));
        assert!(has_invisibles("two words"));
    }
}
