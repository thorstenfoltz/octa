use std::path::{Path, PathBuf};

use super::*;

fn inputs(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn plans_one_output_per_input() {
    let plan = plan_batch(
        &inputs(&["/in/a.csv", "/in/b.csv"]),
        Path::new("/out"),
        "parquet",
        false,
    );
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].output, PathBuf::from("/out/a.parquet"));
    assert_eq!(plan[1].output, PathBuf::from("/out/b.parquet"));
    assert!(plan.iter().all(|i| i.status == BatchStatus::Pending));
}

#[test]
fn de_collides_two_inputs_with_the_same_stem() {
    // /in/a.csv and /other/a.json both want /out/a.parquet.
    let plan = plan_batch(
        &inputs(&["/in/a.csv", "/other/a.json"]),
        Path::new("/out"),
        "parquet",
        false,
    );
    assert_eq!(plan[0].output, PathBuf::from("/out/a.parquet"));
    assert_eq!(
        plan[1].output,
        PathBuf::from("/out/a_2.parquet"),
        "the second must not silently overwrite the first"
    );
}

#[test]
fn skips_existing_files_unless_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("a.csv"), "x").unwrap();

    let plan = plan_batch(&inputs(&["/in/a.json"]), &out, "csv", false);
    assert!(
        matches!(
            plan[0].status,
            BatchStatus::Skipped(SkipReason::ExistingFile)
        ),
        "{:?}",
        plan[0].status
    );

    let plan = plan_batch(&inputs(&["/in/a.json"]), &out, "csv", true);
    assert_eq!(
        plan[0].status,
        BatchStatus::Pending,
        "overwrite was asked for"
    );
}

#[test]
fn rejects_a_read_only_target_format() {
    // SAS is read-only in the registry, so a whole batch aimed at it is
    // pointless work; every item is marked before anything runs.
    let plan = plan_batch(
        &inputs(&["/in/a.csv"]),
        Path::new("/out"),
        "sas7bdat",
        false,
    );
    assert!(
        matches!(
            plan[0].status,
            BatchStatus::Skipped(SkipReason::ReadOnlyTarget)
        ),
        "{:?}",
        plan[0].status
    );
}

#[test]
fn rejects_an_unknown_target_extension() {
    let plan = plan_batch(&inputs(&["/in/a.csv"]), Path::new("/out"), "zzz", false);
    assert!(
        matches!(
            plan[0].status,
            BatchStatus::Skipped(SkipReason::ReadOnlyTarget)
        ),
        "an extension with no writer is not a usable target: {:?}",
        plan[0].status
    );
}

#[test]
fn an_empty_input_list_plans_nothing() {
    assert!(plan_batch(&[], Path::new("/out"), "csv", false).is_empty());
}

#[test]
fn runs_a_real_conversion_and_reports_each_item() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), "id,city\n1,Tokyo\n2,Bonn\n").unwrap();
    std::fs::write(dir.path().join("b.csv"), "id,city\n3,Oslo\n").unwrap();
    let out = dir.path().join("out");

    let plan = plan_batch(
        &[dir.path().join("a.csv"), dir.path().join("b.csv")],
        &out,
        "json",
        false,
    );
    let report = run_batch(
        plan,
        &|_, _| {},
        &std::sync::atomic::AtomicBool::new(false),
        &Default::default(),
    );

    assert_eq!(report.converted, 2, "{report:?}");
    assert_eq!(report.failed, 0, "{report:?}");
    assert!(out.join("a.json").exists());
    assert!(out.join("b.json").exists());
    assert_eq!(
        report.items[0].status,
        BatchStatus::Done { rows: 2 },
        "row count comes from the table actually written"
    );
}

#[test]
fn one_bad_input_does_not_abort_the_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("good.csv"), "id\n1\n").unwrap();
    let missing = dir.path().join("missing.csv");
    let out = dir.path().join("out");

    let plan = plan_batch(&[missing, dir.path().join("good.csv")], &out, "json", false);
    let report = run_batch(
        plan,
        &|_, _| {},
        &std::sync::atomic::AtomicBool::new(false),
        &Default::default(),
    );

    assert_eq!(report.failed, 1, "{report:?}");
    assert_eq!(
        report.converted, 1,
        "the good file still converted: {report:?}"
    );
}

#[test]
fn a_cancelled_run_stops_early() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), "id\n1\n").unwrap();
    let plan = plan_batch(
        &[dir.path().join("a.csv")],
        &dir.path().join("out"),
        "json",
        false,
    );
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let report = run_batch(plan, &|_, _| {}, &cancel, &Default::default());
    assert_eq!(
        report.converted, 0,
        "a pre-cancelled run must convert nothing"
    );
}

#[test]
fn the_report_renders_as_a_table() {
    let report = BatchReport {
        items: vec![BatchItem {
            input: PathBuf::from("/in/a.csv"),
            output: PathBuf::from("/out/a.json"),
            status: BatchStatus::Done { rows: 3 },
        }],
        converted: 1,
        failed: 0,
        skipped: 0,
    };
    let t = report_table(&report);
    assert_eq!(t.row_count(), 1);
    assert_eq!(
        t.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["input", "output", "status", "rows", "error"]
    );
}

#[test]
fn progress_fires_once_per_converted_item() {
    // The GUI drives its progress bar from this, so a skipped item must not
    // silently advance the count past the total.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), "id\n1\n").unwrap();
    std::fs::write(dir.path().join("b.csv"), "id\n2\n").unwrap();
    let plan = plan_batch(
        &[dir.path().join("a.csv"), dir.path().join("b.csv")],
        &dir.path().join("out"),
        "json",
        false,
    );

    let seen = std::sync::Mutex::new(Vec::new());
    let report = run_batch(
        plan,
        &|done, total| seen.lock().unwrap().push((done, total)),
        &std::sync::atomic::AtomicBool::new(false),
        &Default::default(),
    );

    assert_eq!(report.converted, 2);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "one callback per item: {seen:?}");
    assert!(
        seen.iter().all(|&(done, total)| done <= total),
        "progress must never exceed the total: {seen:?}"
    );
}
