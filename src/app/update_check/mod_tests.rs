//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn sha256_hex_matches_known_vector() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn parse_sha256sums_accepts_plain_and_binary_marked_lines() {
    let text = "\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  octa-1.0-linux-x86_64.tar.gz
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *octa-1.0-windows-x86_64.zip
not a checksum line
deadbeef  too_short_hash.txt
";
    let sums = parse_sha256sums(text);
    assert_eq!(sums.len(), 2);
    assert_eq!(
        sums.get("octa-1.0-linux-x86_64.tar.gz").map(String::as_str),
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(
        sums.get("octa-1.0-windows-x86_64.zip").map(String::as_str),
        Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
    );
}

#[test]
fn parse_sha256sums_lowercases_hashes() {
    let text = "ABC3456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF  x.zip\n";
    let sums = parse_sha256sums(text);
    assert_eq!(
        sums.get("x.zip").map(String::as_str),
        Some("abc3456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
}

#[test]
fn parse_release_reads_the_tag_from_the_redirect() {
    for (location, want) in [
        (
            "https://github.com/thorstenfoltz/octa/releases/tag/0.21.2",
            "0.21.2",
        ),
        (
            "https://github.com/thorstenfoltz/octa/releases/tag/v0.17.0",
            "0.17.0",
        ),
    ] {
        assert_eq!(parse_release(location).unwrap(), want, "for {location}");
    }
}

#[test]
fn parse_release_rejects_an_address_without_a_tag() {
    // A repository with no release yet redirects to the release list.
    assert!(parse_release("https://github.com/thorstenfoltz/octa/releases").is_err());
    assert!(parse_release("https://github.com/thorstenfoltz/octa/releases/tag/").is_err());
}

#[test]
fn only_a_higher_version_is_an_update() {
    // The copy already on the newest release, or ahead of what GitHub calls
    // latest, must hear nothing.
    assert!(!is_newer("0.21.2", "0.21.2"));
    assert!(!is_newer("0.21.2", "0.21.3"));
    assert!(!is_newer("0.21.9", "0.21.10"));
    assert!(is_newer("0.21.3", "0.21.2"));
    assert!(is_newer("0.21.10", "0.21.9"));
    assert!(is_newer("0.22.0", "0.21.9"));
    assert!(!is_newer("0.21.9", "0.22.0"));
    assert!(is_newer("1.0.0", "0.99.99"));
}
