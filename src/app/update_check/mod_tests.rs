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
fn parse_release_strips_the_tag_prefix_and_trims_the_notes() {
    let (version, notes) =
        parse_release(r#"{"tag_name": "v0.17.0", "body": "Added a thing\n\n"}"#).unwrap();
    assert_eq!(version, "0.17.0");
    assert_eq!(notes, "Added a thing");
}

#[test]
fn parse_release_tolerates_a_release_published_without_notes() {
    // GitHub omits `body`, or sends null, for a release with no description.
    // Neither may fail the check: the tag is the part that decides whether an
    // update exists at all.
    for json in [
        r#"{"tag_name": "1.0.0"}"#,
        r#"{"tag_name": "1.0.0", "body": null}"#,
        r#"{"tag_name": "1.0.0", "body": ""}"#,
    ] {
        let (version, notes) = parse_release(json).unwrap();
        assert_eq!(version, "1.0.0", "for {json}");
        assert!(notes.is_empty(), "for {json}");
    }
}

#[test]
fn parse_release_rejects_a_response_without_a_tag() {
    assert!(parse_release(r#"{"body": "notes only"}"#).is_err());
    assert!(parse_release("not json at all").is_err());
}
