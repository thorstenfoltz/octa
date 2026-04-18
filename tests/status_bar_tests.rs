use octa::data::{ColumnInfo, DataTable};
use octa::ui::status_bar::{format_number, parse_nav_input};
use std::collections::HashMap;

#[test]
fn test_format_number_zero() {
    assert_eq!(format_number(0), "0");
}

#[test]
fn test_format_number_small() {
    assert_eq!(format_number(1), "1");
    assert_eq!(format_number(12), "12");
    assert_eq!(format_number(999), "999");
}

#[test]
fn test_format_number_thousands() {
    assert_eq!(format_number(1_000), "1,000");
    assert_eq!(format_number(1_234), "1,234");
    assert_eq!(format_number(12_345), "12,345");
    assert_eq!(format_number(999_999), "999,999");
}

#[test]
fn test_format_number_millions() {
    assert_eq!(format_number(1_000_000), "1,000,000");
    assert_eq!(format_number(1_234_567), "1,234,567");
    assert_eq!(format_number(123_456_789), "123,456,789");
}

// --- Navigation input parsing tests ---

fn nav_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![
            vec![
                octa::data::CellValue::Int(1),
                octa::data::CellValue::String("a".into()),
                octa::data::CellValue::Float(1.0),
            ],
            vec![
                octa::data::CellValue::Int(2),
                octa::data::CellValue::String("b".into()),
                octa::data::CellValue::Float(2.0),
            ],
            vec![
                octa::data::CellValue::Int(3),
                octa::data::CellValue::String("c".into()),
                octa::data::CellValue::Float(3.0),
            ],
        ],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

#[test]
fn test_nav_row_col() {
    let table = nav_table();
    // R2:C3 → row 1, col 2 (0-indexed)
    assert_eq!(parse_nav_input("R2:C3", &table), Some((1, 2)));
    assert_eq!(parse_nav_input("2:3", &table), Some((1, 2)));
    assert_eq!(parse_nav_input("R1:C1", &table), Some((0, 0)));
}

#[test]
fn test_nav_row_only() {
    let table = nav_table();
    assert_eq!(parse_nav_input("R2", &table), Some((1, 0)));
    assert_eq!(parse_nav_input("r3", &table), Some((2, 0)));
    assert_eq!(parse_nav_input("1", &table), Some((0, 0)));
}

#[test]
fn test_nav_col_only() {
    let table = nav_table();
    assert_eq!(parse_nav_input("C2", &table), Some((0, 1)));
    assert_eq!(parse_nav_input("c1", &table), Some((0, 0)));
}

#[test]
fn test_nav_col_by_name() {
    let table = nav_table();
    assert_eq!(parse_nav_input("name", &table), Some((0, 1)));
    assert_eq!(parse_nav_input("Score", &table), Some((0, 2)));
    assert_eq!(parse_nav_input("ID", &table), Some((0, 0)));
}

#[test]
fn test_nav_col_name_in_row_col() {
    let table = nav_table();
    // R2:name → row 1, col 1
    assert_eq!(parse_nav_input("R2:name", &table), Some((1, 1)));
    assert_eq!(parse_nav_input("3:score", &table), Some((2, 2)));
}

#[test]
fn test_nav_out_of_range() {
    let table = nav_table();
    assert_eq!(parse_nav_input("R0:C1", &table), None);
    assert_eq!(parse_nav_input("R99:C1", &table), None);
    assert_eq!(parse_nav_input("R1:C99", &table), None);
    assert_eq!(parse_nav_input("0", &table), None);
}

#[test]
fn test_nav_empty() {
    let table = nav_table();
    assert_eq!(parse_nav_input("", &table), None);
    assert_eq!(parse_nav_input("  ", &table), None);
}

#[test]
fn test_nav_unknown_name() {
    let table = nav_table();
    assert_eq!(parse_nav_input("nonexistent", &table), None);
}
