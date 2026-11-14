//! Lance against DuckDB's real extension. It is a download of about 100 MB on first
//! use, so these run only with `OCTA_TEST_LAKEHOUSE=1` and pass as a no-op otherwise,
//! like `lakehouse_live_tests.rs`. Run them after touching `lance_reader.rs`.

use std::path::Path;

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::lakehouse_reader::{self, LakehouseKind};
use octa::formats::lance_reader::LanceReader;
use octa::formats::{FormatReader, ThreadRowCap};

/// The `lance-live` CI job fails on the word `skipped:`, or a typo in its env var
/// would pass every test here without touching the extension.
fn enabled() -> bool {
    let on = std::env::var("OCTA_TEST_LAKEHOUSE").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: OCTA_TEST_LAKEHOUSE is not 1");
    }
    on
}

fn duck() -> duckdb::Connection {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("INSTALL lance; LOAD lance;").unwrap();
    conn
}

fn make(path: &Path, select: &str) {
    duck()
        .execute_batch(&format!(
            "COPY ({select}) TO '{}' (FORMAT lance, MODE 'overwrite')",
            path.display()
        ))
        .unwrap();
}

fn versions(path: &Path) -> usize {
    std::fs::read_dir(path.join("_versions"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "manifest")
        })
        .count()
}

/// `(name, DuckDB type)` as DuckDB itself reads the dataset.
fn described(path: &Path) -> Vec<(String, String)> {
    let conn = duck();
    let mut stmt = conn
        .prepare(&format!(
            "DESCRIBE SELECT * FROM __lance_scan('{}')",
            path.display()
        ))
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Every value as DuckDB prints it, one string per row, in `id` order: an
/// independent view of the dataset, through nothing of Octa's.
fn printed(path: &Path) -> Vec<String> {
    let conn = duck();
    let scan = format!("__lance_scan('{}')", path.display());
    let cols: Vec<String> = described(path)
        .into_iter()
        .map(|(name, _)| format!("coalesce(CAST(\"{name}\" AS VARCHAR), 'NULL')"))
        .collect();
    let sql = format!(
        "SELECT concat_ws(' | ', {}) FROM {scan} ORDER BY \"id\"",
        cols.join(", ")
    );
    let mut stmt = conn.prepare(&sql).unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

const EVERY_TYPE: &str = "SELECT i::BIGINT id, i::INTEGER a_int, i::SMALLINT a_small, \
    i::TINYINT a_tiny, i::UBIGINT a_ubig, i::UINTEGER a_uint, i * 1.5::DOUBLE a_dbl, \
    i * 0.5::FLOAT a_flt, i % 2 = 0 a_bool, 'héllo 列 ' || i a_txt, \
    DATE '2026-11-06' + i::INTEGER a_date, \
    TIME '12:34:56.789' a_time, '12:34:56.123456789'::TIME_NS a_time_ns, \
    TIMESTAMP '2026-11-06 12:34:56.123456' a_ts, \
    TIMESTAMP_S '2026-11-06 12:34:56' a_ts_s, TIMESTAMPTZ '2026-11-06 12:34:56+01' a_tstz, \
    12.34::DECIMAL(18,2) a_dec, \
    123456789012345678901234567890123456.78::DECIMAL(38,2) a_big_dec, \
    ('abc' || i)::BLOB a_blob, [0.25 * i, 0.5, 0.75]::FLOAT[3] vec, [i, 2] a_list, \
    {'x': i, 'y': 'with, comma'} a_struct, MAP {'a': i} a_map, NULL::VARCHAR a_null \
    FROM range(1, 4) t(i)";

/// Read by Octa, written by Octa, compared by DuckDB: same types, same values.
#[test]
fn every_type_survives_a_read_and_a_write() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.lance");
    make(&source, EVERY_TYPE);

    let table = LanceReader.read_file(&source).unwrap();
    assert_eq!(table.format_name.as_deref(), Some("Lance"));
    assert_eq!(table.row_count(), 3);
    let copy = dir.path().join("copy.lance");
    LanceReader.write_file(&copy, &table).unwrap();

    assert_eq!(described(&copy), described(&source));
    assert_eq!(printed(&copy), printed(&source));
}

#[test]
fn a_vector_reads_as_its_values_not_as_debug_text() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.lance");
    make(
        &path,
        "SELECT 1 id, [0.5, 0.25]::FLOAT[2] vec, 12.34::DECIMAL(18,2) d",
    );

    let table = LanceReader.read_file(&path).unwrap();
    assert_eq!(table.columns[1].data_type, "FLOAT[2]");
    assert_eq!(table.get(0, 1).unwrap().to_string(), "[0.5, 0.25]");
    assert_eq!(table.columns[2].data_type, "Decimal128(18, 2)");
    assert_eq!(table.get(0, 2).unwrap().to_string(), "12.34");
    assert_eq!(
        LanceReader.read_schema(&path).unwrap()[1].data_type,
        "FLOAT[2]"
    );
}

/// The whole reason this is more than a plain writer: one edited cell must not
/// turn a dataset's vectors into text or lose the version before.
#[test]
fn saving_an_edit_keeps_the_vectors_and_the_earlier_version() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    make(
        &path,
        "SELECT i::INTEGER id, [i * 0.5, 1.5]::FLOAT[2] vec, 'x' note FROM range(1, 4) t(i)",
    );
    let before = described(&path);

    let mut table = LanceReader.read_file(&path).unwrap();
    table.set(1, 2, CellValue::String("edited".to_string()));
    LanceReader.write_file(&path, &table).unwrap();

    assert_eq!(described(&path), before, "same types");
    assert_eq!(versions(&path), 2, "the earlier version stays");
    assert_eq!(
        printed(&path),
        [
            "1 | [0.5, 1.5] | x",
            "2 | [1.0, 1.5] | edited",
            "3 | [1.5, 1.5] | x"
        ]
    );
}

#[test]
fn an_edit_duckdb_cannot_read_back_fails_and_changes_nothing() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    make(&path, "SELECT 1 id, [0.5, 1.5]::FLOAT[2] vec");
    let before = printed(&path);

    let mut table = LanceReader.read_file(&path).unwrap();
    table.set(0, 1, CellValue::String("not a vector".to_string()));
    let err = LanceReader.write_file(&path, &table).unwrap_err();

    assert!(format!("{err:#}").contains("row 1"), "{err:#}");
    assert_eq!(versions(&path), 1);
    assert_eq!(printed(&path), before);
}

/// Lance refuses a top-level dot, and the reason has to reach the user.
#[test]
fn a_column_name_lance_refuses_says_why_and_changes_nothing() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    make(&path, "SELECT 1 id");
    let before = printed(&path);

    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "a.b".to_string(),
            data_type: "Int64".to_string(),
        }],
        rows: vec![vec![CellValue::Int(1)]],
        ..DataTable::empty()
    };
    let err = LanceReader.write_file(&path, &table).unwrap_err();

    assert!(format!("{err:#}").contains("a.b"), "{err:#}");
    assert_eq!(printed(&path), before);
}

#[test]
fn durations_are_written_as_their_text() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    let two_hours = CellValue::Duration(2 * 3_600 * 1_000_000_000);
    let shown = two_hours.to_string();
    let table = DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "took".to_string(),
                data_type: "Duration(Microsecond)".to_string(),
            },
        ],
        rows: vec![vec![CellValue::Int(1), two_hours]],
        ..DataTable::empty()
    };

    LanceReader.write_file(&path, &table).unwrap();

    assert_eq!(printed(&path), [format!("1 | {shown}")]);
}

/// The wall clock in the cell is Tokyo's, whatever zone this machine is on.
#[test]
fn a_zoned_timestamp_keeps_its_instant() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "at".to_string(),
            data_type: "Timestamp(Microsecond, Some(\"Asia/Tokyo\"))".to_string(),
        }],
        rows: vec![vec![CellValue::DateTime("2026-01-01 09:00:00".to_string())]],
        ..DataTable::empty()
    };

    LanceReader.write_file(&path, &table).unwrap();

    let epoch: i64 = duck()
        .query_row(
            &format!(
                "SELECT epoch(\"at\")::BIGINT FROM __lance_scan('{}')",
                path.display()
            ),
            [],
            |r| r.get(0),
        )
        .unwrap();
    // 2026-01-01 00:00:00 UTC.
    assert_eq!(epoch, 1_767_225_600);
}

/// A save must not mistake the rows it holds for the whole dataset.
#[test]
fn a_capped_read_says_so_and_cannot_overwrite_the_dataset() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    make(&path, "SELECT i id FROM range(5) t(i)");

    let table = {
        let _cap = ThreadRowCap::new(2);
        LanceReader.read_file(&path).unwrap()
    };

    assert_eq!(table.row_count(), 2);
    assert!(table.is_partial());
    assert_eq!(table.known_total(), Some(5));
    assert!(table.refuse_partial_overwrite(&path, &path).is_err());

    let exact = {
        let _cap = ThreadRowCap::new(5);
        LanceReader.read_file(&path).unwrap()
    };
    assert!(!exact.is_partial(), "exactly the cap is the whole dataset");
}

#[test]
fn a_folder_with_any_name_reads_and_writes() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("it's plain");
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "id".to_string(),
            data_type: "Int32".to_string(),
        }],
        rows: vec![vec![CellValue::Int(7)]],
        ..DataTable::empty()
    };

    LanceReader.write_file(&path, &table).unwrap();

    assert_eq!(lakehouse_reader::detect(&path), Some(LakehouseKind::Lance));
    let back = octa::formats::read_table_auto(&path, None, u64::MAX).unwrap();
    assert_eq!(back.get(0, 0), Some(&CellValue::Int(7)));
    assert_eq!(back.format_name.as_deref(), Some("Lance"));
}

#[test]
fn an_empty_table_makes_an_empty_dataset() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.lance");
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        }],
        ..DataTable::empty()
    };

    LanceReader.write_file(&path, &table).unwrap();

    let back = LanceReader.read_file(&path).unwrap();
    assert_eq!(back.row_count(), 0);
    assert_eq!(back.columns[0].data_type, "Int64");
}

/// A DuckDB file's vectors reach Lance as vectors, not as their text.
#[test]
fn a_duckdb_files_vectors_stay_vectors_in_a_dataset() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("v.duckdb");
    duckdb::Connection::open(&source)
        .unwrap()
        .execute_batch(
            "CREATE TABLE t AS SELECT 1 id, [0.5, 1.5]::FLOAT[2] vec, ['p', 'r,s'] words, \
             MAP {'k': 1} mp",
        )
        .unwrap();
    let table = octa::formats::read_table_auto(&source, None, u64::MAX).unwrap();

    let path = dir.path().join("v.lance");
    LanceReader.write_file(&path, &table).unwrap();

    let types: Vec<String> = described(&path).into_iter().map(|(_, t)| t).collect();
    assert_eq!(types[1], "FLOAT[2]");
    assert_eq!(types[2], "VARCHAR[]");
    assert_eq!(types[3], "MAP(VARCHAR, INTEGER)");
    assert_eq!(printed(&path), ["1 | [0.5, 1.5] | [p, 'r,s'] | {k=1}"]);
}

/// Run `git lfs pull` first: the sample's data files are in Git LFS.
#[test]
fn the_sample_opens_with_its_vectors() {
    if !enabled() {
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/lance-table.lance");
    let table = octa::formats::read_table_auto(&path, None, u64::MAX).unwrap();
    assert_eq!(table.row_count(), 4);
    let embedding = table
        .columns
        .iter()
        .find(|c| c.name == "embedding")
        .unwrap();
    assert_eq!(embedding.data_type, "FLOAT[3]");
    assert_eq!(versions(&path), 2);
}

/// A dataset is one table to a folder scan, not its `data/*.lance` fragments.
#[test]
fn a_folder_scan_counts_a_dataset_once() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    make(&dir.path().join("d.lance"), "SELECT 1 id, 'x' AS note");

    let registry = octa::formats::FormatRegistry::new();
    let (files, skipped) = octa::data::schema_drift::collect_schemas(dir.path(), true, &registry);

    assert_eq!(skipped, Vec::new());
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].1.len(), 2);
}

fn octa(config: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_octa"));
    command.env("OCTA_CONFIG_DIR", config);
    command
}

fn stdout_of(output: std::process::Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The real binary, the way the CLI and scripts reach it: through the registry.
#[test]
fn the_command_line_converts_to_and_from_a_dataset() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("in.csv");
    std::fs::write(&csv, "id,name\n1,ada\n2,grace\n").unwrap();
    let dataset = dir.path().join("out.lance");

    stdout_of(
        octa(dir.path())
            .arg("--convert")
            .arg(&csv)
            .arg(&dataset)
            .output()
            .unwrap(),
    );
    assert_eq!(versions(&dataset), 1);

    let head = stdout_of(
        octa(dir.path())
            .arg("--head")
            .arg(&dataset)
            .output()
            .unwrap(),
    );
    assert!(head.contains("grace"), "{head}");

    let back = dir.path().join("back.csv");
    stdout_of(
        octa(dir.path())
            .arg("--convert")
            .arg(&dataset)
            .arg(&back)
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read_to_string(&back).unwrap(),
        "id,name\n1,ada\n2,grace\n"
    );
}

#[test]
fn a_dataset_cannot_go_to_stdout() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("in.csv");
    std::fs::write(&csv, "id\n1\n").unwrap();

    let output = octa(dir.path())
        .arg("--convert")
        .arg(&csv)
        .arg("-")
        .args(["--to", "lance"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("writes a folder"), "{stderr}");
}
