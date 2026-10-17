use super::*;
use crate::data::{CellValue, ColumnInfo};
use crate::db::{DbEngine, DbWriteMode, DbWriteReport};

/// A server with a few tables, handing each over in pages of two rows.
struct Fake {
    tables: Vec<(&'static str, Vec<i64>)>,
}

impl Fake {
    fn rows_of(&self, sql: &str) -> &[i64] {
        self.tables
            .iter()
            .find(|(name, _)| sql.contains(&format!("\"{name}\"")))
            .map(|(_, rows)| rows.as_slice())
            .unwrap_or(&[])
    }
}

fn table(values: &[i64]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "price".into(),
            data_type: "Float64".into(),
        },
    ];
    t.rows = values
        .iter()
        .map(|&v| vec![CellValue::Int(v), CellValue::Float(v as f64 + 0.5)])
        .collect();
    t
}

impl DbConnector for Fake {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec!["public".into()])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.tables.iter().map(|(n, _)| n.to_string()).collect())
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        if sql.contains("\"broken\"") {
            bail!("permission denied for table broken");
        }
        Ok(table(
            &self
                .rows_of(sql)
                .iter()
                .take(1)
                .copied()
                .collect::<Vec<_>>(),
        ))
    }
    fn engine(&self) -> DbEngine {
        DbEngine::Postgres
    }
    fn fetch_batches(
        &mut self,
        sql: &str,
        _: usize,
        sink: &mut dyn FnMut(DataTable) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if sql.contains("\"broken\"") {
            bail!("permission denied for table broken");
        }
        for page in self.rows_of(sql).to_vec().chunks(2) {
            sink(table(page))?;
        }
        Ok(())
    }
    fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        bail!("read only")
    }
}

fn request(dir: &Path, format: ExportFormat, tables: &[&str]) -> ExportRequest {
    ExportRequest {
        catalog: None,
        schema: "public".into(),
        tables: tables.iter().map(|t| t.to_string()).collect(),
        dir: dir.to_path_buf(),
        format,
        overwrite: false,
        options: WriteOptions::default(),
    }
}

fn fake() -> Fake {
    Fake {
        tables: vec![
            ("orders", vec![1, 2, 3, 4, 5]),
            ("empty", vec![]),
            ("broken", vec![1]),
        ],
    }
}

#[test]
fn every_page_reaches_the_file_and_one_failure_does_not_stop_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(
        tmp.path(),
        ExportFormat::Csv,
        &["orders", "broken", "empty"],
    );
    let out = export_tables(&mut fake(), &req, &|_| {}, &AtomicBool::new(false)).unwrap();

    assert!(matches!(&out[0], TableOutcome::Written { rows: 5, .. }));
    assert!(
        matches!(&out[1], TableOutcome::Failed { error, .. } if error.contains("permission denied"))
    );
    assert!(
        !tmp.path().join("broken.csv").exists(),
        "nothing half-written"
    );
    assert!(matches!(&out[2], TableOutcome::Written { rows: 0, .. }));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("orders.csv")).unwrap(),
        "id,price\n1,1.5\n2,2.5\n3,3.5\n4,4.5\n5,5.5\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("empty.csv")).unwrap(),
        "id,price\n",
        "an empty table still gets its header"
    );
}

#[test]
fn parquet_and_json_lines_hold_every_row() {
    let tmp = tempfile::tempdir().unwrap();
    for format in [ExportFormat::Parquet, ExportFormat::Jsonl] {
        let req = request(tmp.path(), format, &["orders"]);
        export_tables(&mut fake(), &req, &|_| {}, &AtomicBool::new(false)).unwrap();
        let path = tmp.path().join(format!("orders.{}", format.extension()));
        let back = crate::formats::read_table_auto(&path, None, u64::MAX).unwrap();
        assert_eq!(back.row_count(), 5, "{format:?}");
        assert_eq!(back.get(4, 1), Some(&CellValue::Float(5.5)), "{format:?}");
    }
}

#[test]
fn an_existing_file_is_skipped_unless_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("orders.csv"), "keep me").unwrap();
    let mut req = request(tmp.path(), ExportFormat::Csv, &["orders"]);
    let out = export_tables(&mut fake(), &req, &|_| {}, &AtomicBool::new(false)).unwrap();
    assert!(matches!(&out[0], TableOutcome::Skipped { .. }));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("orders.csv")).unwrap(),
        "keep me"
    );

    req.overwrite = true;
    let out = export_tables(&mut fake(), &req, &|_| {}, &AtomicBool::new(false)).unwrap();
    assert!(matches!(&out[0], TableOutcome::Written { rows: 5, .. }));
}

#[test]
fn cancel_stops_before_the_next_table() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(tmp.path(), ExportFormat::Csv, &["orders", "empty"]);
    let cancel = AtomicBool::new(false);
    // Cancel as soon as the first page of the first table is written.
    let out = export_tables(
        &mut fake(),
        &req,
        &|p| {
            if p.rows > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert!(matches!(&out[0], TableOutcome::Failed { error, .. } if error == "cancelled"));
    assert!(!tmp.path().join("orders.csv").exists());
}

#[test]
fn file_names_are_safe_and_never_collide() {
    let mut used = HashSet::new();
    assert_eq!(
        file_name("orders", ExportFormat::Csv, &mut used),
        "orders.csv"
    );
    assert_eq!(
        file_name("Orders", ExportFormat::Csv, &mut used),
        "Orders_2.csv"
    );
    assert_eq!(
        file_name("a/b:c", ExportFormat::Csv, &mut used),
        "a_b_c.csv"
    );
    assert_eq!(file_name("..", ExportFormat::Csv, &mut used), "table.csv");
    assert_eq!(ExportFormat::parse(".NDJSON"), Some(ExportFormat::Jsonl));
    assert_eq!(ExportFormat::parse("xlsx"), None);
}
