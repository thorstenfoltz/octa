//! Delta / Iceberg time travel against DuckDB's real extensions. They are
//! downloaded on first use, so these run only with `OCTA_TEST_LAKEHOUSE=1`
//! and pass as a no-op otherwise, like `db_live_tests.rs`. The sample-based
//! cases read `samples/`, which is in Git LFS: run `git lfs pull` first.

use std::path::Path;

use octa::formats::lakehouse_history::{self, VersionId};
use octa::formats::lakehouse_reader::{self, LakehouseKind};
use serde_json::{Value, json};

fn enabled() -> bool {
    std::env::var("OCTA_TEST_LAKEHOUSE").is_ok_and(|v| v == "1")
}

fn parquet(dir: &Path, name: &str, select: &str) -> u64 {
    let path = dir.join(name);
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "COPY ({select}) TO '{}' (FORMAT parquet)",
        path.display()
    ))
    .unwrap();
    std::fs::metadata(&path).unwrap().len()
}

fn schema(fields: &[(&str, &str)]) -> Value {
    let fields: Vec<Value> = fields
        .iter()
        .map(|(n, t)| json!({"name": n, "type": t, "nullable": true, "metadata": {}}))
        .collect();
    json!({"metaData": {
        "id": "6c0b1d6e-0000-4000-8000-000000000001",
        "format": {"provider": "parquet", "options": {}},
        "schemaString": json!({"type": "struct", "fields": fields}).to_string(),
        "partitionColumns": [], "configuration": {}, "createdTime": 1_700_000_000_000i64
    }})
}

fn add(path: &str, size: u64) -> Value {
    json!({"add": {"path": path, "partitionValues": {}, "size": size,
                   "modificationTime": 1_700_000_000_000i64, "dataChange": true}})
}

fn remove(path: &str) -> Value {
    json!({"remove": {"path": path, "deletionTimestamp": 1_700_000_100_000i64, "dataChange": true}})
}

fn commit(dir: &Path, n: u64, op: &str, actions: Vec<Value>) {
    let mut lines = vec![
        json!({"commitInfo": {"timestamp": 1_700_000_000_000i64 + n as i64, "operation": op}})
            .to_string(),
    ];
    lines.extend(actions.iter().map(Value::to_string));
    std::fs::write(
        dir.join("_delta_log").join(format!("{n:020}.json")),
        lines.join("\n") + "\n",
    )
    .unwrap();
}

/// v0 rows 1-3, v1 appends 4, v2 removes it again, v3 adds a `discount`
/// column and row 5.
fn delta_table() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::create_dir(p.join("_delta_log")).unwrap();
    let p0 = parquet(
        p,
        "p0.parquet",
        "SELECT * FROM (VALUES (1::BIGINT, 'a'), (2, 'b'), (3, 'c')) t(id, name)",
    );
    commit(
        p,
        0,
        "WRITE",
        vec![
            json!({"protocol": {"minReaderVersion": 1, "minWriterVersion": 2}}),
            schema(&[("id", "long"), ("name", "string")]),
            add("p0.parquet", p0),
        ],
    );
    let p1 = parquet(p, "p1.parquet", "SELECT 4::BIGINT AS id, 'd' AS name");
    commit(p, 1, "WRITE", vec![add("p1.parquet", p1)]);
    commit(p, 2, "DELETE", vec![remove("p1.parquet")]);
    let p3 = parquet(
        p,
        "p3.parquet",
        "SELECT 5::BIGINT AS id, 'e' AS name, 0.1::DOUBLE AS discount",
    );
    commit(
        p,
        3,
        "WRITE",
        vec![
            schema(&[("id", "long"), ("name", "string"), ("discount", "double")]),
            add("p3.parquet", p3),
        ],
    );
    dir
}

#[test]
fn delta_versions_read_and_compare() {
    if !enabled() {
        return;
    }
    let t = delta_table();
    let kind = LakehouseKind::Delta;
    let rows = |n| {
        lakehouse_reader::read_version(t.path(), kind, &VersionId::Delta(n))
            .unwrap()
            .row_count()
    };
    assert_eq!([rows(0), rows(1), rows(2), rows(3)], [3, 4, 3, 4]);

    let conn = lakehouse_reader::open_connection(kind).unwrap();
    let diff = |a, b| {
        lakehouse_reader::diff_versions(
            &conn,
            t.path(),
            kind,
            &VersionId::Delta(a),
            &VersionId::Delta(b),
            1000,
        )
        .unwrap()
    };
    let d = diff(0, 1);
    assert_eq!((d.added, d.removed), (1, 0));
    assert_eq!(d.table.columns[0].name, "change");
    assert_eq!(
        d.table.get(0, 0),
        Some(&octa::data::CellValue::String("added".into()))
    );
    let d = diff(1, 2);
    assert_eq!((d.added, d.removed), (0, 1));
    let d = diff(2, 3);
    assert_eq!(d.only_in_newer, ["discount"]);
    assert_eq!((d.added, d.removed), (1, 0));

    let h = lakehouse_history::list(t.path(), kind, 500).unwrap();
    assert_eq!(h.versions[1].operation.as_deref(), Some("DELETE"));
}

#[test]
fn a_vacuumed_version_says_so() {
    if !enabled() {
        return;
    }
    let t = delta_table();
    std::fs::remove_file(t.path().join("p1.parquet")).unwrap();
    let err = lakehouse_reader::read_version(t.path(), LakehouseKind::Delta, &VersionId::Delta(1))
        .unwrap_err();
    assert!(format!("{err:#}").contains("VACUUM"), "{err:#}");
}

fn sample(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name)
}

#[test]
fn delta_sample_has_three_versions() {
    if !enabled() {
        return;
    }
    let dir = sample("delta-table");
    let h = lakehouse_history::list(&dir, LakehouseKind::Delta, 500).unwrap();
    assert_eq!(h.total, 3);
    let rows: Vec<usize> = (0..3)
        .map(|n| {
            lakehouse_reader::read_version(&dir, LakehouseKind::Delta, &VersionId::Delta(n))
                .unwrap()
                .row_count()
        })
        .collect();
    assert_eq!(rows, [8, 10, 9]);
}

#[test]
fn iceberg_sample_snapshots_read_and_compare() {
    if !enabled() {
        return;
    }
    let dir = sample("iceberg-table");
    let kind = LakehouseKind::Iceberg;
    let h = lakehouse_history::list(&dir, kind, 500).unwrap();
    assert_eq!(h.total, 2);
    let (newest, first) = (&h.versions[0], &h.versions[1]);
    assert_eq!(
        lakehouse_reader::read_version(&dir, kind, &first.id)
            .unwrap()
            .row_count(),
        3
    );
    assert_eq!(
        lakehouse_reader::read_version(&dir, kind, &newest.id)
            .unwrap()
            .row_count(),
        4
    );
    let conn = lakehouse_reader::open_connection(kind).unwrap();
    let d = lakehouse_reader::diff_versions(
        &conn,
        &dir,
        kind,
        newest.previous.as_ref().unwrap(),
        &newest.id,
        1000,
    )
    .unwrap();
    assert_eq!((d.added, d.removed), (1, 0));
    // The latest version opens like any table folder, from where it sits now.
    assert_eq!(
        lakehouse_reader::read_dir(&dir, kind).unwrap().row_count(),
        4
    );
}
