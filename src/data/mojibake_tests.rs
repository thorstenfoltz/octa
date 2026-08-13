//! Unit tests for [`mojibake`](mojibake). Split out of the source file and
//! included back via `#[path]`, matching `schema_drift` and `fuzzy_join`.

use super::*;

#[test]
fn repairs_the_common_german_case() {
    assert_eq!(repair("MÃ¼ller").as_deref(), Some("Müller"));
    assert_eq!(repair("KÃ¶ln").as_deref(), Some("Köln"));
    assert_eq!(repair("StraÃŸe").as_deref(), Some("Straße"));
}

#[test]
fn repairs_smart_punctuation() {
    assert_eq!(repair("itâ€™s").as_deref(), Some("it’s"));
}

#[test]
fn leaves_clean_text_alone() {
    assert_eq!(repair("Müller"), None);
    assert_eq!(repair("plain ascii"), None);
    assert_eq!(repair(""), None);
}

#[test]
fn refuses_text_it_could_not_have_produced() {
    // Anything above U+00FF cannot be the product of re-decoding single-byte
    // data as UTF-8, so the reversal must refuse rather than mangle it.
    assert_eq!(repair("日本語"), None);
    assert_eq!(repair("Ünicode ok"), None);
}

#[test]
fn refuses_when_the_reversal_would_not_decode() {
    // A lone `Ã` trips the signature but the byte round-trip is not valid
    // UTF-8, so there is nothing provable to repair.
    assert_eq!(repair("Ã"), None);
}

#[test]
fn scan_reports_count_and_before_after_pairs() {
    let vals = vec![
        CellValue::String("MÃ¼ller".to_string()),
        CellValue::String("Schmidt".to_string()),
        CellValue::String("KÃ¶ln".to_string()),
        CellValue::Int(42),
    ];
    let got = scan_column(&vals);
    assert_eq!(got.affected, 2);
    assert_eq!(
        got.examples[0],
        ("MÃ¼ller".to_string(), "Müller".to_string())
    );
}

#[test]
fn scan_examples_are_capped_and_deterministic() {
    let vals: Vec<CellValue> = (0..10)
        .map(|i| CellValue::String(format!("KÃ¶ln {i}")))
        .collect();
    let a = scan_column(&vals);
    let b = scan_column(&vals);
    assert_eq!(a.affected, 10);
    assert_eq!(a.examples.len(), 3, "capped at three worked examples");
    assert_eq!(a.examples, b.examples, "same input, same examples");
}
