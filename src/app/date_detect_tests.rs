use super::*;

/// The worker converts a date column, keeps its nulls and hands back what
/// it held for undo; a text column comes back untouched.
#[test]
fn the_worker_converts_date_columns_and_keeps_the_old_values() {
    let s = |v: &str| CellValue::String(v.into());
    let dates = vec![s("15.04.2024"), CellValue::Null, s("01.12.2025")];
    let text = vec![s("Mortgage"), s("Debt collection"), CellValue::Null];
    let found = detect(
        vec![(0, "d".into()), (3, "p".into())],
        vec![dates.clone(), text],
    );
    assert_eq!(found.findings.promoted, [0]);
    assert_eq!(found.converted.len(), 1);
    let conv = &found.converted[0];
    assert_eq!(conv.col_idx, 0);
    assert_eq!(conv.data_type, date_infer::DATE_TYPE);
    assert_eq!(conv.old_values, dates);
    assert_eq!(
        conv.cells,
        [
            CellValue::Date("2024-04-15".into()),
            CellValue::Null,
            CellValue::Date("2025-12-01".into())
        ]
    );
    assert_eq!(conv.cells, conv.new_values);
}
