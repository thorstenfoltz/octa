//! Detect web links in cell text so the table view can render them as
//! hyperlinks and open them on Ctrl+click.
//!
//! Deliberately strict: a cell counts as a link only when its whole (trimmed)
//! text is a single `http://` or `https://` URL with no interior whitespace.
//! This keeps false positives out of ordinary prose cells that merely mention a
//! URL, and keeps the check cheap on the hot render path.

/// Return the URL when `text` is, after trimming, a single `http`/`https` link;
/// otherwise `None`.
pub fn detect_url(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.len() < "http://".len() {
        return None;
    }
    let lower_ok = trimmed
        .get(..8)
        .map(|p| {
            let p = p.to_ascii_lowercase();
            p.starts_with("http://") || p.starts_with("https://")
        })
        .unwrap_or(false);
    if !lower_ok {
        return None;
    }
    // A single token: no ASCII whitespace anywhere.
    if trimmed.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    // Require at least one char after the scheme separator.
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("");
    if after_scheme.is_empty() {
        return None;
    }
    Some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_https_url() {
        assert_eq!(
            detect_url("https://example.com/path?q=1"),
            Some("https://example.com/path?q=1")
        );
    }

    #[test]
    fn http_and_surrounding_whitespace_trimmed() {
        assert_eq!(detect_url("  http://a.b  "), Some("http://a.b"));
    }

    #[test]
    fn scheme_is_case_insensitive() {
        assert_eq!(
            detect_url("HTTPS://Example.COM"),
            Some("HTTPS://Example.COM")
        );
    }

    #[test]
    fn not_a_link() {
        assert_eq!(detect_url("see https://a.b for more"), None); // interior space
        assert_eq!(detect_url("ftp://a.b"), None); // wrong scheme
        assert_eq!(detect_url("example.com"), None); // no scheme
        assert_eq!(detect_url("https://"), None); // nothing after scheme
        assert_eq!(detect_url(""), None);
        assert_eq!(detect_url("hello world"), None);
    }
}
