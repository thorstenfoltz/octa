//! Repair text that was decoded with the wrong character set and then stored
//! as valid UTF-8: `Ã¤` where `ä` belongs, `â€™` for a right single quote.
//!
//! This is invisible to [`crate::data::encoding`], which chooses an encoding
//! while *reading* a file. By the time the damage is in the file the bytes are
//! legal UTF-8, so nothing flags them and every later reader faithfully
//! reproduces the mess.
//!
//! The repair is a **verified round-trip, never a guess**: map each character
//! back to the single byte it must have come from, decode those bytes as UTF-8,
//! and accept the result only if it decodes cleanly and no corruption signature
//! survives. Anything that fails those checks is left alone and reported, which
//! is the whole safety property here: a "repair" that mangles good data is
//! worse than the corruption it was aiming at.

use crate::data::CellValue;

/// Worked examples kept per column, for the UI to show before anything is
/// applied.
const MAX_EXAMPLES: usize = 3;

/// Whether `s` carries a signature of Windows-1252 or Latin-1 text that was
/// re-decoded as UTF-8.
///
/// `Ã` covers the accented-letter cases (`Ã¤` `Ã¶` `Ã¼` `ÃŸ` ...), `â€` the
/// smart-punctuation ones (quotes, dashes), `Â` the stray non-breaking space
/// before punctuation, and `ï»¿` a byte-order mark that came through as text.
pub fn looks_corrupted(s: &str) -> bool {
    s.contains('Ã') || s.contains("â€") || s.contains('Â') || s.contains("ï»¿")
}

/// Reverse the corruption, or `None` when `s` is clean or the reversal cannot
/// be proven.
pub fn repair(s: &str) -> Option<String> {
    if !looks_corrupted(s) {
        return None;
    }
    // Encode back to the single-byte form the text must have come from.
    //
    // This has to go through Windows-1252 rather than a plain `ch as u8`: the
    // usual culprit is cp1252, whose 0x80..=0x9F range maps to characters well
    // above U+00FF (0x9F is `Ÿ` U+0178, 0x80 is `€` U+20AC, 0x99 is `™`
    // U+2122). A Latin-1 style cast rejects exactly the cases that matter,
    // `StraÃŸe` and `â€™` among them.
    //
    // `encode` substitutes HTML character references for anything cp1252 cannot
    // represent and reports it, so `had_errors` is the guard against mangling
    // text that never came from cp1252 in the first place.
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(s);
    if had_errors {
        return None;
    }
    let fixed = String::from_utf8(bytes.into_owned()).ok()?;
    // Refuse a no-op, and refuse a "repair" that leaves damage behind: both
    // mean the reversal did not actually explain the input.
    if fixed == s || looks_corrupted(&fixed) {
        return None;
    }
    Some(fixed)
}

/// What a scan of one column found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColumnMojibake {
    /// Cells that would change. A real count, never an estimate.
    pub affected: usize,
    /// Up to [`MAX_EXAMPLES`] `(before, after)` pairs, in row order so a rescan
    /// of unchanged data returns the same examples.
    pub examples: Vec<(String, String)>,
}

/// Scan one column's values.
pub fn scan_column(values: &[CellValue]) -> ColumnMojibake {
    let mut out = ColumnMojibake::default();
    for v in values {
        let CellValue::String(s) = v else { continue };
        if let Some(fixed) = repair(s) {
            out.affected += 1;
            if out.examples.len() < MAX_EXAMPLES {
                out.examples.push((s.clone(), fixed));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "mojibake_tests.rs"]
mod tests;
