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
