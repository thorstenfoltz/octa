use super::*;

fn table(name: &str, values: &[Option<&str>]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.to_string(),
        data_type: "Utf8".to_string(),
    }];
    t.rows = values
        .iter()
        .map(|v| {
            vec![match v {
                Some(s) => CellValue::String((*s).to_string()),
                None => CellValue::Null,
            }]
        })
        .collect();
    t
}

#[test]
fn every_child_matching_a_parent_is_clean() {
    let parent = table("id", &[Some("1"), Some("2"), Some("3")]);
    let child = table("parent_id", &[Some("1"), Some("2"), Some("2")]);
    let r = check(&parent, 0, &child, 0);
    assert!(r.is_clean());
    assert_eq!(r.checked_rows, 3);
    assert_eq!(r.parent_values, 3);
    assert!(r.orphans.is_empty());
}

#[test]
fn orphans_are_named_and_counted() {
    let parent = table("id", &[Some("1"), Some("2")]);
    let child = table(
        "parent_id",
        &[Some("1"), Some("9"), Some("9"), Some("9"), Some("7")],
    );
    let r = check(&parent, 0, &child, 0);
    assert!(!r.is_clean());
    assert_eq!(r.orphan_rows, 4);
    assert_eq!(r.orphan_values, 2);
    // Most rows first.
    assert_eq!(
        r.orphans,
        vec![
            Orphan {
                value: "9".into(),
                rows: 3
            },
            Orphan {
                value: "7".into(),
                rows: 1
            },
        ]
    );
    assert!(r.sentence().contains("4 child row"));
}

/// The convention every relational database uses: a null foreign key means
/// "no parent", not "a parent that went missing".
#[test]
fn a_null_key_is_not_an_orphan() {
    let parent = table("id", &[Some("1")]);
    let child = table("parent_id", &[Some("1"), None, None]);
    let r = check(&parent, 0, &child, 0);
    assert!(r.is_clean());
    assert_eq!(r.null_keys, 2);
    assert_eq!(r.checked_rows, 1);
    assert!(r.sentence().starts_with("No orphans"));
}

/// An empty string is a missing key too, not a key whose value happens to
/// be nothing: a CSV writes an absent value that way.
#[test]
fn an_empty_string_counts_as_no_key() {
    let parent = table("id", &[Some("1")]);
    let child = table("parent_id", &[Some(""), Some("  ")]);
    let r = check(&parent, 0, &child, 0);
    assert_eq!(r.null_keys, 2);
    assert_eq!(r.checked_rows, 0);
}

/// One side out of a CSV and the other out of a database is the common
/// case, and `1` has to match `1` across that boundary.
#[test]
fn a_number_matches_its_text_form() {
    let mut parent = DataTable::empty();
    parent.columns = vec![ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    parent.rows = vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]];
    let child = table("parent_id", &[Some("1"), Some(" 2 ")]);
    assert!(check(&parent, 0, &child, 0).is_clean());
}

#[test]
fn the_list_is_capped_but_the_counts_are_not() {
    let parent = table("id", &[Some("keep")]);
    let owned: Vec<String> = (0..MAX_LISTED + 50).map(|i| format!("x{i}")).collect();
    let child = table(
        "parent_id",
        &owned.iter().map(|s| Some(s.as_str())).collect::<Vec<_>>(),
    );
    let r = check(&parent, 0, &child, 0);
    assert_eq!(r.orphan_rows, MAX_LISTED + 50, "the count stays exact");
    assert_eq!(r.orphan_values, MAX_LISTED + 50);
    assert_eq!(r.orphans.len(), MAX_LISTED, "only the list is capped");
}

#[test]
fn the_report_table_lists_one_row_per_value() {
    let parent = table("id", &[Some("1")]);
    let child = table("parent_id", &[Some("9"), Some("9")]);
    let t = report_table("orders", &check(&parent, 0, &child, 0));
    assert_eq!(t.row_count(), 1);
    assert_eq!(t.get(0, 0), Some(&CellValue::String("orders".into())));
    assert_eq!(t.get(0, 1), Some(&CellValue::String("9".into())));
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(2)));
}
