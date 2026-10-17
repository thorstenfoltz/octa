use super::*;
use crate::data::ColumnInfo;

fn numeric_table() -> DataTable {
    let mut t = DataTable::empty();
    for n in ["x", "y", "label"] {
        t.columns.push(ColumnInfo {
            name: n.to_string(),
            data_type: if n == "label" { "Utf8" } else { "Float64" }.to_string(),
        });
    }
    // y = 2x (perfect positive correlation); label is non-numeric.
    for i in 1..=5 {
        t.rows.push(vec![
            CellValue::Float(i as f64),
            CellValue::Float((i * 2) as f64),
            CellValue::String(format!("r{i}")),
        ]);
    }
    t
}

#[test]
fn perfect_positive_pearson() {
    let t = numeric_table();
    let m = correlation_matrix(&t, CorrMethod::Pearson);
    // Only x and y are numeric.
    assert_eq!(m.columns, vec!["x", "y"]);
    assert!((m.matrix[0][1].unwrap() - 1.0).abs() < 1e-9);
    assert!((m.matrix[0][0].unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn perfect_negative() {
    let mut t = DataTable::empty();
    for n in ["a", "b"] {
        t.columns.push(ColumnInfo {
            name: n.to_string(),
            data_type: "Int64".to_string(),
        });
    }
    for i in 0..4 {
        t.rows.push(vec![CellValue::Int(i), CellValue::Int(10 - i)]);
    }
    let m = correlation_matrix(&t, CorrMethod::Pearson);
    assert!((m.matrix[0][1].unwrap() + 1.0).abs() < 1e-9);
}

#[test]
fn spearman_monotonic() {
    // y = x^3 is monotonic but not linear: Spearman = 1, Pearson < 1.
    let mut t = DataTable::empty();
    for n in ["x", "y"] {
        t.columns.push(ColumnInfo {
            name: n.to_string(),
            data_type: "Float64".to_string(),
        });
    }
    for i in 1..=6 {
        let x = i as f64;
        t.rows
            .push(vec![CellValue::Float(x), CellValue::Float(x.powi(3))]);
    }
    let sp = correlation_matrix(&t, CorrMethod::Spearman);
    assert!((sp.matrix[0][1].unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn matrix_to_table_shapes_correctly() {
    let m = CorrMatrix {
        columns: vec!["x".into(), "y".into()],
        matrix: vec![vec![Some(1.0), Some(0.5)], vec![Some(0.5), Some(1.0)]],
    };
    let t = matrix_to_table(&m);
    assert_eq!(t.col_count(), 3); // variable + x + y
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.columns[0].name, "variable");
    assert_eq!(t.columns[1].name, "x");
}

#[test]
fn zero_variance_is_none() {
    let mut t = DataTable::empty();
    for n in ["a", "b"] {
        t.columns.push(ColumnInfo {
            name: n.to_string(),
            data_type: "Float64".to_string(),
        });
    }
    for _ in 0..3 {
        t.rows
            .push(vec![CellValue::Float(5.0), CellValue::Float(1.0)]);
    }
    let m = correlation_matrix(&t, CorrMethod::Pearson);
    assert_eq!(m.matrix[0][1], None);
}
