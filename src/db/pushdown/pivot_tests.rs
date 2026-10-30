use super::super::join_keys::tests::text_table;
use super::super::test_support::{DuckConn, source};
use super::*;
use crate::data::pivot::pivot_sql;

/// DuckDB's own answers for this table (checked while planning):
/// columns `g, 10, 9, B, a, b_1`; Count 0 / Sum NULL where empty; the
/// NULL `k` row (x, NULL, 4) is in no column; (y, a, NULL) counts 0.
fn table() -> DataTable {
    let mut t = text_table(
        &["g", "k", "v"],
        &[
            &["x", "b", "1"],
            &["x", "a", "2"],
            &["y", "B", "3"],
            &["x", "NULL", "4"],
            &["y", "a", "NULL"],
            &["z", "10", "5"],
            &["z", "9", "6"],
        ],
    );
    t.columns[2].data_type = "Int64".into();
    for r in &mut t.rows {
        if let CellValue::String(s) = &r[2] {
            r[2] = CellValue::Int(s.parse().unwrap());
        }
    }
    t
}

fn spec(agg: PivotAgg) -> PivotSpec {
    PivotSpec {
        columns: vec!["g".into(), "k".into(), "v".into()],
        on: "k".into(),
        agg,
        value: "v".into(),
        group: vec!["g".into()],
    }
}

/// Cells as numbers where they are numbers, so DuckDB's int128 and the
/// server's double compare equal; NULL stays NULL.
fn norm(t: &DataTable) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = t
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| match super::super::cell_f64(c) {
                    Some(x) => format!("{x}"),
                    None => c.to_string(),
                })
                .collect()
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn the_server_pivot_is_duckdbs_pivot() {
    for agg in [PivotAgg::Count, PivotAgg::Sum, PivotAgg::Max] {
        let t = table();
        let want = crate::sql::run_query(&t, &pivot_sql("k", agg, "v", &["g".into()], &t.columns))
            .unwrap()
            .table;
        for chunk in [chunk_size(1), 2] {
            let mut c = DuckConn::new(table());
            let (got, more) = run(
                &mut c,
                &source(),
                &spec(agg),
                chunk,
                1_000,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(!more);
            let names =
                |t: &DataTable| t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
            assert_eq!(names(&got), names(&want), "{agg:?} chunk {chunk}");
            assert_eq!(norm(&got), norm(&want), "{agg:?} chunk {chunk}");
        }
    }
}

#[test]
fn no_group_chosen_groups_by_every_other_column() {
    let mut s = spec(PivotAgg::Sum);
    s.group.clear();
    let mut c = DuckConn::new(table());
    let (got, _) = run(&mut c, &source(), &s, 100, 1_000, &AtomicBool::new(false)).unwrap();
    assert_eq!(got.columns[0].name, "g");
    assert_eq!(got.row_count(), 3);
}

#[test]
fn more_groups_than_the_cap_says_so() {
    let mut c = DuckConn::new(table());
    let (got, more) = run(
        &mut c,
        &source(),
        &spec(PivotAgg::Count),
        100,
        2,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(more);
    assert_eq!(got.row_count(), 2);
}

#[test]
fn values_are_compared_exactly_on_mysql() {
    let sql = cell_expr(
        DbEngine::MySql,
        PivotAgg::Sum,
        "CAST(`k` AS CHAR)",
        "b",
        "`v`",
    );
    assert_eq!(
        sql,
        "SUM(CASE WHEN CAST(CAST(`k` AS CHAR) AS BINARY) = CAST('b' AS BINARY) THEN CAST(`v` AS DOUBLE) END)"
    );
}

#[test]
fn duckdb_names_case_twins_apart() {
    let labels: Vec<String> = ["10", "9", "B", "a", "b", ""]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let names = column_names(&["g".into()], &labels);
    assert_eq!(&names[..5], &["10", "9", "B", "a", "b_1"]);
    assert_eq!(names[5], crate::i18n::t("dialog.cf_empty"));
}
