//! Natural order for text: a run of digits compares by its value, so `file2`
//! sorts before `file10` and `v1.9` before `v1.10`. Everything else compares
//! as the letter-by-letter order does, upper and lower case alike.
//!
//! Digit runs are compared by length once their leading zeros are gone, then
//! digit by digit, never parsed into an integer, so a 40-digit code cannot
//! overflow. When two strings are equal apart from leading zeros, the one with
//! more zeros comes first (`007` before `7`), so the order never wobbles.

use std::cmp::Ordering;

pub fn cmp_natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a, b);
    // The first leading-zero difference, used only if nothing else decides.
    let mut tie = Ordering::Equal;
    loop {
        let (Some(cx), Some(cy)) = (x.chars().next(), y.chars().next()) else {
            // One side ran out: the shorter one first, then the zero tie.
            return x.len().cmp(&y.len()).then(tie);
        };
        if cx.is_ascii_digit() && cy.is_ascii_digit() {
            let (rx, ry) = (digit_run(x), digit_run(y));
            let (vx, vy) = (rx.trim_start_matches('0'), ry.trim_start_matches('0'));
            let by_value = vx.len().cmp(&vy.len()).then_with(|| vx.cmp(vy));
            if by_value != Ordering::Equal {
                return by_value;
            }
            if tie == Ordering::Equal {
                tie = ry.len().cmp(&rx.len());
            }
            x = &x[rx.len()..];
            y = &y[ry.len()..];
        } else {
            let by_char = cx.to_lowercase().cmp(cy.to_lowercase());
            if by_char != Ordering::Equal {
                return by_char;
            }
            x = &x[cx.len_utf8()..];
            y = &y[cy.len_utf8()..];
        }
    }
}

/// The ASCII digits at the start of `s`.
fn digit_run(s: &str) -> &str {
    let end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    &s[..end]
}

#[cfg(test)]
#[path = "natural_sort_tests.rs"]
mod tests;
