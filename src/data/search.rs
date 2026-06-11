use super::SearchMode;

/// Precompiled matcher for the current search query and mode.
pub enum RowMatcher {
    Plain(String),
    Regex(regex::Regex),
    Invalid,
}

impl RowMatcher {
    pub fn new(query: &str, mode: SearchMode) -> Self {
        match mode {
            SearchMode::Plain => RowMatcher::Plain(query.to_lowercase()),
            SearchMode::Wildcard => {
                let pattern = super::wildcard_to_regex(query);
                match regex::Regex::new(&pattern) {
                    Ok(re) => RowMatcher::Regex(re),
                    Err(_) => RowMatcher::Invalid,
                }
            }
            SearchMode::Regex => match regex::Regex::new(query) {
                Ok(re) => RowMatcher::Regex(re),
                Err(_) => RowMatcher::Invalid,
            },
        }
    }

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
            Ok(re) => RowMatcher::Regex(re),
            Err(_) => RowMatcher::Invalid,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        match self {
            RowMatcher::Plain(q) => text.to_lowercase().contains(q),
            RowMatcher::Regex(re) => re.is_match(text),
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
            RowMatcher::Regex(re) => re.clone(),
            RowMatcher::Invalid => return Vec::new(),
        };
        re.find_iter(text)
            .filter(|m| m.end() > m.start())
            .map(|m| m.start()..m.end())
            .collect()
    }

    /// Replace matching portion(s) in `text` with `replacement`.
    pub fn replace(&self, text: &str, replacement: &str) -> String {
        match self {
            RowMatcher::Plain(q) => {
                // Use case-insensitive regex for correct Unicode handling.
                // Direct byte-offset mapping between to_lowercase() and the
                // original string is unsafe for characters whose lowercase
                // form has a different byte length (e.g. Turkish İ).
                let escaped = regex::escape(q);
                match regex::Regex::new(&format!("(?i){escaped}")) {
                    Ok(re) => re.replace(text, replacement).to_string(),
                    Err(_) => text.to_string(),
                }
            }
            RowMatcher::Regex(re) => re.replace(text, replacement).to_string(),
            RowMatcher::Invalid => text.to_string(),
        }
    }
}

#[cfg(test)]
mod option_tests {
    use super::*;

    #[test]
    fn case_sensitive_plain() {
        let cs = RowMatcher::with_options("Foo", SearchMode::Plain, true, false);
        assert!(cs.matches("a Foo b"));
        assert!(!cs.matches("a foo b"));
        let ci = RowMatcher::with_options("Foo", SearchMode::Plain, false, false);
        assert!(ci.matches("a foo b"));
    }

    #[test]
    fn whole_word_plain() {
        let ww = RowMatcher::with_options("cat", SearchMode::Plain, false, true);
        assert!(ww.matches("the cat sat"));
        assert!(!ww.matches("category"));
        assert!(!ww.matches("scatter"));
    }

    #[test]
    fn whole_word_and_case_together() {
        let m = RowMatcher::with_options("ID", SearchMode::Plain, true, true);
        assert!(m.matches("the ID here"));
        assert!(!m.matches("the id here"));
        assert!(!m.matches("IDENT"));
    }

    #[test]
    fn defaults_match_new_for_plain() {
        // both off == case-insensitive substring, same as `new`.
        let m = RowMatcher::with_options("bar", SearchMode::Plain, false, false);
        assert!(m.matches("BARimba"));
    }
}
