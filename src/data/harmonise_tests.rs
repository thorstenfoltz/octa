//! Unit tests for [`harmonise`](harmonise). Split out and included via
//! `#[path]`, matching `schema_drift` and `join_diag`.

use super::*;

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_string(),
        data_type: ty.to_string(),
    }
}

fn opts() -> HarmoniseOptions {
    HarmoniseOptions {
        root: std::path::PathBuf::new(),
        out_dir: std::path::PathBuf::from("/tmp/octa_harmonise_test_out"),
        ignore_case: false,
        overwrite: true,
    }
}

#[test]
fn a_matching_file_is_left_alone() {
    let target = vec![col("id", "Int64"), col("name", "Utf8")];
    let files = vec![("a.csv".to_string(), target.clone())];
    let plan = plan_harmonise(&files, &target, &opts());
    assert_eq!(plan.actions[0].status, FileStatus::AlreadyMatches);
    assert!(plan.actions[0].ops.is_empty());
    assert_eq!(plan.to_change(), 0);
}

#[test]
fn a_missing_column_is_added_as_null() {
    let target = vec![col("id", "Int64"), col("name", "Utf8")];
    let files = vec![("b.csv".to_string(), vec![col("id", "Int64")])];
    let plan = plan_harmonise(&files, &target, &opts());
    assert_eq!(plan.actions[0].status, FileStatus::Harmonise);
    assert!(
        plan.actions[0]
            .ops
            .iter()
            .any(|o| matches!(o, ColumnOp::AddNull { name, .. } if name == "name")),
        "got {:?}",
        plan.actions[0].ops
    );
}

#[test]
fn an_extra_column_is_dropped_and_named() {
    let target = vec![col("id", "Int64")];
    let files = vec![(
        "c.csv".to_string(),
        vec![col("id", "Int64"), col("legacy", "Utf8")],
    )];
    let plan = plan_harmonise(&files, &target, &opts());
    assert_eq!(
        plan.actions[0].dropped,
        vec!["legacy".to_string()],
        "dropping is correct, but it must be reported"
    );
}

#[test]
fn a_differing_type_becomes_a_cast() {
    let target = vec![col("id", "Int64")];
    let files = vec![("d.csv".to_string(), vec![col("id", "Utf8")])];
    let plan = plan_harmonise(&files, &target, &opts());
    assert!(
        plan.actions[0].ops.iter().any(
            |o| matches!(o, ColumnOp::Cast { name, to_type } if name == "id" && to_type == "Int64")
        ),
        "got {:?}",
        plan.actions[0].ops
    );
}

#[test]
fn case_folding_renames_rather_than_dropping_and_adding() {
    let target = vec![col("Amount", "Int64")];
    let files = vec![("e.csv".to_string(), vec![col("amount", "Int64")])];
    let mut o = opts();
    o.ignore_case = true;
    let plan = plan_harmonise(&files, &target, &o);
    assert!(
        plan.actions[0].ops.iter().any(
            |op| matches!(op, ColumnOp::Rename { from, to } if from == "amount" && to == "Amount")
        ),
        "got {:?}",
        plan.actions[0].ops
    );
    assert!(
        plan.actions[0].dropped.is_empty(),
        "a case-only difference must not lose the column"
    );
}

#[test]
fn without_case_folding_a_case_difference_is_a_different_column() {
    let target = vec![col("Amount", "Int64")];
    let files = vec![("e.csv".to_string(), vec![col("amount", "Int64")])];
    let plan = plan_harmonise(&files, &target, &opts());
    assert_eq!(plan.actions[0].dropped, vec!["amount".to_string()]);
}

#[test]
fn two_inputs_that_would_write_the_same_output_are_refused() {
    let target = vec![col("id", "Int64")];
    let files = vec![
        ("one/x.csv".to_string(), target.clone()),
        ("two/x.csv".to_string(), target.clone()),
    ];
    let plan = plan_harmonise(&files, &target, &opts());
    let refused: Vec<&FileAction> = plan
        .actions
        .iter()
        .filter(|a| matches!(a.status, FileStatus::Refused(_)))
        .collect();
    assert_eq!(
        refused.len(),
        2,
        "both sides of a collision are refused, not silently renamed: {:?}",
        plan.actions
    );
}

#[test]
fn an_empty_target_is_refused_rather_than_emptying_every_file() {
    let files = vec![("a.csv".to_string(), vec![col("id", "Int64")])];
    let plan = plan_harmonise(&files, &[], &opts());
    assert!(matches!(plan.actions[0].status, FileStatus::Refused(_)));
}

#[test]
fn output_keeps_the_input_extension_and_lands_in_out_dir() {
    let target = vec![col("id", "Int64")];
    let files = vec![("/data/parts/part-01.parquet".to_string(), target.clone())];
    let plan = plan_harmonise(&files, &target, &opts());
    let out = &plan.actions[0].output;
    assert_eq!(out.extension().unwrap(), "parquet");
    assert_eq!(out.file_name().unwrap(), "part-01.parquet");
    assert!(out.starts_with("/tmp/octa_harmonise_test_out"));
}

// --- Runner ---------------------------------------------------------------
//
// These touch a real filesystem, because the safety property being tested is
// "a refused file leaves nothing behind on disk".

use std::sync::atomic::AtomicBool;

fn run(
    dir: &std::path::Path,
    out: &std::path::Path,
    target: &[ColumnInfo],
    ignore_case: bool,
) -> HarmoniseReport {
    let registry = crate::formats::FormatRegistry::new();
    let (files, _skipped) = crate::data::schema_drift::collect_schemas(dir, false, &registry);
    let opts = HarmoniseOptions {
        root: dir.to_path_buf(),
        out_dir: out.to_path_buf(),
        ignore_case,
        overwrite: true,
    };
    let plan = plan_harmonise(&files, target, &opts);
    run_harmonise(
        &plan,
        &opts,
        &|_, _| {},
        &AtomicBool::new(false),
        &Default::default(),
    )
}

#[test]
fn harmonises_drifting_files_into_one_shape() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), "id,name\n1,alice\n").unwrap();
    std::fs::write(dir.path().join("b.csv"), "id,name\n2,bob\n").unwrap();
    // Missing the `name` column entirely.
    std::fs::write(dir.path().join("c.csv"), "id\n3\n").unwrap();

    let target = vec![col("id", "Int64"), col("name", "Utf8")];
    let report = run(dir.path(), out.path(), &target, false);

    assert_eq!(report.refused, 0, "{report:?}");
    assert_eq!(report.written, 3);

    let fixed = crate::formats::read_table_auto(&out.path().join("c.csv"), None, u64::MAX).unwrap();
    assert_eq!(fixed.columns.len(), 2, "the missing column was added");
    assert_eq!(fixed.columns[1].name, "name");
    assert_eq!(fixed.row_count(), 1);
}

#[test]
fn a_lossy_cast_refuses_the_file_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.csv"), "id\nnot-a-number\n").unwrap();

    let target = vec![col("id", "Int64")];
    let report = run(dir.path(), out.path(), &target, false);

    assert_eq!(report.written, 0, "{report:?}");
    assert_eq!(report.refused, 1);
    assert!(
        !out.path().join("bad.csv").exists(),
        "a refused file must leave nothing behind"
    );
    let FileStatus::Refused(why) = &report.items[0].status else {
        panic!("expected Refused, got {:?}", report.items[0].status);
    };
    assert!(why.contains("losing values"), "unhelpful reason: {why}");
}

#[test]
fn originals_are_never_touched() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.csv");
    std::fs::write(&src, "id,extra\n1,keep-me\n").unwrap();
    let before = std::fs::read_to_string(&src).unwrap();

    let target = vec![col("id", "Int64")];
    let report = run(dir.path(), out.path(), &target, false);

    assert_eq!(report.written, 1);
    assert_eq!(
        std::fs::read_to_string(&src).unwrap(),
        before,
        "the input file must be byte-identical afterwards"
    );
    assert_eq!(report.items[0].dropped, vec!["extra".to_string()]);
}
