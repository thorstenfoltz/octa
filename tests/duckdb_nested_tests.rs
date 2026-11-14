//! Nested DuckDB columns (vectors, lists, structs, maps) through every way Octa
//! reads and writes them. The bundled DuckDB is all these need, with no extension,
//! network or env var, so `cargo test` runs them on all three CI platforms and they
//! cannot pass as a no-op.
//!
//! Before this suite, such a column was typed by a word search (`INTEGER[]` =
//! `Int64`) and each cell held a debug dump of its whole batch, so converting a
//! `.duckdb` file to Parquet wrote NULL over every vector and reported success.

use std::path::Path;

use duckdb::Connection;
use octa::data::{CellValue, DataTable};
use octa::formats::duckdb_reader::DuckDbReader;
use octa::formats::{FormatReader, read_table_auto};

const SETUP: &str = "CREATE TABLE t AS SELECT i::INTEGER id, [i * 0.5, 1.5]::FLOAT[2] vec, \
    [i, 2] lst, {'x': i, 'y': 'a, b'} st, MAP {'k': i} mp, ['p', 'r,s', 'it''s'] words, \
    'plain' note FROM range(1, 3) t(i);";

fn sample(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("nested.duckdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(SETUP)
        .unwrap();
    path
}

fn read(path: &Path) -> DataTable {
    read_table_auto(path, None, u64::MAX).unwrap()
}

fn cell(table: &DataTable, row: usize, column: &str) -> String {
    let col = table
        .columns
        .iter()
        .position(|c| c.name == column)
        .unwrap_or_else(|| panic!("no column {column}"));
    table.get(row, col).unwrap().to_string()
}

/// What DuckDB itself sees in the file, through nothing of Octa's.
fn in_file(path: &Path, expr: &str) -> String {
    Connection::open(path)
        .unwrap()
        .query_row(
            &format!("SELECT CAST({expr} AS VARCHAR) FROM t WHERE id = 1"),
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn assert_nothing_is_a_debug_dump(table: &DataTable) {
    for row in 0..table.row_count() {
        for col in 0..table.col_count() {
            let text = table.get(row, col).unwrap().to_string();
            for dump in ["Array<", "ListArray", "StructArray", "MapArray", "Regular("] {
                assert!(!text.contains(dump), "row {row} col {col}: {text}");
            }
        }
    }
}

#[test]
fn a_file_reads_with_its_real_types_and_each_rows_own_values() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));

    assert_nothing_is_a_debug_dump(&table);
    let type_of = |name: &str| {
        table
            .columns
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .data_type
            .clone()
    };
    assert_eq!(type_of("vec"), "FLOAT[2]");
    assert_eq!(type_of("lst"), "BIGINT[]");
    assert_eq!(type_of("st"), "STRUCT(x BIGINT, y VARCHAR)");
    assert_eq!(type_of("mp"), "MAP(VARCHAR, BIGINT)");
    assert_eq!(type_of("id"), "Int64");

    assert_eq!(cell(&table, 0, "vec"), "[0.5, 1.5]");
    assert_eq!(cell(&table, 1, "vec"), "[1.0, 1.5]");
    assert_eq!(cell(&table, 1, "lst"), "[2, 2]");
    assert_eq!(cell(&table, 0, "st"), "{'x': 1, 'y': 'a, b'}");
    assert_eq!(cell(&table, 0, "mp"), "{k=1}");
    assert_eq!(cell(&table, 0, "words"), "[p, 'r,s', 'it\\'s']");
}

/// The silent data loss: every format has to come out holding the values.
#[test]
fn converting_to_any_format_keeps_the_values() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let registry = octa::formats::FormatRegistry::new();

    for ext in [
        "csv", "tsv", "parquet", "json", "jsonl", "orc", "avro", "arrow", "xlsx",
    ] {
        let out = dir.path().join(format!("out.{ext}"));
        let writer = registry.reader_for_path(&out).unwrap();
        writer
            .write_file(&out, &table)
            .unwrap_or_else(|e| panic!("{ext}: {e:#}"));
        let back = read(&out);
        for (column, wanted) in [
            ("vec", "0.5"),
            ("lst", "2"),
            ("st", "a, b"),
            ("mp", "k"),
            ("words", "r,s"),
        ] {
            let text = cell(&back, 0, column);
            assert!(
                text.contains(wanted),
                "{ext}: {column} came back as {text:?}"
            );
        }
    }
}

#[test]
fn parquet_holds_the_text_and_duckdb_can_read_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let table = read(&sample(dir.path()));
    let out = dir.path().join("out.parquet");
    octa::formats::parquet_reader::ParquetReader
        .write_file(&out, &table)
        .unwrap();

    let same: bool = Connection::open_in_memory()
        .unwrap()
        .query_row(
            &format!(
                "SELECT CAST(vec AS FLOAT[2]) = [0.5, 1.5]::FLOAT[2] \
                 AND CAST(words AS VARCHAR[]) = ['p', 'r,s', 'it''s'] \
                 FROM '{}' WHERE id = 1",
                out.display()
            ),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(same);
}

/// Editing another column must leave a vector exactly as it is.
#[test]
fn saving_an_edit_leaves_a_nested_column_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let before = in_file(&path, "vec") + &in_file(&path, "words") + &in_file(&path, "mp");

    let mut table = read(&path);
    let note = table.columns.iter().position(|c| c.name == "note").unwrap();
    table.set(0, note, CellValue::String("edited".to_string()));
    DuckDbReader.write_file(&path, &table).unwrap();

    assert_eq!(in_file(&path, "note"), "edited");
    assert_eq!(
        in_file(&path, "vec") + &in_file(&path, "words") + &in_file(&path, "mp"),
        before
    );
    assert_eq!(in_file(&path, "typeof(vec)"), "FLOAT[2]");
}

#[test]
fn editing_a_nested_cell_writes_that_type_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let mut table = read(&path);
    let vec = table.columns.iter().position(|c| c.name == "vec").unwrap();
    table.set(0, vec, CellValue::String("[9.0, 8.0]".to_string()));

    DuckDbReader.write_file(&path, &table).unwrap();

    assert_eq!(in_file(&path, "vec"), "[9.0, 8.0]");
    assert_eq!(in_file(&path, "typeof(vec)"), "FLOAT[2]");
}

/// A save in one transaction: nothing of a refused edit may reach the file.
#[test]
fn a_nested_edit_duckdb_cannot_read_fails_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let mut table = read(&path);
    let vec = table.columns.iter().position(|c| c.name == "vec").unwrap();
    let note = table.columns.iter().position(|c| c.name == "note").unwrap();
    table.set(0, note, CellValue::String("must not land".to_string()));
    table.set(0, vec, CellValue::String("not a vector".to_string()));

    assert!(DuckDbReader.write_file(&path, &table).is_err());

    assert_eq!(in_file(&path, "vec"), "[0.5, 1.5]");
    assert_eq!(in_file(&path, "note"), "plain");
}

/// A schema change rewrites whole rows, so the text really goes through the cast.
#[test]
fn a_schema_change_rewrites_nested_cells_without_changing_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let before = in_file(&path, "words") + &in_file(&path, "st") + &in_file(&path, "mp");

    let mut table = read(&path);
    let end = table.col_count();
    table.insert_column(end, "added".to_string(), "Utf8".to_string());
    DuckDbReader
        .write_file_schema_aware(&path, &table, true, &Default::default())
        .unwrap();

    assert_eq!(
        in_file(&path, "words") + &in_file(&path, "st") + &in_file(&path, "mp"),
        before
    );
    assert_eq!(in_file(&path, "typeof(words)"), "VARCHAR[]");
}

/// The SQL panel, `--sql` and MCP `run_sql` all read through here.
#[test]
fn a_query_shows_nested_values_as_text() {
    let table = DataTable {
        columns: vec![octa::data::ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        }],
        rows: vec![vec![CellValue::Int(1)]],
        ..DataTable::empty()
    };
    let out = octa::sql::run_query(
        &table,
        "SELECT [id, 2] l, {'x': id} s, MAP {'k': id} m, [0.5, 1.5]::FLOAT[2] v FROM data",
    )
    .unwrap()
    .table;

    assert_nothing_is_a_debug_dump(&out);
    assert_eq!(cell(&out, 0, "l"), "[1, 2]");
    assert_eq!(cell(&out, 0, "s"), "{'x': 1}");
    assert_eq!(cell(&out, 0, "m"), "{k=1}");
    assert_eq!(cell(&out, 0, "v"), "[0.5, 1.5]");
}

/// Large-file mode reads a Parquet in place through the same conversion.
#[test]
fn large_file_mode_shows_nested_values_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n.parquet");
    Connection::open_in_memory()
        .unwrap()
        .execute_batch(&format!(
            "COPY (SELECT 1 id, [1, 2] lst, {{'x': 1}} st) TO '{}' (FORMAT parquet)",
            path.display()
        ))
        .unwrap();

    let large = octa::formats::large::open(&path).unwrap();
    assert_eq!(large.columns()[1].data_type, "INTEGER[]");
    let page = large.page(0, 10, None, None).unwrap();

    assert_nothing_is_a_debug_dump(&page);
    assert_eq!(cell(&page, 0, "lst"), "[1, 2]");
    assert_eq!(cell(&page, 0, "st"), "{'x': 1}");
}

/// The driver panics on this type, which ended the process: `--sql` exited 101.
/// SQL cannot be rewritten, so it says what to do. The rest read it as text.
#[test]
fn a_nanosecond_time_is_never_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let parquet = dir.path().join("t.parquet");
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "COPY (SELECT 1 id, '12:34:56.123456789'::TIME_NS t) TO '{}' (FORMAT parquet)",
        parquet.display()
    ))
    .unwrap();
    let time = "12:34:56.123456789";

    let table = read(&parquet);
    let result = octa::sql::run_query(&table, "SELECT '12:34:56.123456789'::TIME_NS AS v");
    let Err(err) = result else {
        panic!("refused, not read");
    };
    assert!(
        format!("{err:#}").contains("CAST(\"v\" AS VARCHAR)"),
        "{err:#}"
    );
    let as_text = octa::sql::run_query(
        &table,
        "SELECT CAST('12:34:56.123456789'::TIME_NS AS VARCHAR) AS v",
    )
    .unwrap()
    .table;
    assert_eq!(cell(&as_text, 0, "v"), time);

    let large = octa::formats::large::open(&parquet).unwrap();
    let page = large.page(0, 10, None, None).unwrap();
    assert_eq!(cell(&page, 0, "t"), time);
    let values = large.value_frequency_job(1, "", 10).unwrap()().unwrap();
    assert_eq!(values.rows[0].label, time);

    let parts = dir.path().join("parts");
    std::fs::create_dir(&parts).unwrap();
    std::fs::copy(&parquet, parts.join("part-0.parquet")).unwrap();
    let kind = octa::formats::lakehouse_reader::detect(&parts).unwrap();
    let dataset = octa::formats::lakehouse_reader::read_dir(&parts, kind).unwrap();
    assert_eq!(cell(&dataset, 0, "t"), time);
}

/// The real binary, end to end.
#[test]
fn the_command_line_shows_values_and_converts_without_losing_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample(dir.path());
    let octa = || {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_octa"));
        command.env("OCTA_CONFIG_DIR", dir.path());
        command
    };

    let head = octa().arg("--head").arg(&path).output().unwrap();
    assert!(head.status.success());
    let head = String::from_utf8(head.stdout).unwrap();
    assert!(head.contains("[0.5, 1.5]"), "{head}");
    assert!(!head.contains("ListArray"), "{head}");

    let out = dir.path().join("o.csv");
    let converted = octa()
        .arg("--convert")
        .arg(&path)
        .arg(&out)
        .output()
        .unwrap();
    assert!(converted.status.success());
    let csv = std::fs::read_to_string(&out).unwrap();
    assert!(csv.contains("\"[0.5, 1.5]\""), "{csv}");
    assert_eq!(csv.lines().count(), 3, "one line per row: {csv}");
}
