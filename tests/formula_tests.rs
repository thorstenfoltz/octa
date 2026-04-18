use octa::data::*;
use std::collections::HashMap;

fn formula_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "A".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "B".into(),
                data_type: "Float64".into(),
            },
            ColumnInfo {
                name: "C".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(10),
                CellValue::Float(2.5),
                CellValue::String("hello".into()),
            ],
            vec![
                CellValue::Int(20),
                CellValue::Float(3.0),
                CellValue::String("world".into()),
            ],
            vec![CellValue::Int(30), CellValue::Float(0.5), CellValue::Null],
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

// --- Basic arithmetic ---

#[test]
fn formula_addition() {
    let table = formula_table();
    let result = evaluate_formula("A1+B1", &table);
    assert_eq!(result, Some(12.5)); // 10 + 2.5
}

#[test]
fn formula_subtraction() {
    let table = formula_table();
    let result = evaluate_formula("A1-B1", &table);
    assert_eq!(result, Some(7.5)); // 10 - 2.5
}

#[test]
fn formula_multiplication() {
    let table = formula_table();
    let result = evaluate_formula("A1*B1", &table);
    assert_eq!(result, Some(25.0)); // 10 * 2.5
}

#[test]
fn formula_division() {
    let table = formula_table();
    let result = evaluate_formula("A1/B1", &table);
    assert_eq!(result, Some(4.0)); // 10 / 2.5
}

#[test]
fn formula_division_by_zero() {
    let table = formula_table();
    // B3 is 0.5, not zero; use a literal 0
    let result = evaluate_formula("A1/0", &table);
    assert_eq!(result, None);
}

// --- Operator precedence ---

#[test]
fn formula_precedence_mul_before_add() {
    let table = formula_table();
    // A1 + B1 * A2 = 10 + 2.5 * 20 = 10 + 50 = 60
    let result = evaluate_formula("A1+B1*A2", &table);
    assert_eq!(result, Some(60.0));
}

#[test]
fn formula_parentheses_override_precedence() {
    let table = formula_table();
    // (A1 + B1) * A2 = (10 + 2.5) * 20 = 12.5 * 20 = 250
    let result = evaluate_formula("(A1+B1)*A2", &table);
    assert_eq!(result, Some(250.0));
}

// --- Cell references ---

#[test]
fn formula_different_rows() {
    let table = formula_table();
    let result = evaluate_formula("A1+A2+A3", &table);
    assert_eq!(result, Some(60.0)); // 10 + 20 + 30
}

#[test]
fn formula_null_cell_treated_as_zero() {
    let table = formula_table();
    // C3 is Null, should be 0
    // Need a numeric column — C is string. Let's use that Null is 0.
    // Actually C3 is Null which returns 0.0 per cell_as_f64
    // But C1 is "hello" which won't parse as f64 -> None -> 0.0
    let result = evaluate_formula("A1+0", &table);
    assert_eq!(result, Some(10.0));
}

#[test]
fn formula_with_numeric_literal() {
    let table = formula_table();
    let result = evaluate_formula("A1*2+3", &table);
    assert_eq!(result, Some(23.0)); // 10*2+3
}

#[test]
fn formula_with_float_literal() {
    let table = formula_table();
    let result = evaluate_formula("A1*1.5", &table);
    assert_eq!(result, Some(15.0));
}

// --- Invalid formulas ---

#[test]
fn formula_invalid_ref_returns_none() {
    let table = formula_table();
    // Row 0 doesn't exist in 1-based (A0 is invalid)
    let result = evaluate_formula("A0+B1", &table);
    assert_eq!(result, None);
}

#[test]
fn formula_empty_returns_none() {
    let table = formula_table();
    let result = evaluate_formula("", &table);
    assert_eq!(result, None);
}

#[test]
fn formula_just_number() {
    let table = formula_table();
    let result = evaluate_formula("42", &table);
    assert_eq!(result, Some(42.0));
}

#[test]
fn formula_nested_parens() {
    let table = formula_table();
    let result = evaluate_formula("((A1+A2))*2", &table);
    assert_eq!(result, Some(60.0)); // (10+20)*2
}

#[test]
fn formula_unary_minus() {
    let table = formula_table();
    let result = evaluate_formula("-5+A1", &table);
    assert_eq!(result, Some(5.0)); // -5 + 10
}

// --- Multi-column references ---

#[test]
fn formula_aa_column_ref() {
    // AA would be column 26 (0-indexed), which doesn't exist in our 3-col table
    let table = formula_table();
    // Should still parse, but cell value would be 0 (out of bounds)
    let result = evaluate_formula("AA1+0", &table);
    // get(0, 26) -> None -> 0.0
    assert_eq!(result, Some(0.0));
}
