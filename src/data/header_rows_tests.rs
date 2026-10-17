//! Unit tests for the header engine. Included via `#[path]` from `header_rows.rs`.

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn s(v: &str) -> CellValue {
    CellValue::String(v.to_string())
}

fn table(names: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = names
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

fn names(t: &DataTable) -> Vec<String> {
    t.columns.iter().map(|c| c.name.clone()).collect()
}

fn types(t: &DataTable) -> Vec<String> {
    t.columns.iter().map(|c| c.data_type.clone()).collect()
}

fn values(t: &DataTable) -> Vec<Vec<CellValue>> {
    (0..t.row_count())
        .map(|r| {
            (0..t.col_count())
                .map(|c| t.get(r, c).cloned().unwrap())
                .collect()
        })
        .collect()
}

/// A CSV with a title line, the way the widened reader returns it.
fn titled() -> DataTable {
    table(
        &["Report generated 03.10.2026", "column_2", "column_3"],
        vec![
            vec![s("Region"), s("Amount"), s("Share")],
            vec![s("North"), CellValue::Int(10), CellValue::Float(0.5)],
            vec![s("South"), CellValue::Int(20), CellValue::Float(0.25)],
        ],
    )
}

#[test]
fn one_row_becomes_the_header_and_rows_above_go() {
    let mut t = titled();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(names(&t), ["Region", "Amount", "Share"]);
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.get(0, 0), Some(&s("North")));
}

#[test]
fn types_are_detected_again() {
    let mut t = titled();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(types(&t), ["Utf8", "Int64", "Float64"]);
}

#[test]
fn european_numbers_become_numbers() {
    let mut t = table(
        &["Title", "column_2"],
        vec![
            vec![s("Name"), s("Amount")],
            vec![s("A"), s("1.234,56")],
            vec![s("B"), s("7,5")],
        ],
    );
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(types(&t)[1], "Float64");
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(1234.56)));
}

#[test]
fn dates_become_dates() {
    let mut t = table(
        &["Export", "column_2"],
        vec![
            vec![s("Day"), s("Value")],
            vec![s("31.12.2024"), CellValue::Int(1)],
            vec![s("15.01.2025"), CellValue::Int(2)],
        ],
    );
    let mut off = t.clone();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(types(&t)[0], "Date32");
    // With date detection switched off the column stays text.
    use_header_rows(&mut off, 0, 0, false).unwrap();
    assert_eq!(types(&off)[0], "Utf8");
}

#[test]
fn two_rows_combine_and_merged_gaps_fill_from_the_left() {
    let mut t = table(
        &["Title", "column_2", "column_3", "column_4", "column_5"],
        vec![
            vec![
                CellValue::Null,
                s("2025"),
                CellValue::Null,
                s("2026"),
                CellValue::Null,
            ],
            vec![s("Region"), s("Q1"), s("Q2"), s("Q1"), s("Q2")],
            vec![
                s("North"),
                CellValue::Int(1),
                CellValue::Int(2),
                CellValue::Int(3),
                CellValue::Int(4),
            ],
        ],
    );
    use_header_rows(&mut t, 0, 1, true).unwrap();
    assert_eq!(
        names(&t),
        ["Region", "2025 Q1", "2025 Q2", "2026 Q1", "2026 Q2"]
    );
    assert_eq!(t.row_count(), 1);
}

#[test]
fn whole_float_names_have_no_decimal_part() {
    let mut t = table(
        &["a", "b"],
        vec![
            vec![CellValue::Float(2025.0), CellValue::Float(1.5)],
            vec![CellValue::Int(1), CellValue::Int(2)],
        ],
    );
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(names(&t), ["2025", "1.5"]);
}

#[test]
fn blank_and_repeated_names() {
    let mut t = table(
        &["a", "b", "c"],
        vec![
            vec![s("Q1"), CellValue::Null, s("Q1")],
            vec![s("x"), s("y"), s("z")],
        ],
    );
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(names(&t), ["Q1", "column_2", "Q1_2"]);
}

#[test]
fn a_pending_edit_in_the_header_row_is_the_name() {
    let mut t = titled();
    t.set(0, 1, s("Total"));
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(names(&t)[1], "Total");
}

#[test]
fn one_undo_restores_everything() {
    let mut t = titled();
    let before = (names(&t), types(&t), values(&t));
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert!(t.undo());
    assert_eq!((names(&t), types(&t), values(&t)), before);
    assert!(!t.undo(), "the whole action is one undo step");
}

#[test]
fn redo_applies_it_again() {
    let mut t = titled();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    let after = (names(&t), types(&t), values(&t));
    t.undo();
    t.redo();
    assert_eq!((names(&t), types(&t), values(&t)), after);
}

#[test]
fn rows_past_the_end_are_refused() {
    let mut t = titled();
    let err = use_header_rows(&mut t, 0, 9, true).unwrap_err();
    assert!(format!("{err:#}").contains("rows 1-10"), "{err:#}");
    assert_eq!(t.row_count(), 3, "nothing changed");
}

#[test]
fn a_typed_column_keeps_its_precise_type() {
    let mut t = table(
        &["a"],
        vec![vec![CellValue::Int(5)], vec![CellValue::Int(6)]],
    );
    t.columns[0].data_type = "Int32".into();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(types(&t), ["Int32"]);
}

#[test]
fn header_is_data_puts_the_names_back_as_the_first_row() {
    let mut t = table(
        &["1001", "Anna", "2024-01-03"],
        vec![vec![
            CellValue::Int(1002),
            s("Ben"),
            CellValue::Date("2024-01-04".into()),
        ]],
    );
    t.columns[0].data_type = "Int64".into();
    t.columns[2].data_type = "Date32".into();
    header_is_data(&mut t, true).unwrap();
    assert_eq!(names(&t), ["column_1", "column_2", "column_3"]);
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(1001)));
    assert_eq!(t.get(0, 1), Some(&s("Anna")));
    assert_eq!(types(&t), ["Int64", "Utf8", "Date32"]);
}

#[test]
fn a_typed_column_holding_a_name_becomes_text() {
    let mut t = table(&["id"], vec![vec![CellValue::Int(1)]]);
    t.columns[0].data_type = "Int32".into();
    header_is_data(&mut t, true).unwrap();
    assert_eq!(types(&t), ["Utf8"]);
}

#[test]
fn header_is_data_then_row_one_round_trips() {
    let mut t = titled();
    let original = names(&t);
    header_is_data(&mut t, true).unwrap();
    use_header_rows(&mut t, 0, 0, true).unwrap();
    assert_eq!(names(&t), original);
}

#[test]
fn has_real_header_reads_the_names_and_follows_undo() {
    let mut t = titled();
    assert!(has_real_header(&t.columns));
    header_is_data(&mut t, true).unwrap();
    assert!(!has_real_header(&t.columns));
    t.undo();
    assert!(has_real_header(&t.columns));
}

#[test]
fn suggests_the_row_below_a_csv_title() {
    assert_eq!(
        suggest(&titled()),
        Some(HeaderSuggestion { first: 0, last: 0 })
    );
}

#[test]
fn suggests_below_an_excel_title_and_a_blank_row() {
    let t = table(
        &["Sales 2025", "Column2", "Column3"],
        vec![
            vec![CellValue::Null, CellValue::Null, CellValue::Null],
            vec![s("Region"), s("Q1"), s("Q2")],
            vec![s("North"), CellValue::Float(1.0), CellValue::Float(2.0)],
        ],
    );
    assert_eq!(suggest(&t), Some(HeaderSuggestion { first: 1, last: 1 }));
}

#[test]
fn suggests_a_merged_group_row_too() {
    let t = table(
        &["Title", "column_2", "column_3", "column_4", "column_5"],
        vec![
            vec![
                CellValue::Null,
                s("2025"),
                CellValue::Null,
                s("2026"),
                CellValue::Null,
            ],
            vec![s("Region"), s("Q1"), s("Q2"), s("Q1"), s("Q2")],
            vec![
                s("North"),
                CellValue::Int(1),
                CellValue::Int(2),
                CellValue::Int(3),
                CellValue::Int(4),
            ],
        ],
    );
    assert_eq!(suggest(&t), Some(HeaderSuggestion { first: 0, last: 1 }));
}

#[test]
fn a_normal_file_gets_no_suggestion() {
    let t = table(&["id", "name"], vec![vec![CellValue::Int(1), s("Anna")]]);
    assert_eq!(suggest(&t), None);
}

#[test]
fn a_one_column_file_gets_no_suggestion() {
    let t = table(&["column_1"], vec![vec![s("Name")]]);
    assert_eq!(suggest(&t), None);
}

#[test]
fn header_block_uses_a_consecutive_selection_around_the_click() {
    let sel: std::collections::HashSet<usize> = [3, 4].into_iter().collect();
    assert_eq!(header_block(&sel, 4), (3, 4));
    assert_eq!(header_block(&sel, 7), (7, 7), "click outside the selection");
    let gaps: std::collections::HashSet<usize> = [1, 4].into_iter().collect();
    assert_eq!(header_block(&gaps, 4), (4, 4), "not one block");
}
