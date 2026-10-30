use super::*;

fn table(cols: &[&str], rows: &[&[&str]]) -> DataTable {
    let columns = cols
        .iter()
        .map(|c| ColumnInfo {
            name: c.to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    let rows = rows
        .iter()
        .map(|r| r.iter().map(|v| CellValue::String(v.to_string())).collect())
        .collect();
    DataTable {
        columns,
        rows,
        ..DataTable::empty()
    }
}

#[test]
fn transposes_shape_and_values() {
    // 2 columns x 3 rows -> 3 (+1 header) columns x 2 rows.
    let t = table(
        &["id", "name"],
        &[&["1", "ann"], &["2", "bob"], &["3", "cy"]],
    );
    let out = transpose_table(&t);
    // Columns: "column", "1", "2", "3".
    assert_eq!(
        out.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["column", "1", "2", "3"]
    );
    // Two output rows, one per original column.
    assert_eq!(out.rows.len(), 2);
    assert_eq!(out.rows[0][0], CellValue::String("id".into()));
    assert_eq!(out.rows[0][1], CellValue::String("1".into()));
    assert_eq!(out.rows[0][3], CellValue::String("3".into()));
    assert_eq!(out.rows[1][0], CellValue::String("name".into()));
    assert_eq!(out.rows[1][2], CellValue::String("bob".into()));
    // All output columns are text.
    assert!(out.columns.iter().all(|c| c.data_type == "Utf8"));
}

#[test]
fn empty_table_gives_no_rows() {
    let t = table(&[], &[]);
    let out = transpose_table(&t);
    assert_eq!(out.columns.len(), 1); // just the leading "column".
    assert!(out.rows.is_empty());
}

#[test]
fn ragged_missing_cells_become_empty() {
    // Second row is short; missing cell reads as empty text.
    let mut t = table(&["a", "b"], &[&["1", "2"]]);
    t.rows.push(vec![CellValue::String("3".into())]); // missing col b.
    let out = transpose_table(&t);
    // Row for column "b": header + r0="2" + r1="" (missing).
    assert_eq!(out.rows[1][0], CellValue::String("b".into()));
    assert_eq!(out.rows[1][2], CellValue::String(String::new()));
}
