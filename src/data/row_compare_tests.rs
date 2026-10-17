use super::*;

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = ["city", "amount"]
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = vec![
        vec![
            CellValue::String("Aachen".into()),
            CellValue::Float(12.5_f64),
        ],
        vec![
            CellValue::String("Aachen".into()),
            CellValue::Float(13.1_f64),
        ],
        vec![CellValue::String("Bonn".into()), CellValue::Float(12.5_f64)],
    ];
    t
}

#[test]
fn flags_only_the_fields_that_differ() {
    let out = compare_rows(&table(), &[0, 1]);
    assert_eq!(
        out.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["column", "differs", "row 1", "row 2"]
    );
    // city is the same in both rows, amount is not.
    assert_eq!(out.get(0, 1).unwrap().to_string(), "no");
    assert_eq!(out.get(1, 1).unwrap().to_string(), "yes");
}

#[test]
fn three_rows_are_three_columns() {
    let out = compare_rows(&table(), &[0, 1, 2]);
    assert_eq!(out.col_count(), 5);
    // A field two rows agree on still differs once the third disagrees.
    assert_eq!(out.get(0, 1).unwrap().to_string(), "yes");
    assert_eq!(out.get(0, 2).unwrap().to_string(), "Aachen");
    assert_eq!(out.get(0, 4).unwrap().to_string(), "Bonn");
}

#[test]
fn one_row_differs_from_nothing() {
    let out = compare_rows(&table(), &[2]);
    assert_eq!(out.col_count(), 3);
    assert!(out.rows.iter().all(|r| r[1].to_string() == "no"));
}

/// A selection of two or more rows is enough on its own, and it wins over
/// whatever was marked earlier.
#[test]
fn a_selection_beats_the_marks_but_one_selected_row_does_not() {
    use crate::data::{MarkColor, MarkKey};
    let mut t = table();
    t.set_mark(MarkKey::Row(0), MarkColor::Red);
    t.set_mark(MarkKey::Row(1), MarkColor::Red);
    assert_eq!(rows_to_compare(&t, &HashSet::new()), vec![0, 1]);
    assert_eq!(rows_to_compare(&t, &HashSet::from([2])), vec![0, 1]);
    assert_eq!(rows_to_compare(&t, &HashSet::from([2, 1])), vec![1, 2]);
}

#[test]
fn only_whole_row_marks_count_and_they_come_out_sorted() {
    use crate::data::{MarkColor, MarkKey};
    let mut t = table();
    t.set_mark(MarkKey::Row(2), MarkColor::Red);
    t.set_mark(MarkKey::Row(0), MarkColor::Red);
    t.set_mark(MarkKey::Cell(1, 0), MarkColor::Blue);
    t.set_mark(MarkKey::Column(1), MarkColor::Green);
    assert_eq!(marked_rows(&t), vec![0, 2]);
}

#[test]
fn row_labels_follow_the_page_offset() {
    let mut t = table();
    t.row_offset = 900_000;
    let out = compare_rows(&t, &[0, 2]);
    assert_eq!(out.columns[2].name, "row 900001");
    assert_eq!(out.columns[3].name, "row 900003");
}
