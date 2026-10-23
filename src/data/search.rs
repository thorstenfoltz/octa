use super::SearchMode;

/// Precompiled matcher for the current search query and mode.
pub enum RowMatcher {
    Plain(String),
    /// A Regex-mode search: a replacement may refer to its groups (`$1`).
    Regex(regex::Regex),
    /// A Plain or Wildcard search that needed a regex to run (a `*`, the case
    /// or whole-word toggle). It matches like `Regex`, but a replacement is
    /// inserted as typed: `$5` in a plain replace means five dollars, not
    /// capture group five.
    Literal(regex::Regex),
    Invalid,
}

impl RowMatcher {
    pub fn new(query: &str, mode: SearchMode) -> Self {
        match mode {
            SearchMode::Plain => RowMatcher::Plain(query.to_lowercase()),
            SearchMode::Wildcard => {
                let pattern = super::wildcard_to_regex(query);
                match regex::Regex::new(&pattern) {
                    Ok(re) => RowMatcher::Literal(re),
                    Err(_) => RowMatcher::Invalid,
                }
            }
            SearchMode::Regex => match regex::Regex::new(query) {
                Ok(re) => RowMatcher::Regex(re),
                Err(_) => RowMatcher::Invalid,
            },
        }
    }
}

/// The RE2 pattern that matches exactly what [`RowMatcher::with_options`]
/// would match, for handing the same search to an engine that speaks regex
/// (DuckDB's `regexp_matches`, and through it the whole-file scan).
///
/// This exists so a scan over the file answers the *same question* the search
/// box asked. Built separately, a scan would quietly ignore the mode, the
/// `Aa` toggle and whole-word, and then report its count in the same words -
/// a confidently wrong number, which is the whole failure this feature is
/// meant to prevent.
///
/// Returns `None` for a pattern the regex engine refuses, matching
/// `RowMatcher::Invalid`'s "matches nothing" rather than sending broken SQL.
/// DuckDB's regex is RE2, the same family as the `regex` crate, so a pattern
/// that builds here behaves the same there.
pub fn sql_regex_pattern(
    query: &str,
    mode: SearchMode,
    case_sensitive: bool,
    whole_word: bool,
) -> Option<String> {
    let base = match mode {
        SearchMode::Plain => regex::escape(query),
        SearchMode::Wildcard => {
            let p = super::wildcard_to_regex(query);
            p.strip_prefix("(?i)").map(str::to_string).unwrap_or(p)
        }
        SearchMode::Regex => query.to_string(),
    };
    let pattern = if whole_word {
        format!(r"\b(?:{base})\b")
    } else {
        base
    };
    // Case folding as an inline flag rather than a builder option, because the
    // pattern has to travel to another engine as text.
    let pattern = if case_sensitive {
        pattern
    } else {
        format!("(?i){pattern}")
    };
    regex::Regex::new(&pattern).ok().map(|_| pattern)
}

impl RowMatcher {
    /// Like [`new`](Self::new) but with explicit **case-sensitive** and
    /// **whole-word** toggles (the GUI search bar's `Aa` / whole-word
    /// buttons). When both are off this matches `new`'s case-insensitive
    /// behaviour; turning either on routes through a regex so the semantics
    /// are uniform across Plain / Wildcard / Regex modes.
    pub fn with_options(
        query: &str,
        mode: SearchMode,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Self {
        // Fast path: the common plain, case-insensitive, substring search.
        if matches!(mode, SearchMode::Plain) && !case_sensitive && !whole_word {
            return RowMatcher::Plain(query.to_lowercase());
        }
        // Base pattern per mode (without any case flag - that's applied via the
        // RegexBuilder below so it composes with whole-word wrapping).
        let base = match mode {
            SearchMode::Plain => regex::escape(query),
            SearchMode::Wildcard => {
                let p = super::wildcard_to_regex(query);
                // `wildcard_to_regex` hard-codes a leading `(?i)`; strip it so
                // the case flag is controlled solely by the builder.
                p.strip_prefix("(?i)").map(str::to_string).unwrap_or(p)
            }
            SearchMode::Regex => query.to_string(),
        };
        let pattern = if whole_word {
            format!(r"\b(?:{base})\b")
        } else {
            base
        };
        match regex::RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
        {
            Ok(re) if matches!(mode, SearchMode::Regex) => RowMatcher::Regex(re),
            Ok(re) => RowMatcher::Literal(re),
            Err(_) => RowMatcher::Invalid,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        match self {
            RowMatcher::Plain(q) => text.to_lowercase().contains(q),
            RowMatcher::Regex(re) | RowMatcher::Literal(re) => re.is_match(text),
            RowMatcher::Invalid => false,
        }
    }

    /// Byte ranges of every non-overlapping match in `text`, in order.
    ///
    /// Used by the highlight search to paint matches in place. For `Plain`
    /// matching we go through a case-insensitive regex (same reasoning as
    /// `replace`: a direct byte-offset map between `to_lowercase()` and the
    /// original is unsafe for characters whose lowercase form differs in byte
    /// length, e.g. Turkish dotted-I). Zero-width matches are skipped so a
    /// pathological regex like `a*` cannot loop forever.
    pub fn find_ranges(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let re = match self {
            RowMatcher::Plain(q) => {
                if q.is_empty() {
                    return Vec::new();
                }
                let escaped = regex::escape(q);
                match regex::Regex::new(&format!("(?i){escaped}")) {
                    Ok(re) => re,
                    Err(_) => return Vec::new(),
                }
            }
            RowMatcher::Regex(re) | RowMatcher::Literal(re) => re.clone(),
            RowMatcher::Invalid => return Vec::new(),
        };
        re.find_iter(text)
            .filter(|m| m.end() > m.start())
            .map(|m| m.start()..m.end())
            .collect()
    }

    /// Replace the first match in `text` with `replacement`.
    pub fn replace(&self, text: &str, replacement: &str) -> String {
        self.replace_n(text, replacement, 1)
    }

    /// Replace **every** matching portion in `text` with `replacement`
    /// (unlike [`replace`](Self::replace), which replaces only the first
    /// match). Used by the column-wide "replace in column" transform and by
    /// Find & Replace.
    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        self.replace_n(text, replacement, 0)
    }

    /// Replace the one match at `range`, a range [`find_ranges`](Self::find_ranges)
    /// returned for this `text`. The match is replaced on its own, so a regex
    /// replacement still expands its groups.
    pub fn replace_range(
        &self,
        text: &str,
        range: std::ops::Range<usize>,
        replacement: &str,
    ) -> String {
        let Some(found) = text.get(range.clone()) else {
            return text.to_string();
        };
        format!(
            "{}{}{}",
            &text[..range.start],
            self.replace(found, replacement),
            &text[range.end..]
        )
    }

    /// `limit` 0 replaces every match. Only a Regex-mode search expands `$1`
    /// in the replacement; every other mode inserts it as typed.
    fn replace_n(&self, text: &str, replacement: &str, limit: usize) -> String {
        let (re, expand) = match self {
            // An empty search matches between every two characters; it finds
            // nothing (`find_ranges`), so it replaces nothing either.
            RowMatcher::Plain(q) if q.is_empty() => return text.to_string(),
            // Case-insensitive regex for correct Unicode handling. Direct
            // byte-offset mapping between to_lowercase() and the original is
            // unsafe for characters whose lowercase form has a different byte
            // length (e.g. Turkish dotted I).
            RowMatcher::Plain(q) => match regex::Regex::new(&format!("(?i){}", regex::escape(q))) {
                Ok(re) => (re, false),
                Err(_) => return text.to_string(),
            },
            RowMatcher::Regex(re) => (re.clone(), true),
            RowMatcher::Literal(re) => (re.clone(), false),
            RowMatcher::Invalid => return text.to_string(),
        };
        if expand {
            re.replacen(text, limit, replacement).into_owned()
        } else {
            re.replacen(text, limit, regex::NoExpand(replacement))
                .into_owned()
        }
    }
}

/// A table cell as `(row, col)`.
pub type Cell = (usize, usize);

/// Replace every match in each of `cells`, the way Replace all does. Returns
/// how many cells changed. A cell keeps its type where the new text still
/// reads as one (`CellValue::parse_like`).
pub fn replace_in_cells(
    table: &mut super::DataTable,
    cells: &[Cell],
    matcher: &RowMatcher,
    replacement: &str,
) -> usize {
    let mut changed = 0;
    for &(row, col) in cells {
        let Some(old) = table.get(row, col).cloned() else {
            continue;
        };
        let text = old.to_string();
        if !matcher.matches(&text) {
            continue;
        }
        let new = super::CellValue::parse_like(&old, &matcher.replace_all(&text, replacement));
        if new != old {
            table.set(row, col, new);
            changed += 1;
        }
    }
    changed
}

/// Where Replace next acts, given the matching cells in display order and the
/// selected cell: the selected cell when it is a match, else the first match
/// after it (wrapping), else the first match. Also returns the match to select
/// afterwards, the one after the target, so pressing Next again moves on even
/// when the replacement still matches. `order` maps a cell to its display
/// position, so "after" means after on screen.
pub fn replace_next_target(
    cells: &[Cell],
    selected: Option<Cell>,
    order: impl Fn(Cell) -> (usize, usize),
) -> Option<(Cell, Option<Cell>)> {
    if cells.is_empty() {
        return None;
    }
    let at = match selected {
        Some(sel) => cells.iter().position(|&c| c == sel).unwrap_or_else(|| {
            let key = order(sel);
            cells.iter().position(|&c| order(c) > key).unwrap_or(0)
        }),
        None => 0,
    };
    let then = (cells.len() > 1).then(|| cells[(at + 1) % cells.len()]);
    Some((cells[at], then))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod option_tests;
