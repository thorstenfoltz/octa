use super::*;

/// Build a table from a grid of `Option<&str>`, where `None` is a null.
fn table(cols: &[&str], rows: &[&[Option<&str>]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| match c {
                    Some(s) => CellValue::String((*s).to_string()),
                    None => CellValue::Null,
                })
                .collect()
        })
        .collect();
    t
}

/// Eight rows where `b` and `c` are null together, which is the finding,
/// and two where only `b` is - which is not.
fn joined_badly() -> DataTable {
    let mut rows: Vec<Vec<Option<&str>>> = Vec::new();
    for _ in 0..8 {
        rows.push(vec![Some("x"), None, None]);
    }
    for _ in 0..2 {
        rows.push(vec![Some("x"), None, Some("y")]);
    }
    for _ in 0..10 {
        rows.push(vec![Some("x"), Some("y"), Some("z")]);
    }
    let refs: Vec<&[Option<&str>]> = rows.iter().map(|r| r.as_slice()).collect();
    table(&["a", "b", "c"], &refs)
}

#[test]
fn columns_null_together_are_one_pattern() {
    let found = find_patterns(&joined_badly());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].columns, vec![1, 2]);
    assert_eq!(found[0].rows, 8);
    assert!((found[0].share - 0.4).abs() < 1e-9);
}

/// The whole point: a single column being null is the null percentage the
/// main report already carries, not a pattern.
#[test]
fn a_single_null_column_is_not_a_pattern() {
    let mut rows: Vec<Vec<Option<&str>>> = Vec::new();
    for _ in 0..20 {
        rows.push(vec![Some("x"), None]);
    }
    let refs: Vec<&[Option<&str>]> = rows.iter().map(|r| r.as_slice()).collect();
    assert!(find_patterns(&table(&["a", "b"], &refs)).is_empty());
}

#[test]
fn a_clean_table_has_no_section() {
    let t = table(
        &["a", "b"],
        &[&[Some("1"), Some("2")], &[Some("3"), Some("4")]],
    );
    assert!(find_patterns(&t).is_empty());
    assert!(section_table(&t).is_none());
}

/// An empty string is a hole too: CSV readers differ on whether a blank
/// field arrives as `Null` or as `String("")`, and the user sees the same
/// gap either way.
#[test]
fn empty_strings_count_as_missing() {
    let mut rows: Vec<Vec<Option<&str>>> = Vec::new();
    for _ in 0..6 {
        rows.push(vec![Some("x"), Some(""), Some("")]);
    }
    let refs: Vec<&[Option<&str>]> = rows.iter().map(|r| r.as_slice()).collect();
    let found = find_patterns(&table(&["a", "b", "c"], &refs));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].columns, vec![1, 2]);
}

/// A few stray rows are not a pattern, however clean the rest is.
#[test]
fn a_handful_of_rows_is_below_the_floor() {
    let mut rows: Vec<Vec<Option<&str>>> = Vec::new();
    for _ in 0..3 {
        rows.push(vec![None, None]);
    }
    for _ in 0..97 {
        rows.push(vec![Some("x"), Some("y")]);
    }
    let refs: Vec<&[Option<&str>]> = rows.iter().map(|r| r.as_slice()).collect();
    assert!(find_patterns(&table(&["a", "b"], &refs)).is_empty());
}

#[test]
fn the_section_names_the_columns() {
    let out = section_table(&joined_badly()).expect("a section");
    assert_eq!(out.row_count(), 1);
    assert_eq!(out.get(0, 0), Some(&CellValue::String("b, c".to_string())));
    assert_eq!(out.get(0, 1), Some(&CellValue::Int(2)));
    assert_eq!(out.get(0, 2), Some(&CellValue::Int(8)));
    assert_eq!(out.get(0, 3), Some(&CellValue::Float(40.0)));
}
