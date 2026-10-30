use super::*;

fn cols(names: &[(&str, &str)]) -> Vec<ColumnInfo> {
    names
        .iter()
        .map(|(n, t)| ColumnInfo {
            name: n.to_string(),
            data_type: t.to_string(),
        })
        .collect()
}

fn tbl(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols(columns);
    t.rows = rows;
    t
}

#[test]
fn folding_merges_columns_differing_only_in_case() {
    let a = cols(&[("Amount", "Int64")]);
    let b = cols(&[("amount", "Int64")]);

    let strict = plan_union(&[&a, &b], false);
    assert_eq!(
        strict.columns.len(),
        2,
        "without folding they are two columns"
    );

    let folded = plan_union(&[&a, &b], true);
    assert_eq!(folded.columns.len(), 1, "with folding they are one");
    assert_eq!(
        folded.columns[0].name, "Amount",
        "first spelling encountered wins"
    );
}

#[test]
fn folding_still_widens_types() {
    let a = cols(&[("Amount", "Int64")]);
    let b = cols(&[("amount", "Float64")]);
    let folded = plan_union(&[&a, &b], true);
    assert_eq!(folded.columns.len(), 1);
    assert_eq!(folded.columns[0].target_type, "Float64");
}

/// A single file holding both spellings still merges them, rather than
/// producing a plan with a duplicated output column.
#[test]
fn folding_handles_both_spellings_in_one_source() {
    let a = cols(&[("Amount", "Int64"), ("amount", "Float64")]);
    let folded = plan_union(&[&a], true);
    assert_eq!(folded.columns.len(), 1);
    assert_eq!(folded.columns[0].target_type, "Float64");
}

/// The flag travels on the plan, so `union_tables` matches names the same
/// way `plan_union` grouped them. Without that, a folded plan would find no
/// source column for `Amount` in the file spelling it `amount`, and the
/// whole column would come back null.
#[test]
fn folded_plan_still_finds_the_source_column() {
    let a = tbl(&[("Amount", "Int64")], vec![vec![CellValue::Int(1)]]);
    let b = tbl(&[("amount", "Int64")], vec![vec![CellValue::Int(2)]]);
    let plan = plan_union(&[&a.columns, &b.columns], true);
    let out = union_tables(&[&a, &b], &plan).expect("union");

    assert_eq!(out.columns.len(), 1);
    assert_eq!(out.row_count(), 2);
    assert_eq!(out.get(0, 0).map(|c| c.to_string()), Some("1".to_string()));
    assert_eq!(
        out.get(1, 0).map(|c| c.to_string()),
        Some("2".to_string()),
        "the second file spells the column differently, but it is the same column"
    );
}

#[test]
fn plan_widens_and_unions_columns() {
    let a = cols(&[("id", "Int64"), ("region", "Utf8"), ("amt", "Int64")]);
    let b = cols(&[
        ("id", "Int64"),
        ("region", "Utf8"),
        ("amt", "Float64"),
        ("note", "Utf8"),
    ]);
    let plan = plan_union(&[&a, &b], false);
    let names: Vec<_> = plan
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.target_type.as_str(), c.include))
        .collect();
    assert_eq!(
        names,
        vec![
            ("id", "Int64", true),
            ("region", "Utf8", true),
            ("amt", "Float64", true),
            ("note", "Utf8", true),
        ]
    );
}

#[test]
fn plan_disagreement_falls_back_to_utf8() {
    let a = cols(&[("v", "Int64")]);
    let b = cols(&[("v", "Date")]);
    let plan = plan_union(&[&a, &b], false);
    assert_eq!(plan.columns[0].target_type, "Utf8");
}

#[test]
fn union_fills_missing_with_null_and_widens() {
    let a = tbl(
        &[("id", "Int64"), ("amt", "Int64")],
        vec![vec![CellValue::Int(1), CellValue::Int(10)]],
    );
    let b = tbl(
        &[("id", "Int64"), ("amt", "Float64"), ("note", "Utf8")],
        vec![vec![
            CellValue::Int(2),
            CellValue::Float(2.5),
            CellValue::String("hi".into()),
        ]],
    );
    let plan = plan_union(&[&a.columns, &b.columns], false);
    let out = union_tables(&[&a, &b], &plan).unwrap();
    assert_eq!(
        out.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "amt", "note"]
    );
    assert_eq!(out.row_count(), 2);
    assert_eq!(out.get(0, 1), Some(&CellValue::Float(10.0)));
    assert_eq!(out.get(0, 2), Some(&CellValue::Null));
    assert_eq!(out.get(1, 2), Some(&CellValue::String("hi".into())));
}

#[test]
fn union_respects_dropped_columns() {
    let a = tbl(
        &[("id", "Int64"), ("drop_me", "Utf8")],
        vec![vec![CellValue::Int(1), CellValue::String("x".into())]],
    );
    let mut plan = plan_union(&[&a.columns], false);
    plan.columns
        .iter_mut()
        .find(|c| c.name == "drop_me")
        .unwrap()
        .include = false;
    let out = union_tables(&[&a], &plan).unwrap();
    assert_eq!(
        out.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
}

/// Two units of one time type merge into the finer one, and a `duration`
/// cast reads text cells as durations.
#[test]
fn time_types_merge_and_cast_by_word() {
    let a = tbl(&[("t", "Time64(µs)")], vec![vec![CellValue::Time(1_000)]]);
    let b = tbl(&[("t", "Time64(ns)")], vec![vec![CellValue::Time(1)]]);
    let plan = plan_union(&[&a.columns, &b.columns], false);
    assert_eq!(plan.columns[0].target_type, "Time64(ns)");

    let c = tbl(
        &[("d", "Utf8")],
        vec![vec![CellValue::String("26:00:00".into())]],
    );
    let mut plan = plan_union(&[&c.columns], false);
    plan.columns[0].target_type = "duration".into();
    let out = union_tables(&[&c], &plan).unwrap();
    assert_eq!(out.columns[0].data_type, "Duration(Microsecond)");
    assert_eq!(out.rows[0][0], CellValue::Duration(26 * 3_600_000_000_000));
}
