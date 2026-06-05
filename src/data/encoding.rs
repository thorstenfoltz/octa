//! Best-effort text decoding for non-UTF-8 files.
//!
//! Real-world text exports (especially from non-English Windows tools) are
//! often Windows-1252 / Latin-1 or UTF-16 with a BOM rather than UTF-8. Reading
//! them with `String::from_utf8` fails outright, and `from_utf8_lossy` fills the
//! file with U+FFFD replacement characters. This module sniffs a BOM first,
//! takes the UTF-8 fast path when the bytes are already valid UTF-8, and
//! otherwise detects the encoding with `chardetng` and decodes via
//! `encoding_rs`, so such files open as readable text.

use encoding_rs::Encoding;

/// Decode `bytes` into a `String`, auto-detecting the encoding. Returns the
/// decoded text plus the name of the encoding used (e.g. `"UTF-8"`,
/// `"windows-1252"`, `"UTF-16LE"`). Valid UTF-8 is the fast path and skips
/// detection entirely.
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static str) {
    // 1. A BOM is decisive (UTF-8 / UTF-16LE / UTF-16BE). `decode` strips it.
    if let Some((enc, _bom_len)) = Encoding::for_bom(bytes) {
        let (text, _, _) = enc.decode(bytes);
        return (text.into_owned(), enc.name());
    }
    // 2. Valid UTF-8 without a BOM: return as-is, no detection cost.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_owned(), "UTF-8");
    }
    // 3. Detect with chardetng, then decode (lossy only for stray bad bytes).
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let enc = detector.guess(None, true);
    let (text, _, _) = enc.decode(bytes);
    (text.into_owned(), enc.name())
}

/// Read a file and decode it to text, auto-detecting the encoding. Thin wrapper
/// over [`decode_bytes`] for readers that previously called
/// `std::fs::read_to_string`.
pub fn read_to_string_detected(path: &std::path::Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(decode_bytes(&bytes).0)
}

#[cfg(test)]
mod tests {
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
}
