//! Unit tests for [`pii`](super). Included via `#[path]` so it stays an inner
//! `tests` module with access to private helpers.

use super::*;
use crate::data::ColumnInfo;

fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.to_string())
}

fn find(found: &[ColumnPii], col: usize) -> Option<&ColumnPii> {
    found.iter().find(|f| f.column == col)
}

#[test]
fn email_detected_by_value_and_name() {
    let t = table(
        &["email"],
        vec![vec![s("a@b.com")], vec![s("c@d.org")], vec![s("e@f.net")]],
    );
    let f = scan_pii(&t, 100);
    let e = find(&f, 0).unwrap();
    assert_eq!(e.kind, PiiKind::Email);
    assert!(e.by_name && e.value_match > 0.9);
    assert!(e.confidence > 0.9);
}

#[test]
fn name_detected_by_header_only() {
    // No reliable value pattern for names; the header carries the signal.
    let t = table(
        &["first_name", "last_name"],
        vec![vec![s("Alice"), s("Smith")], vec![s("Bob"), s("Jones")]],
    );
    let f = scan_pii(&t, 100);
    assert_eq!(find(&f, 0).unwrap().kind, PiiKind::Name);
    assert_eq!(find(&f, 1).unwrap().kind, PiiKind::Name);
    // Name-only -> the 0.6 header-match floor.
    assert!((find(&f, 0).unwrap().confidence - 0.6).abs() < 1e-9);
}

#[test]
fn gender_detected_by_value_domain() {
    let t = table(
        &["g"],
        vec![vec![s("m")], vec![s("f")], vec![s("m")], vec![s("w")]],
    );
    let f = scan_pii(&t, 100);
    assert_eq!(find(&f, 0).unwrap().kind, PiiKind::Gender);
}

#[test]
fn country_and_birthdate_need_header() {
    let t = table(
        &["country", "birthdate"],
        vec![
            vec![s("Germany"), s("1990-04-12")],
            vec![s("France"), s("1985-11-03")],
        ],
    );
    let f = scan_pii(&t, 100);
    assert_eq!(find(&f, 0).unwrap().kind, PiiKind::Country);
    assert_eq!(find(&f, 1).unwrap().kind, PiiKind::BirthDate);
}

#[test]
fn salary_is_not_a_phone() {
    // Plain integers must not register as phones (the old false positive).
    let t = table(
        &["salary"],
        vec![vec![s("85000")], vec![s("92000")], vec![s("120000")]],
    );
    let f = scan_pii(&t, 100);
    assert!(find(&f, 0).is_none());
}

#[test]
fn ip_column_classified_as_ip_not_phone() {
    let t = table(
        &["ip_address"],
        vec![
            vec![s("192.168.0.1")],
            vec![s("10.0.0.5")],
            vec![s("8.8.8.8")],
        ],
    );
    let f = scan_pii(&t, 100);
    assert_eq!(find(&f, 0).unwrap().kind, PiiKind::Ip);
}

#[test]
fn phone_with_separators_detected() {
    let t = table(
        &["contact"],
        vec![vec![s("+49 151 23456789")], vec![s("(555) 123-4567")]],
    );
    let f = scan_pii(&t, 100);
    assert_eq!(find(&f, 0).unwrap().kind, PiiKind::Phone);
}

#[test]
fn plain_text_column_not_flagged() {
    let t = table(
        &["comments"],
        vec![vec![s("great product")], vec![s("works fine")]],
    );
    let f = scan_pii(&t, 100);
    assert!(find(&f, 0).is_none());
}
