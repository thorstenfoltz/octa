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
