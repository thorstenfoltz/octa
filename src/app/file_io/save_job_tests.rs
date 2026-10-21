use super::*;
use crate::app::state::SaveMode;
use octa::data::CellValue;
use octa::data::num_format::RoundingMode;
use octa::formats::FormatReader;
use octa::formats::write_options::{CsvOptions, ParquetOptions};

fn table(rows: usize) -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "amount".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: (0..rows)
            .map(|i| vec![CellValue::Int(i as i64), CellValue::Float(i as f64 + 0.25)])
            .collect(),
        ..DataTable::empty()
    }
}

fn csv() -> SliceFormat {
    SliceFormat::Delimited {
        opts: CsvOptions::default(),
        delimiter: b',',
    }
}

fn written(path: &std::path::Path) -> WrittenSave {
    WrittenSave {
        path: path.to_path_buf(),
        mode: SaveMode::Rebase,
        delimited: None,
        filtered_copy: false,
        written_rows: 0,
    }
}

/// One step per frame, as `drive_save_jobs` does, until the save ends.
fn run(job: &mut SaveJob, table: &DataTable) -> Outcome {
    for _ in 0..10_000 {
        if let Some(outcome) = job.step(table) {
            return outcome;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("the save never finished");
}

fn assert_done(outcome: Outcome) {
    match outcome {
        Outcome::Done {
            table_changed: false,
            back: None,
        } => {}
        Outcome::Done { .. } => panic!("finished, but not cleanly"),
        Outcome::Failed(e) => panic!("failed: {e}"),
        Outcome::Changed => panic!("stopped as changed"),
    }
}

/// A save fed seven rows at a time writes the same file a save of the whole
/// table writes, pending edits included.
#[test]
fn a_streamed_save_writes_what_a_whole_save_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = table(1000);
    t.set(3, 1, CellValue::Float(99.5));

    let whole = dir.path().join("whole.csv");
    octa::formats::csv_reader::write_delimited_with(&whole, &t, &CsvOptions::default(), b',')
        .unwrap();
    let sliced = dir.path().join("sliced.csv");
    let mut job = SaveJob::start(&t, None, csv(), None, Steps::default(), written(&sliced), 7);
    assert_done(run(&mut job, &t));
    assert_eq!(
        std::fs::read(&whole).unwrap(),
        std::fs::read(&sliced).unwrap()
    );

    let parquet = dir.path().join("sliced.parquet");
    let format = SliceFormat::Parquet(ParquetOptions::default());
    let mut job = SaveJob::start(
        &t,
        None,
        format,
        None,
        Steps::default(),
        written(&parquet),
        7,
    );
    assert_done(run(&mut job, &t));
    let back = octa::formats::parquet_reader::ParquetReader
        .read_file(&parquet)
        .unwrap();
    assert_eq!(back.row_count(), 1000);
    assert_eq!(back.rows[3][1], CellValue::Float(99.5));
    assert_eq!(back.rows[999][0], CellValue::Int(999));
}

/// A copy of the rows on screen writes those rows, in that order, and the
/// rounding a Save asks for is applied slice by slice.
#[test]
fn chosen_rows_are_written_in_order_and_rounded() {
    let dir = tempfile::tempdir().unwrap();
    let t = table(20);
    let path = dir.path().join("copy.csv");
    let round = HashMap::from([(
        1,
        NumberFormat {
            decimals: Some(0),
            rounding: RoundingMode::Down,
        },
    )]);
    let rows = Some(vec![9, 2, 15]);
    let mut job = SaveJob::start(
        &t,
        rows,
        csv(),
        Some(round),
        Steps::default(),
        written(&path),
        2,
    );
    assert_done(run(&mut job, &t));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "id,amount\n9,9.0\n2,2.0\n15,15.0\n"
    );
}

/// The table changing before every row was handed over stops the save, and
/// the old file stays, with no temp left beside it.
#[test]
fn a_table_that_changes_mid_save_leaves_the_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv");
    std::fs::write(&path, "old\n").unwrap();
    let mut t = table(1000);
    let mut job = SaveJob::start(&t, None, csv(), None, Steps::default(), written(&path), 7);
    assert!(job.step(&t).is_none());

    t.rows.truncate(10);
    assert!(matches!(job.step(&t), Some(Outcome::Changed)));
    drop(job);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::fs::read_dir(dir.path()).unwrap().count() > 1 {
        assert!(std::time::Instant::now() < deadline, "the temp file stayed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
}

/// A format with no slice writer is put back together on the worker and
/// written by its own writer: the same rows, pending edits included, and
/// for a save of the table itself its formulas, except where a cell was
/// typed over.
#[test]
fn a_whole_table_format_is_put_back_together_on_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = table(1000);
    t.set(3, 1, CellValue::Float(99.5));
    let path = dir.path().join("out.json");
    let writer = JobWriter::Whole {
        reader: "JSON".to_string(),
        opts: Box::default(),
        carry: Carry::of(&t),
    };
    let mut job = SaveJob::start(&t, None, writer, None, Steps::default(), written(&path), 7);
    assert_done(run(&mut job, &t));
    let back = octa::formats::json_reader::JsonReader
        .read_file(&path)
        .unwrap();
    assert_eq!(back.row_count(), 1000);
    assert_eq!(back.get(3, 1).unwrap().to_string(), "99.5");
    assert_eq!(back.get(999, 0).unwrap().to_string(), "999");

    t.formulas.insert((0, 1), "=A1*2".to_string());
    t.formulas.insert((3, 1), "=A4*2".to_string());
    let carry = Carry::of(&t);
    assert!(carry.formulas.contains_key(&(0, 1)));
    assert!(
        !carry.formulas.contains_key(&(3, 1)),
        "the typed value wins"
    );
}

/// The copies a save takes first run on the worker; one that fails stops the
/// save with the file as it was.
#[test]
fn copies_before_the_write_run_on_the_worker_and_a_failed_one_stops_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv");
    std::fs::write(&path, "old\n").unwrap();
    let t = table(100);

    let steps = Steps {
        before: vec![PreStep::Backup(path.clone())],
        back: None,
    };
    let mut job = SaveJob::start(&t, None, csv(), None, steps, written(&path), 7);
    assert_done(run(&mut job, &t));
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read_to_string(backups[0].path()).unwrap(), "old\n");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .starts_with("id,amount")
    );

    // A versions root that is a file cannot hold a version.
    std::fs::write(&path, "old\n").unwrap();
    let root = dir.path().join("not-a-folder");
    std::fs::write(&root, "").unwrap();
    let steps = Steps {
        before: vec![PreStep::KeepVersion {
            store: octa::file_versions::Store::Shared(root),
            target: path.clone(),
            limits: Default::default(),
        }],
        back: None,
    };
    let mut job = SaveJob::start(&t, None, csv(), None, steps, written(&path), 7);
    match run(&mut job, &t) {
        Outcome::Failed(e) => assert!(e.contains("Keeping the earlier version failed"), "{e}"),
        _ => panic!("the save went ahead without its version"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
}
