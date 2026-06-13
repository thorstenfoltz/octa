//! Unit tests for [`encoding`](encoding). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn plain_utf8_is_passthrough() {
    let (s, enc) = decode_bytes("héllo".as_bytes());
    assert_eq!(s, "héllo");
    assert_eq!(enc, "UTF-8");
}

#[test]
fn windows_1252_is_detected() {
    // 0xE9 is 'é' in Windows-1252 / Latin-1 but invalid lone UTF-8.
    let bytes = b"caf\xe9 r\xe9sum\xe9 na\xefve text here";
    let (s, _enc) = decode_bytes(bytes);
    assert!(s.contains("café"), "decoded: {s}");
    assert!(
        !s.contains('\u{FFFD}'),
        "should not contain replacement chars"
    );
}

#[test]
fn utf16le_bom_is_decoded() {
    // "Hi" in UTF-16LE with BOM.
    let bytes = [0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
    let (s, enc) = decode_bytes(&bytes);
    assert_eq!(s, "Hi");
    assert_eq!(enc, "UTF-16LE");
}

#[test]
fn utf8_bom_is_stripped() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"data");
    let (s, _enc) = decode_bytes(&bytes);
    assert_eq!(s, "data");
}
