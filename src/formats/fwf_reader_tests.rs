//! Unit tests for [`fwf_reader`](fwf_reader). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use std::path::Path;

#[test]
fn parses_aligned_columns() {
    let content = "name    age  city\nAlice   30   Berlin\nBob     7    Rome\n";
    let table = read_fwf(content, Path::new("t.fwf")).unwrap();
    assert_eq!(
        table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["name", "age", "city"]
    );
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.get(0, 0), Some(&CellValue::String("Alice".into())));
    assert_eq!(table.get(0, 2), Some(&CellValue::String("Berlin".into())));
    assert_eq!(table.get(1, 1), Some(&CellValue::String("7".into())));
}

#[test]
fn blank_header_field_gets_fallback_name() {
    // Second column header is blank but data is present.
    let content = "id        \n1       xx\n2       yy\n";
    let table = read_fwf(content, Path::new("t.fwf")).unwrap();
    assert_eq!(table.columns[0].name, "id");
    assert_eq!(table.columns[1].name, "col_2");
}

#[test]
fn empty_input_is_empty_table() {
    let table = read_fwf("", Path::new("t.fwf")).unwrap();
    assert_eq!(table.row_count(), 0);
}
