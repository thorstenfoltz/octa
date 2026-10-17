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
