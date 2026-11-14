//! Numbers read from text keep every digit, and Octa's sums add without float
//! noise. Bundled DuckDB only, so `cargo test` runs these on every platform.
//!
//! Before: `--convert a.csv b.csv` turned `12345678901234567890.12` into
//! `12345678901234567000`, CSV to Parquet stored DOUBLE, and a sum of 0.1, 0.2
//! and 0.3 was 0.6000000000000001, which does not equal 0.6.

use std::path::{Path, PathBuf};

use octa::data::calculated::{CalcKind, CalcSpec, compute};
use octa::data::cohort::{CohortSpec, Period, Show, collect};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::{FormatRegistry, read_table_auto};

const BIG: &str = "12345678901234567890.12";

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn read(path: &Path) -> DataTable {
    read_table_auto(path, None, u64::MAX).unwrap()
}

fn save(table: &DataTable, path: &Path) {
    FormatRegistry::new()
        .reader_for_path(path)
        .unwrap()
        .write_file(path, table)
        .unwrap();
}

fn col(t: &DataTable, name: &str) -> usize {
    t.columns.iter().position(|c| c.name == name).unwrap()
}

fn money_csv(dir: &Path) -> PathBuf {
    write(
        dir,
        "money.csv",
        &format!("id,amount,big,sci\n1,0.1,{BIG},1e5\n2,0.2,1.00,2e3\n3,0.3,2.50,3\n"),
    )
}

#[test]
fn a_text_file_reads_its_numbers_as_exact_decimals() {
    let dir = tempfile::tempdir().unwrap();
    let t = read(&money_csv(dir.path()));
    let types: Vec<&str> = t.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(
        types,
        ["Int64", "Decimal128(2, 1)", "Decimal128(22, 2)", "Float64"],
        "an exponent stays a float"
    );
    assert_eq!(t.get(0, 2), Some(&CellValue::String(BIG.into())));
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(0.1)));
}

#[test]
fn every_digit_survives_a_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let t = read(&money_csv(dir.path()));
    for ext in ["csv", "parquet", "arrow", "avro", "orc", "json"] {
        let out = dir.path().join(format!("out.{ext}"));
        save(&t, &out);
        let back = read(&out);
        let big = col(&back, "big");
        assert_eq!(back.get(0, big).unwrap().to_string(), BIG, "{ext}");
        let amount = col(&back, "amount");
        assert_eq!(back.get(1, amount).unwrap().to_string(), "0.2", "{ext}");
    }
    // Parquet and Avro hold a real decimal, not a DOUBLE.
    for ext in ["parquet", "avro"] {
        let back = read(&dir.path().join(format!("out.{ext}")));
        assert_eq!(
            back.columns[col(&back, "big")].data_type,
            "Decimal128(22, 2)",
            "{ext}"
        );
    }
}

#[test]
fn european_numbers_keep_their_digits() {
    let dir = tempfile::tempdir().unwrap();
    let t = read(&write(
        dir.path(),
        "de.csv",
        "name;amount\na;12.345.678.901.234.567.890,12\nb;0,1\n",
    ));
    let amount = col(&t, "amount");
    assert_eq!(t.columns[amount].data_type, "Decimal128(22, 2)");
    assert_eq!(t.get(0, amount).unwrap().to_string(), BIG);
}

#[test]
fn sql_sums_a_text_files_decimals_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let t = read(&money_csv(dir.path()));
    let out = octa::sql::run_query(
        &t,
        "SELECT sum(amount) = 0.6 AS eq, sum(big) AS b FROM data",
    )
    .unwrap()
    .table;
    assert_eq!(out.get(0, 0), Some(&CellValue::Bool(true)));
    assert_eq!(
        out.get(0, 1).unwrap().to_string(),
        "12345678901234567893.62"
    );
}

/// A float column (a Parquet DOUBLE, an Excel column) sums by the digits it
/// shows in every analysis Octa builds the SQL for.
#[test]
fn a_float_column_sums_by_the_digits_it_shows() {
    let t = DataTable {
        columns: vec![
            ColumnInfo {
                name: "g".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "x".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: [0.1, 0.2, 0.3]
            .iter()
            .map(|v| vec![CellValue::String("a".into()), CellValue::Float(*v)])
            .collect(),
        ..DataTable::empty()
    };
    let out = octa::sql::run_query(&t, "SELECT sum(x) = 0.6 FROM data")
        .unwrap()
        .table;
    assert_eq!(out.get(0, 0), Some(&CellValue::Bool(true)));

    let summary = octa::data::summary::build_summary_table(
        &t,
        &[
            octa::data::summary::SummaryStat::ColumnName,
            octa::data::summary::SummaryStat::Type,
            octa::data::summary::SummaryStat::Sum,
        ],
    )
    .unwrap();
    let sum = col(&summary, "sum");
    assert_eq!(summary.get(1, sum), Some(&CellValue::Float(0.6)));
    let ty = col(&summary, "type");
    assert_eq!(
        summary.get(1, ty).unwrap().to_string(),
        "DOUBLE",
        "it is still a float"
    );

    let stats = octa::ui::status_bar::compute_selection_stats(&t, (0..3).map(|r| (r, 1)));
    assert_eq!(stats.sum, 0.6);
    assert_eq!(stats.avg, 0.2);
}

#[test]
fn a_running_total_ends_on_the_exact_sum() {
    let dir = tempfile::tempdir().unwrap();
    let t = read(&money_csv(dir.path()));
    let spec = CalcSpec {
        kind: CalcKind::RunningTotal,
        value: Some(col(&t, "amount")),
        order: None,
        descending: false,
        groups: vec![],
        extras: Default::default(),
    };
    let totals = compute(&t, &spec).unwrap();
    assert_eq!(totals[2], CellValue::Float(0.6));
    let diff = compute(
        &t,
        &CalcSpec {
            kind: CalcKind::DiffPrev,
            ..spec
        },
    )
    .unwrap();
    assert_eq!(
        diff[2],
        CellValue::Float(0.1),
        "0.3 - 0.2, not 0.09999999999999998"
    );
}

#[test]
fn a_cohort_sum_adds_cents_to_cents() {
    let t = DataTable {
        columns: ["who", "when", "amount"]
            .iter()
            .zip(["Utf8", "Date32", "Float64"])
            .map(|(n, ty)| ColumnInfo {
                name: (*n).into(),
                data_type: ty.into(),
            })
            .collect(),
        rows: [0.1, 0.2, 0.3]
            .iter()
            .map(|v| {
                vec![
                    CellValue::String("ann".into()),
                    CellValue::Date("2026-01-05".into()),
                    CellValue::Float(*v),
                ]
            })
            .collect(),
        ..DataTable::empty()
    };
    let spec = CohortSpec {
        who: 0,
        when: 1,
        period: Period::Month,
        show: Show::Sum,
        sum_col: Some(2),
    };
    let wide = octa::data::cohort::wide(&collect(&t, &spec).unwrap());
    let first = wide
        .columns
        .iter()
        .position(|c| c.name.ends_with("_0"))
        .unwrap();
    assert_eq!(wide.get(0, first), Some(&CellValue::Float(0.6)));
}

#[test]
fn large_file_mode_reads_decimals_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = money_csv(dir.path());
    let large = octa::formats::large::open(&path).unwrap();
    let types: Vec<&str> = large
        .columns()
        .iter()
        .map(|c| c.data_type.as_str())
        .collect();
    assert_eq!(types[1], "Decimal128(2, 1)");
    assert_eq!(types[2], "Decimal128(22, 2)");
    assert_eq!(types[3], "Float64", "an exponent stays a float");
    assert_eq!(large.row_count(), 3);
    let page = large.page(0, 10, None, None).unwrap();
    assert_eq!(page.get(0, 2).unwrap().to_string(), BIG);
}

#[test]
fn an_edit_keeps_the_digits_typed() {
    let typed = CellValue::parse_like(&CellValue::Float(1.0), BIG);
    assert_eq!(typed, CellValue::String(BIG.into()));
    assert_eq!(
        CellValue::parse_like(&CellValue::Float(1.0), "0.1"),
        CellValue::Float(0.1)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Float(1.0), "1e3"),
        CellValue::Float(1000.0)
    );
}

#[test]
fn json_numbers_keep_every_digit_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(
        dir.path(),
        "big.json",
        &format!(r#"[{{"id": 1, "amount": {BIG}}}, {{"id": 2, "amount": 0.5}}]"#),
    );
    let table = read(&src);
    let amount = col(&table, "amount");
    assert_eq!(table.get(0, amount).unwrap().to_string(), BIG);
    assert_eq!(table.columns[amount].data_type, "Decimal128(22, 2)");

    for name in ["out.json", "out.jsonl"] {
        let out = dir.path().join(name);
        save(&table, &out);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains(BIG), "{name}: {text}");
        assert!(
            !text.contains(&format!("\"{BIG}\"")),
            "{name} wrote text: {text}"
        );
        let back = read(&out);
        assert_eq!(back.get(0, col(&back, "amount")).unwrap().to_string(), BIG);
    }
}

/// A number with more digits than a float holds is kept as its digits, and is
/// still a number to Octa's analyses and to a SQL INSERT.
#[test]
fn long_numbers_kept_as_digits_are_still_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "p.csv", &format!("price\n{BIG}\n1.5\n"));
    let table = read(&src);
    assert_eq!(table.get(0, 0), Some(&CellValue::String(BIG.into())));
    assert_eq!(table.get(1, 0), Some(&CellValue::Float(1.5)));

    use octa::data::value_frequency::{BinningMode, compute_value_frequency};
    let bins = compute_value_frequency(&table, 0, None, BinningMode::Custom(1)).unwrap();
    assert_eq!(bins.rows.len(), 1, "both in one bin, none left as raw text");
    assert_eq!(bins.rows[0].count, 2);

    let block = octa::data::copy_formats::Block {
        table: &table,
        cols: vec![0],
        rows: vec![0],
        only: None,
    };
    let insert = octa::data::copy_formats::to_insert(&block, "t");
    assert!(insert.contains(&format!("VALUES ({BIG});")), "{insert}");
}

/// Octa types each column, so in a number column `1` and `1.0` (and `1.50`
/// and `1.5`) are the same value to every duplicate check. In a text column
/// they are different text.
#[test]
fn duplicates_compare_numbers_by_value() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(
        dir.path(),
        "n.csv",
        "n,code\n1,1\n1.0,1.0\n1.50,x\n1.5,y\n2,z\n",
    );
    let table = read(&src);
    assert!(table.columns[0].data_type.starts_with("Decimal128"));
    assert_eq!(table.columns[1].data_type, "Utf8");

    assert_eq!(
        octa::data::duplicates::find_duplicate_rows(&table, &[0]),
        vec![0, 1, 2, 3]
    );
    use octa::data::dedupe::{KeepWhich, dedupe_rows};
    assert_eq!(dedupe_rows(&table, &[0], KeepWhich::First).row_count(), 3);
    assert!(
        octa::data::duplicates::find_duplicate_rows(&table, &[1]).is_empty(),
        "text stays text"
    );
    let unique = octa::data::unique_columns::find_unique_columns(&table, 1);
    assert_eq!(unique.single[0].distinct_count, 3, "1, 1.5 and 2");
}
