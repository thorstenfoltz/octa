use super::*;
use crate::data::calculated::CalcKind;
use crate::db::DbEngine;

fn c(kind: CalcKind, order: Option<&str>, groups: &[&str]) -> ServerCalc {
    ServerCalc {
        name: "x".into(),
        kind,
        value: Some("amount".into()),
        value_type: "Float64".into(),
        order: order.map(str::to_string),
        descending: true,
        groups: groups.iter().map(|g| g.to_string()).collect(),
        extras: Default::default(),
    }
}

#[test]
fn rank_orders_by_the_value_with_empties_last() {
    let sql = calc_sql(DbEngine::Postgres, &c(CalcKind::Rank, None, &["region"])).unwrap();
    assert_eq!(
        sql,
        r#"RANK() OVER (PARTITION BY "region" ORDER BY CASE WHEN "amount" IS NULL THEN 1 ELSE 0 END, "amount" DESC)"#
    );
}

#[test]
fn running_total_frames_from_the_first_row() {
    let sql = calc_sql(
        DbEngine::Postgres,
        &c(CalcKind::RunningTotal, Some("day"), &[]),
    )
    .unwrap();
    assert!(sql.starts_with("SUM("), "{sql}");
    assert!(sql.contains(r#"ORDER BY CASE WHEN "day" IS NULL THEN 1 ELSE 0 END, "day" DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"#), "{sql}");
}

#[test]
fn percent_of_total_never_divides_by_zero() {
    let sql = calc_sql(DbEngine::Mssql, &c(CalcKind::PercentOfTotal, None, &[])).unwrap();
    assert!(sql.contains("NULLIF(SUM("), "{sql}");
    assert!(sql.contains("OVER ()"), "{sql}");
}

#[test]
fn the_order_following_kinds_need_an_order_column_on_a_server() {
    assert!(calc_sql(DbEngine::Postgres, &c(CalcKind::RowNumber, None, &[])).is_none());
    assert!(calc_sql(DbEngine::Postgres, &c(CalcKind::DiffPrev, None, &[])).is_none());
    assert!(calc_sql(DbEngine::Postgres, &c(CalcKind::Rank, None, &[])).is_some());
}

#[test]
fn clickhouse_has_no_lag() {
    assert!(
        calc_sql(
            DbEngine::ClickHouse,
            &c(CalcKind::DiffPrev, Some("day"), &[])
        )
        .is_none()
    );
    assert!(calc_sql(DbEngine::ClickHouse, &c(CalcKind::Rank, None, &[])).is_some());
}

#[test]
fn a_calculated_column_is_selected_beside_the_table() {
    let cols = vec![crate::db::pushdown::derived::ServerColumn::Calc(c(
        CalcKind::Rank,
        None,
        &[],
    ))];
    let sql =
        crate::db::pushdown::derived::select_with(DbEngine::Postgres, r#""public"."t""#, &cols);
    assert!(
        sql.starts_with(r#"SELECT octa_t.*, RANK() OVER ("#),
        "{sql}"
    );
    assert!(
        sql.ends_with(r#" AS "x" FROM "public"."t" octa_t"#),
        "{sql}"
    );
}

/// The SQL, run by DuckDB, gives what the local engine computes: every kind
/// over rows with a NULL, a tie and a zero, once ungrouped and once grouped.
#[test]
fn the_server_sql_agrees_with_the_local_engine() {
    use super::super::derived::{ServerColumn, select_with};
    use super::super::test_support::DuckConn;
    use crate::data::calculated::{CalcSpec, compute};
    use crate::data::{CellValue, ColumnInfo, DataTable};
    use crate::db::DbConnector;

    let mut t = DataTable::empty();
    t.columns = [
        ("id", "Int64"),
        ("region", "Utf8"),
        ("day", "Int64"),
        ("amount", "Float64"),
    ]
    .iter()
    .map(|(n, d)| ColumnInfo {
        name: (*n).into(),
        data_type: (*d).into(),
    })
    .collect();
    let rows = [
        ("n", 10.0),
        ("n", 30.0),
        ("n", 30.0),
        ("n", 0.0),
        ("s", 5.0),
        ("s", 20.0),
        ("s", 40.0),
    ];
    t.rows = rows
        .iter()
        .enumerate()
        .map(|(i, (g, a))| {
            let amount = if i == 3 {
                CellValue::Null
            } else {
                CellValue::Float(*a)
            };
            let amount = if i == 5 {
                CellValue::Float(0.0)
            } else {
                amount
            };
            vec![
                CellValue::Int(i as i64),
                CellValue::String((*g).into()),
                CellValue::Int(i as i64 + 1),
                amount,
            ]
        })
        .collect();
    let text = |v: &CellValue| match v {
        CellValue::Null => String::new(),
        v => v
            .to_string()
            .parse::<f64>()
            .map_or(v.to_string(), |f| format!("{f:.6}")),
    };
    // A visit number is never computed on a server.
    for kind in CalcKind::ALL
        .into_iter()
        .filter(|k| *k != CalcKind::Session)
    {
        for groups in [vec![], vec![1usize]] {
            let spec = CalcSpec {
                kind,
                value: Some(3),
                order: Some(2),
                descending: true,
                groups,
                extras: Default::default(),
            };
            let want = compute(&t, &spec).unwrap();
            let col = ServerColumn::Calc(ServerCalc::of(&spec, &t.columns, "x"));
            let sql = select_with(DbEngine::Postgres, "\"data\"", &[col]);
            let got = DuckConn::new(t.clone()).query(&sql).unwrap();
            let (idc, xc) = (0, got.columns.iter().position(|c| c.name == "x").unwrap());
            for r in 0..got.row_count() {
                let id = match got.get(r, idc).unwrap() {
                    CellValue::Int(i) => *i as usize,
                    v => panic!("{v:?}"),
                };
                assert_eq!(
                    text(got.get(r, xc).unwrap()),
                    text(&want[id]),
                    "{kind:?} groups {:?} id {id}\n{sql}",
                    spec.groups
                );
            }
        }
    }
}

/// A running total and a difference on a decimal column match the local
/// engine to the last digit: 0.6, not 0.6000000000000001, and 0.1, not
/// 0.09999999999999998.
#[test]
fn a_decimal_column_adds_and_subtracts_exactly() {
    use super::super::derived::{ServerColumn, select_with};
    use super::super::test_support::{DuckConn, cents};
    use crate::data::CellValue;
    use crate::data::calculated::CalcSpec;
    use crate::db::DbConnector;

    let t = cents();
    for (kind, want) in [(CalcKind::RunningTotal, "0.6"), (CalcKind::DiffPrev, "0.1")] {
        let spec = CalcSpec {
            kind,
            value: Some(4),
            order: Some(0),
            descending: false,
            groups: vec![],
            extras: Default::default(),
        };
        let col = ServerColumn::Calc(ServerCalc::of(&spec, &t.columns, "x"));
        let sql = select_with(DbEngine::Postgres, "\"data\"", &[col]);
        let got = DuckConn::new(t.clone()).query(&sql).unwrap();
        let x = got.columns.iter().position(|c| c.name == "x").unwrap();
        let last = (0..got.row_count())
            .find(|&r| got.get(r, 0) == Some(&CellValue::Int(3)))
            .unwrap();
        // The test connector hands DuckDB's DECIMAL back as its text.
        assert_eq!(got.get(last, x).unwrap().to_string(), want, "{kind:?}");
    }
}

#[test]
fn previous_and_next_are_lag_and_lead_and_a_session_stays_local() {
    let mut calc = c(CalcKind::Previous, Some("day"), &["region"]);
    calc.extras.offset = Some(3);
    let sql = calc_sql(DbEngine::Postgres, &calc).unwrap();
    assert!(
        sql.starts_with("LAG(\"amount\", 3) OVER (PARTITION BY"),
        "{sql}"
    );
    calc.kind = CalcKind::Next;
    assert!(
        calc_sql(DbEngine::MySql, &calc)
            .unwrap()
            .starts_with("LEAD(")
    );
    assert_eq!(calc_sql(DbEngine::ClickHouse, &calc), None);
    // No order column: a server has no row order of its own.
    assert_eq!(
        calc_sql(DbEngine::Postgres, &c(CalcKind::Previous, None, &[])),
        None
    );
    assert_eq!(
        calc_sql(DbEngine::Postgres, &c(CalcKind::Session, Some("day"), &[])),
        None
    );
}

#[test]
fn abc_carries_its_limits_into_the_sql() {
    let mut calc = c(CalcKind::Abc, None, &[]);
    calc.extras.abc_a = Some(70.0);
    let sql = calc_sql(DbEngine::Postgres, &calc).unwrap();
    assert!(
        sql.contains("< 70 THEN 'A'") && sql.contains("< 95 THEN 'B'"),
        "{sql}"
    );
}
