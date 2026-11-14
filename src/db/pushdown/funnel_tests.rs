use super::*;
use crate::db::DbEngine;

fn steps(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| (*s).to_string()).collect()
}

fn spec(steps: &[String], within_days: Option<u32>) -> ServerFunnel<'_> {
    ServerFunnel {
        who: "who",
        event: "event",
        when: "when",
        steps,
        within_days,
    }
}

#[test]
fn each_step_joins_the_one_before() {
    let s = steps(&["view", "it's paid"]);
    let [main, skipped] = sql(DbEngine::Postgres, "\"t\"", &spec(&s, Some(2)));
    assert!(main.contains("octa_e = 'view'"), "{main}");
    assert!(main.contains("octa_e = 'it''s paid'"), "{main}");
    assert!(main.contains("JOIN s0 ON x.octa_w = s0.octa_w"), "{main}");
    assert!(
        main.contains("x.octa_t >= s0.octa_t AND x.octa_t <= s0.octa_t0 + 172800"),
        "{main}"
    );
    assert!(
        main.contains("SELECT 1 AS octa_k, COUNT(*) AS octa_n FROM s1"),
        "{main}"
    );
    assert!(
        skipped.contains("\"who\" IS NULL OR \"when\" IS NULL"),
        "{skipped}"
    );
}

#[test]
fn a_bad_request_sends_nothing() {
    use super::super::test_support::{DuckConn, source};
    let mut conn = DuckConn::new(crate::data::DataTable::empty());
    let s = steps(&["view"]);
    let err = run(
        &mut conn,
        &source(),
        &spec(&s, None),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(format!("{err:#}"), crate::i18n::t("funnel.err_steps"));
    assert!(conn.log.is_empty(), "nothing was sent");
}

/// The SQL, run by DuckDB, counts what the local engine counts from the same
/// rows: any number of steps, with and without a window.
#[test]
fn duckdb_agrees_with_the_local_engine() {
    use super::super::test_support::{DuckConn, source};
    use crate::data::funnel::{FunnelSpec, collect};
    use crate::data::{CellValue, ColumnInfo, DataTable};

    let mut t = DataTable::empty();
    t.columns = [
        ("who", "Utf8"),
        ("event", "Utf8"),
        ("when", "Timestamp(Microsecond, None)"),
    ]
    .iter()
    .map(|(n, d)| ColumnInfo {
        name: (*n).into(),
        data_type: (*d).into(),
    })
    .collect();
    t.rows = [
        ("a", "view", "2024-01-01 10:00:00"),
        ("a", "cart", "2024-01-01 10:05:00"),
        ("a", "pay", "2024-01-03 09:00:00"),
        ("b", "view", "2024-01-01 11:00:00"),
        ("b", "pay", "2024-01-01 11:01:00"),
        ("b", "cart", "2024-01-01 11:02:00"),
        ("c", "view", "2024-01-03 08:00:00"),
        ("c", "view", "2024-01-04 08:00:00"),
        ("c", "cart", "2024-01-04 08:00:00"),
        ("d", "cart", "2024-01-01 12:00:00"),
        ("e", "view", "2024-01-05 00:00:00"),
        ("e", "cart", "2024-01-09 00:00:00"),
        ("e", "pay", "2024-01-09 00:00:01"),
    ]
    .iter()
    .map(|(w, e, d)| {
        vec![
            CellValue::String((*w).into()),
            CellValue::String((*e).into()),
            CellValue::DateTime((*d).into()),
        ]
    })
    .collect();
    for names in [
        &["view", "cart"][..],
        &["view", "cart", "pay"],
        &["pay", "cart", "view"],
    ] {
        for within_days in [None, Some(1), Some(4)] {
            let s = steps(names);
            let local = collect(
                &t,
                &FunnelSpec {
                    who: 0,
                    event: 1,
                    when: Some(2),
                    steps: s.clone(),
                    within_days,
                },
            )
            .unwrap();
            let mut conn = DuckConn::new(t.clone());
            let server = run(
                &mut conn,
                &source(),
                &spec(&s, within_days),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(server, local, "{names:?} {within_days:?}");
        }
    }
}
