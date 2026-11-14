//! Exact decimals through every surface. Bundled DuckDB only, so `cargo test` runs
//! these on all three CI platforms and they cannot pass as a no-op.
//!
//! Before: `DECIMAL` was typed `Float64`, so Parquet got a DOUBLE (`...456.78`
//! became `1.23e+35`), a Parquet decimal re-saved as text, and a column read from a
//! `.duckdb` file sorted as text, with `10.00` before `9.00`.

use std::path::Path;

use duckdb::Connection;
use octa::data::{CellValue, ColumnInfo, DataTable, TextOrder, is_numeric_data_type};
use octa::formats::duckdb_reader::DuckDbReader;
use octa::formats::parquet_reader::ParquetReader;
use octa::formats::{FormatReader, read_table_auto};

const BIG: &str = "123456789012345678901234567890123456.78";

fn sample(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("money.duckdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE t (id INTEGER, price DECIMAL(18,2), big DECIMAL(38,2), note VARCHAR);
             INSERT INTO t VALUES
                 (1, 9.00, {BIG}, 'a'),
                 (2, 10.00, 0.05, 'b'),
                 (3, -1.50, NULL, 'c'),
                 (4, 1234567890123456.78, 1.00, 'd');"
        ))
        .unwrap();
    path
}

fn read(path: &Path) -> DataTable {
    read_table_auto(path, None, u64::MAX).unwrap()
}

fn col(table: &DataTable, name: &str) -> usize {
    table.columns.iter().position(|c| c.name == name).unwrap()
}

fn cell(table: &DataTable, row: usize, name: &str) -> String {
    table.get(row, col(table, name)).unwrap().to_string()
}

/// DuckDB's own reading of a file, through nothing of Octa's.
fn duck_says(sql: &str) -> String {
    Connection::open_in_memory()
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[test]
fn a_decimal_column_has_its_real_type_and_every_digit() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));

    assert_eq!(
        table.columns[col(&table, "price")].data_type,
        "Decimal128(18, 2)"
    );
    assert_eq!(cell(&table, 0, "price"), "9.00");
    assert_eq!(cell(&table, 0, "big"), BIG);
    assert!(is_numeric_data_type("Decimal128(18, 2)"));
    assert!(is_numeric_data_type("Decimal256(50, 2)"));
}

/// The case that was asked for.
#[test]
fn parquet_gets_a_real_decimal_with_every_digit() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let out = dir.path().join("out.parquet");
    ParquetReader.write_file(&out, &table).unwrap();

    let p = out.display();
    assert_eq!(
        duck_says(&format!("SELECT typeof(big) FROM '{p}' LIMIT 1")),
        "DECIMAL(38,2)"
    );
    assert_eq!(
        duck_says(&format!("SELECT typeof(price) FROM '{p}' LIMIT 1")),
        "DECIMAL(18,2)"
    );
    assert_eq!(
        duck_says(&format!("SELECT big::VARCHAR FROM '{p}' WHERE id = 1")),
        BIG
    );
    assert_eq!(
        duck_says(&format!("SELECT price::VARCHAR FROM '{p}' WHERE id = 4")),
        "1234567890123456.78"
    );
    assert_eq!(
        duck_says(&format!(
            "SELECT coalesce(big::VARCHAR, 'null') FROM '{p}' WHERE id = 3"
        )),
        "null",
        "a null stays a null"
    );
}

#[test]
fn a_parquet_decimal_stays_exact_through_octa_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.parquet");
    ParquetReader
        .write_file(&first, &read(&sample(dir.path())))
        .unwrap();

    let table = read(&first);
    assert_eq!(cell(&table, 0, "big"), BIG, "read exactly");
    assert_eq!(cell(&table, 1, "big"), "0.05");
    assert_eq!(
        table.get(1, col(&table, "price")),
        Some(&CellValue::Float(10.0))
    );

    let second = dir.path().join("second.parquet");
    ParquetReader.write_file(&second, &table).unwrap();
    let p = second.display();
    assert_eq!(
        duck_says(&format!("SELECT typeof(big) FROM '{p}' LIMIT 1")),
        "DECIMAL(38,2)",
        "it used to come back as text"
    );
    assert_eq!(
        duck_says(&format!("SELECT big::VARCHAR FROM '{p}' WHERE id = 1")),
        BIG
    );
}

#[test]
fn arrow_gets_the_same_real_decimal() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let out = dir.path().join("out.arrow");
    octa::formats::arrow_ipc_reader::ArrowIpcReader
        .write_file(&out, &table)
        .unwrap();

    let back = read(&out);
    assert_eq!(
        back.columns[col(&back, "big")].data_type,
        "Decimal128(38, 2)"
    );
    assert_eq!(cell(&back, 0, "big"), BIG);
}

/// The formats with no decimal type of their own still keep the digits. This looks
/// at what the file holds, not at `read_table_auto`, which goes on to read a column
/// of number-like text as floats: opening such a file rounds it, a separate matter
/// from whether it was written correctly.
#[test]
fn every_format_writes_every_digit() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let registry = octa::formats::FormatRegistry::new();
    let write = |ext: &str| {
        let out = dir.path().join(format!("out.{ext}"));
        let writer = registry.reader_for_path(&out).unwrap();
        writer
            .write_file(&out, &table)
            .unwrap_or_else(|e| panic!("{ext}: {e:#}"));
        out
    };

    for ext in ["csv", "tsv", "json", "jsonl"] {
        let text = std::fs::read_to_string(write(ext)).unwrap();
        assert!(text.contains(BIG), "{ext}: {text}");
    }
    for ext in ["orc", "avro", "xlsx", "parquet", "arrow"] {
        let out = write(ext);
        let back = registry
            .reader_for_path(&out)
            .unwrap()
            .read_file(&out)
            .unwrap();
        assert_eq!(cell(&back, 0, "big"), BIG, "{ext}");
    }
}

#[test]
fn a_decimal_column_sorts_by_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut table = read(&sample(dir.path()));
    let price = col(&table, "price");

    table.sort_rows_by_columns(&[(price, true)], TextOrder::Letters);
    let sorted: Vec<String> = (0..4).map(|r| cell(&table, r, "price")).collect();
    assert_eq!(sorted, ["-1.50", "9.00", "10.00", "1234567890123456.78"]);

    table.sort_rows_by_columns(&[(price, false)], TextOrder::Letters);
    assert_eq!(cell(&table, 0, "price"), "1234567890123456.78");
    assert_eq!(cell(&table, 3, "price"), "-1.50");
}

#[test]
fn nulls_and_typed_text_in_a_decimal_column_still_sort() {
    let dir = tempfile::tempdir().unwrap();
    let mut table = read(&sample(dir.path()));
    let big = col(&table, "big");
    table.set(1, big, CellValue::String("not a number".to_string()));

    table.sort_rows_by_columns(&[(big, true)], TextOrder::Letters);
    assert_eq!(
        table.get(0, big),
        Some(&CellValue::Null),
        "nulls first, as ever"
    );
}

/// Writing a null here would lose the value and report success.
#[test]
fn a_value_that_cannot_be_stored_stops_the_write_and_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.parquet");
    let table = read(&sample(dir.path()));
    ParquetReader.write_file(&path, &table).unwrap();
    let before = std::fs::read(&path).unwrap();

    let mut edited = table.clone();
    edited.set(
        1,
        col(&table, "price"),
        CellValue::String("about ten".to_string()),
    );
    let err = ParquetReader.write_file(&path, &edited).unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("about ten") && text.contains("row 2"),
        "{text}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the file is untouched"
    );

    let arrow = dir.path().join("out.arrow");
    let ipc = octa::formats::arrow_ipc_reader::ArrowIpcReader;
    assert!(ipc.write_file(&arrow, &edited).is_err());
    assert!(!arrow.exists(), "the refusal comes before the file");

    let mut too_big = table.clone();
    too_big.set(
        0,
        col(&table, "price"),
        CellValue::String("1e30".to_string()),
    );
    assert!(ParquetReader.write_file(&path, &too_big).is_err());
    too_big.set(
        0,
        col(&table, "price"),
        CellValue::String("12345678901234567890.00".to_string()),
    );
    assert!(ParquetReader.write_file(&path, &too_big).is_err());
}

/// The SQL panel's copy of the table is a real decimal, so a sum stays exact where a
/// double would already have lost the cents.
#[test]
fn sql_over_a_decimal_column_is_exact() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "d".to_string(),
            data_type: "Decimal128(18, 2)".to_string(),
        }],
        rows: vec![
            vec![CellValue::String("1234567890123456.78".to_string())],
            vec![CellValue::String("0.01".to_string())],
        ],
        ..DataTable::empty()
    };

    let out = octa::sql::run_query(&table, "SELECT SUM(d) s, any_value(typeof(d)) t FROM data")
        .unwrap()
        .table;

    assert_eq!(cell(&out, 0, "s"), "1234567890123456.79");
    assert_eq!(cell(&out, 0, "t"), "DECIMAL(18,2)");
}

#[test]
fn the_summary_tab_still_reads_a_decimal_as_a_number() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let stats = octa::data::summary::SummaryStat::default_enabled();

    let summary = octa::data::summary::build_summary_table(&table, &stats).unwrap();

    let row = (0..summary.row_count())
        .find(|&r| cell(&summary, r, "column_name") == "price")
        .unwrap();
    assert_eq!(cell(&summary, row, "min"), "-1.5");
}

/// Editing a `.duckdb` decimal goes back as that decimal.
#[test]
fn saving_a_duckdb_file_keeps_its_decimal_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let mut table = read(&path);
    table.set(
        1,
        col(&table, "price"),
        CellValue::String("10.50".to_string()),
    );

    DuckDbReader.write_file(&path, &table).unwrap();

    let conn = Connection::open(&path).unwrap();
    let (kind, value, big): (String, String, String) = conn
        .query_row(
            "SELECT typeof(price), price::VARCHAR, (SELECT big::VARCHAR FROM t WHERE id = 1)
             FROM t WHERE id = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), value.as_str(), big.as_str()),
        ("DECIMAL(18,2)", "10.50", BIG)
    );
}

#[test]
fn a_new_decimal_column_in_a_duckdb_file_is_a_decimal() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let mut table = read(&path);
    let end = table.col_count();
    table.insert_column(end, "added".to_string(), "Decimal128(10, 2)".to_string());

    DuckDbReader
        .write_file_schema_aware(&path, &table, true, &Default::default())
        .unwrap();

    let kind: String = Connection::open(&path)
        .unwrap()
        .query_row("SELECT typeof(added) FROM t LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "DECIMAL(10,2)");
}

/// Save to database and Export schema used to see `Float64` here, then would have
/// seen an unknown type and written TEXT.
#[test]
fn schema_export_writes_a_real_decimal_in_every_dialect() {
    use octa::data::schema_export::SchemaTarget;
    let columns = [ColumnInfo {
        name: "price".to_string(),
        data_type: "Decimal128(18, 2)".to_string(),
    }];
    for (target, wanted) in [
        (SchemaTarget::PostgresSqlDdl, "NUMERIC(18, 2)"),
        (SchemaTarget::MysqlSqlDdl, "DECIMAL(18, 2)"),
        (SchemaTarget::SqliteSqlDdl, "NUMERIC"),
        (SchemaTarget::DatabricksSqlDdl, "DECIMAL(18, 2)"),
        (SchemaTarget::SnowflakeSqlDdl, "NUMBER(18, 2)"),
        (SchemaTarget::MssqlSqlDdl, "DECIMAL(18, 2)"),
        (SchemaTarget::PydanticV2, "float"),
        (SchemaTarget::TypeScript, "number"),
        (SchemaTarget::JsonSchema, "number"),
        (SchemaTarget::RustStruct, "f64"),
    ] {
        let out = target.export(&columns, "t");
        assert!(out.contains(wanted), "{target:?}: {out}");
        assert!(
            !out.contains("Decimal128"),
            "{target:?} called it unknown: {out}"
        );
    }
}

/// Delta and Iceberg tables and folders of Parquet parts read through the same type.
#[test]
fn a_folder_of_parts_keeps_its_decimals_too() {
    let dir = tempfile::tempdir().unwrap();
    let parts = dir.path().join("parts");
    std::fs::create_dir(&parts).unwrap();
    Connection::open_in_memory()
        .unwrap()
        .execute_batch(&format!(
            "COPY (SELECT {BIG}::DECIMAL(38,2) big) TO '{}' (FORMAT parquet)",
            parts.join("part-0.parquet").display()
        ))
        .unwrap();

    let kind = octa::formats::lakehouse_reader::detect(&parts).unwrap();
    let table = octa::formats::lakehouse_reader::read_dir(&parts, kind).unwrap();

    assert_eq!(table.columns[0].data_type, "Decimal128(38, 2)");
    assert_eq!(cell(&table, 0, "big"), BIG);
}
