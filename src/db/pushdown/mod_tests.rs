use super::test_support::{DuckConn, source};
use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "a".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("x".into())],
        vec![CellValue::Int(2), CellValue::Null],
        vec![CellValue::Null, CellValue::String("y".into())],
    ];
    t
}

#[test]
fn a_filtered_source_reads_only_its_rows() {
    let mut src = source();
    assert_eq!(src.from_sql(), "\"data\"");
    src.filter = Some("\"k\" = 'a'".into());
    assert_eq!(
        src.from_sql(),
        "(SELECT * FROM \"data\" WHERE \"k\" = 'a') AS octa_view"
    );
    assert_eq!(src.table_sql(), "\"data\"");
    let mut c = DuckConn::new(super::join_keys::tests::text_table(
        &["k"],
        &[&["a"], &["a"], &["b"]],
    ));
    assert_eq!(
        count_rows(&mut c, &src, &AtomicBool::new(false)).unwrap(),
        2
    );
}

#[test]
fn a_filtered_source_has_no_catalog_estimate() {
    let mut src = source();
    assert!(super::row_estimate::estimate_sql(&src).is_some());
    src.filter = Some("1 = 1".into());
    assert!(super::row_estimate::estimate_sql(&src).is_none());
}

#[test]
fn counts_the_whole_source() {
    let mut c = DuckConn::new(table());
    assert_eq!(
        count_rows(&mut c, &source(), &AtomicBool::new(false)).unwrap(),
        3
    );
}

#[test]
fn per_column_aggregates_slices_one_row_per_column() {
    let mut c = DuckConn::new(table());
    let per_col = vec![
        vec!["COUNT(\"a\")".to_string(), "MAX(\"a\")".to_string()],
        vec!["COUNT(\"b\")".to_string()],
    ];
    let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
    assert_eq!(cell_i64(&got[0].as_ref().unwrap()[0]), Some(2));
    assert_eq!(cell_i64(&got[0].as_ref().unwrap()[1]), Some(2));
    assert_eq!(cell_i64(&got[1].as_ref().unwrap()[0]), Some(2));
}

/// One column the server refuses must not blank the others.
#[test]
fn a_refused_column_only_blanks_itself() {
    let mut c = DuckConn::new(table());
    let per_col = vec![
        vec!["COUNT(\"a\")".to_string()],
        vec!["no_such_function(\"b\")".to_string()],
    ];
    let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
    assert_eq!(cell_i64(&got[0].as_ref().unwrap()[0]), Some(2));
    assert!(got[1].is_none());
}

/// Wider than one statement: the chunks must slice back to the same
/// per-column cells a single statement would give.
#[test]
fn a_table_wider_than_one_statement_is_chunked() {
    let n = MAX_SELECT_ITEMS / 2 + 10;
    let mut t = DataTable::empty();
    t.columns = (0..n)
        .map(|i| ColumnInfo {
            name: format!("c{i}"),
            data_type: "Int64".into(),
        })
        .collect();
    t.rows = (0..3)
        .map(|r| {
            (0..n)
                .map(|i| {
                    if (r + i) % 3 == 0 {
                        CellValue::Null
                    } else {
                        CellValue::Int((i * 10 + r) as i64)
                    }
                })
                .collect()
        })
        .collect();
    let per_col: Vec<Vec<String>> = (0..n)
        .map(|i| vec![format!("COUNT(\"c{i}\")"), format!("MAX(\"c{i}\")")])
        .collect();
    let mut c = DuckConn::new(t);
    let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.log.len(), 2, "two statements, not one per column");
    assert!(
        c.log
            .iter()
            .all(|q| q.matches("COUNT(").count() <= MAX_SELECT_ITEMS)
    );
    for (i, cells) in got.iter().enumerate() {
        let cells = cells.as_ref().unwrap();
        let present: Vec<i64> = (0..3)
            .filter(|r| (r + i) % 3 != 0)
            .map(|r| (i * 10 + r) as i64)
            .collect();
        assert_eq!(cell_i64(&cells[0]), Some(present.len() as i64), "c{i}");
        assert_eq!(cell_i64(&cells[1]), present.iter().max().copied(), "c{i}");
    }
}

#[test]
fn width_chunks_keep_each_run_under_the_cap() {
    assert!(width_chunks(&[]).is_empty());
    assert_eq!(width_chunks(&[1, 2]), vec![0..2]);
    assert_eq!(
        width_chunks(&[MAX_SELECT_ITEMS - 1, 2, MAX_SELECT_ITEMS + 5]),
        vec![0..1, 1..2, 2..3]
    );
}

#[test]
fn a_cancel_stops_before_the_next_query() {
    let mut c = DuckConn::new(table());
    assert!(count_rows(&mut c, &source(), &AtomicBool::new(true)).is_err());
}

/// The docs page's engine table is generated from the dialect, so it
/// cannot drift from the code. On failure, paste the printed table
/// between the markers in docs/usage/analyses-on-live-databases.md.
#[test]
fn docs_support_table_matches_the_dialect() {
    let mut table = String::from(
        "| Engine | Median, quartiles, IQR, outliers | Correlation | Join diagnostics: spaces and punctuation | Join diagnostics: leading zeros | Fast random sample |\n|---|---|---|---|---|---|\n",
    );
    for &e in DbEngine::ALL {
        let quart = match (dialect::quartiles_sql(e, "t", "x"), e) {
            (None, _) => "loaded rows",
            (Some(_), DbEngine::Trino | DbEngine::Athena) => "server (approximate)",
            (Some(_), _) => "server",
        };
        let corr = if dialect::pearson_select(e, "a", "b").1 == 1 {
            "server (CORR)"
        } else {
            "server (sums)"
        };
        let on = |b: bool| if b { "server" } else { "loaded rows" };
        let regex = on(dialect::collapse_ws(e, "x").is_some());
        let zeros = on(dialect::strip_leading_zeros(e, "x").is_some());
        let fast = if sample::fast_available(e) {
            "block sampling"
        } else {
            "exact only"
        };
        table.push_str(&format!(
            "| {} | {quart} | {corr} | {regex} | {zeros} | {fast} |\n",
            e.label()
        ));
    }
    let doc = include_str!("../../../docs/usage/analyses-on-live-databases.md");
    let start = "<!-- support-table:start -->\n";
    let inner = doc
        .split(start)
        .nth(1)
        .and_then(|s| s.split("<!-- support-table:end -->").next());
    // MegaLinter's table formatter pads the columns, so compare the cells
    // and not the spacing (a separator cell reduces to "").
    let cells = |t: &str| -> Vec<Vec<String>> {
        t.lines()
            .map(|l| {
                l.split('|')
                    .map(|c| c.trim().trim_matches('-').to_string())
                    .collect()
            })
            .collect()
    };
    assert_eq!(inner.map(cells), Some(cells(&table)), "\n{table}");
}
