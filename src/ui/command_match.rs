//! Matching for Find a command: every typed word must appear in the name,
//! the menu or the English name, in any order, ignoring case, accents and
//! punctuation (the folding is the country matcher's, `country_match::normalise`).

use crate::data::country_match::normalise as fold;

/// `None` when a word is missing; `Some(0)` when the name starts with the
/// query, `Some(1)` otherwise.
pub fn rank(query: &str, name: &str, place: &str, english: &str) -> Option<u8> {
    let q = fold(query);
    let name = fold(name);
    let hay = format!("{name} {} {}", fold(place), fold(english));
    if !q.split_whitespace().all(|w| hay.contains(w)) {
        return None;
    }
    Some(if !q.is_empty() && name.starts_with(&q) {
        0
    } else {
        1
    })
}

#[cfg(test)]
#[path = "command_match_tests.rs"]
mod tests;
