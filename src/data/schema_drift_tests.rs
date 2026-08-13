use super::*;
use crate::data::ColumnInfo;

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_string(),
        data_type: ty.to_string(),
    }
}

fn files() -> Vec<(String, Vec<ColumnInfo>)> {
    vec![
        (
            "part-0.parquet".into(),
            vec![col("id", "Int64"), col("amount", "Float64")],
        ),
        (
            "part-1.parquet".into(),
            vec![col("id", "Int64"), col("amount", "Float64")],
        ),
    ]
}

fn row_for(t: &crate::data::DataTable, name: &str) -> usize {
    (0..t.row_count())
        .find(|r| t.get(*r, 1).map(|c| c.to_string()).as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no row for {name}"))
}

#[test]
fn identical_schemas_are_one_variant_and_no_drift() {
    let r = analyse(&files(), &DriftOptions::default());
    assert_eq!(r.variants.len(), 1);
    assert_eq!(r.variants[0].files.len(), 2);
    assert!(!r.has_drift);
    assert!(r.drifting_columns.is_empty());
}

#[test]
fn a_type_difference_creates_a_second_variant() {
    let mut f = files();
    f.push((
        "part-2.parquet".into(),
        vec![col("id", "Int64"), col("amount", "Utf8")],
    ));
    let r = analyse(&f, &DriftOptions::default());
    assert_eq!(r.variants.len(), 2);
    assert!(r.has_drift);
    assert_eq!(r.drifting_columns, vec!["amount".to_string()]);
}

/// The largest group first, so the odd file out is visibly the minority.
#[test]
fn variants_are_ordered_by_file_count() {
    let mut f = files();
    f.push(("odd.parquet".into(), vec![col("id", "Utf8")]));
    let r = analyse(&f, &DriftOptions::default());
    assert_eq!(r.variants[0].files.len(), 2);
    assert_eq!(r.variants[1].files.len(), 1);
}

#[test]
fn a_missing_column_is_reported_as_missing_not_as_a_type_difference() {
    let mut f = files();
    f.push(("short.parquet".into(), vec![col("id", "Int64")]));
    let r = analyse(&f, &DriftOptions::default());
    assert!(r.has_drift);
    assert_eq!(r.drifting_columns, vec!["amount".to_string()]);

    let t = report_table(&r);
    let row = row_for(&t, "amount");
    assert!(
        t.get(row, 0)
            .map(|c| c.to_string())
            .unwrap_or_default()
            .starts_with("missing in"),
        "expected a missing status, got {:?}",
        t.get(row, 0)
    );
}

#[test]
fn case_folding_is_off_by_default_and_merges_when_on() {
    let f = vec![
        ("a.csv".into(), vec![col("Amount", "Int64")]),
        ("b.csv".into(), vec![col("amount", "Int64")]),
    ];

    let strict = analyse(&f, &DriftOptions::default());
    assert!(strict.has_drift, "differing case must be drift by default");

    let folded = analyse(
        &f,
        &DriftOptions {
            ignore_case: true,
            ..Default::default()
        },
    );
    assert!(
        !folded.has_drift,
        "folded case must collapse to one variant"
    );
    assert_eq!(folded.variants.len(), 1);
}

/// First spelling encountered wins, so the report names a column the way the
/// first file spells it rather than inventing a canonical form.
#[test]
fn folding_keeps_the_first_spelling() {
    let f = vec![
        ("a.csv".into(), vec![col("Amount", "Int64")]),
        ("b.csv".into(), vec![col("amount", "Int64")]),
    ];
    let r = analyse(
        &f,
        &DriftOptions {
            ignore_case: true,
            ..Default::default()
        },
    );
    assert_eq!(r.variants[0].columns[0].name, "Amount");
}

/// A folded scan can still have two variants for some other reason, and then
/// the two spellings live in different variants. The report must group them as
/// the scan did, or it reports the very difference the caller asked to ignore.
#[test]
fn folding_survives_into_the_report_table() {
    let f = vec![
        (
            "a.csv".into(),
            vec![col("Amount", "Int64"), col("extra", "Int64")],
        ),
        ("b.csv".into(), vec![col("amount", "Int64")]),
    ];
    let r = analyse(
        &f,
        &DriftOptions {
            ignore_case: true,
            ..Default::default()
        },
    );
    assert_eq!(r.variants.len(), 2, "the extra column is a real difference");

    let t = report_table(&r);
    assert_eq!(
        t.row_count(),
        2,
        "Amount and amount are one column, plus extra"
    );
    assert_eq!(
        t.get(row_for(&t, "Amount"), 0).map(|c| c.to_string()),
        Some("consistent".to_string()),
        "the folded column has the same type in both variants"
    );
}

#[test]
fn report_table_has_one_column_per_variant_plus_status_and_name() {
    let mut f = files();
    f.push((
        "part-2.parquet".into(),
        vec![col("id", "Int64"), col("amount", "Utf8")],
    ));
    let r = analyse(&f, &DriftOptions::default());
    let t = report_table(&r);

    assert_eq!(t.columns[0].name, "status");
    assert_eq!(t.columns[1].name, "column");
    assert_eq!(t.columns.len(), 4, "status + column + 2 variants");
    assert!(
        t.columns[2].name.contains("2 files"),
        "got {:?}",
        t.columns[2].name
    );
    assert_eq!(t.row_count(), 2, "one row per distinct column name");
}

/// Problem rows first: a scan is read top down and the drift is the point.
#[test]
fn drifting_rows_sort_above_consistent_ones() {
    let mut f = files();
    f.push((
        "part-2.parquet".into(),
        vec![col("id", "Int64"), col("amount", "Utf8")],
    ));
    let r = analyse(&f, &DriftOptions::default());
    let t = report_table(&r);
    assert_eq!(
        t.get(0, 1).map(|c| c.to_string()),
        Some("amount".to_string())
    );
    assert_eq!(t.get(1, 1).map(|c| c.to_string()), Some("id".to_string()));
}

#[test]
fn no_files_is_empty_not_a_panic() {
    let r = analyse(&[], &DriftOptions::default());
    assert!(r.variants.is_empty());
    assert!(!r.has_drift);
    assert_eq!(report_table(&r).row_count(), 0);
}
