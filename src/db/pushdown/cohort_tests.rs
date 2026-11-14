use super::*;
use crate::db::DbEngine;

fn spec(sum: Option<&str>, period: Period, show: Show) -> ServerCohort<'_> {
    ServerCohort {
        who: "who",
        when: "when",
        sum,
        sum_type: "Float64",
        period,
        show,
    }
}

#[test]
fn postgres_sql_buckets_and_counts_distinct_people() {
    let [main, sizes, skipped] = sql(
        DbEngine::Postgres,
        "\"t\"",
        &spec(None, Period::Month, Show::Count),
    );
    assert!(main.contains("DATE_TRUNC('month', \"when\")"), "{main}");
    assert!(main.contains("COUNT(DISTINCT x.octa_w)"), "{main}");
    assert!(sizes.contains("GROUP BY octa_c"), "{sizes}");
    assert!(
        skipped.contains("\"who\" IS NULL OR \"when\" IS NULL"),
        "{skipped}"
    );
}

#[test]
fn a_sum_adds_the_column_as_a_float() {
    let [main, ..] = sql(
        DbEngine::Mssql,
        "[t]",
        &spec(Some("amount"), Period::Week, Show::Sum),
    );
    assert!(main.contains("SUM(x.octa_v)"), "{main}");
    assert!(main.contains("[amount]"), "{main}");
}

#[test]
fn a_decimal_column_sums_as_it_is() {
    let decimal = ServerCohort {
        sum_type: "Decimal128(10, 2)",
        ..spec(Some("amount"), Period::Week, Show::Sum)
    };
    let [main, ..] = sql(DbEngine::Mssql, "[t]", &decimal);
    assert!(main.contains(", [amount] AS octa_v"), "{main}");
}

#[test]
fn a_sum_without_a_column_says_so() {
    use super::super::test_support::{DuckConn, source};
    let mut conn = DuckConn::new(crate::data::DataTable::empty());
    let err = run(
        &mut conn,
        &source(),
        &spec(None, Period::Month, Show::Sum),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(format!("{err:#}"), crate::i18n::t("cohort.err_no_sum"));
    assert!(conn.log.is_empty(), "nothing was sent");
}

/// The SQL, run by DuckDB, gives the table the local engine builds from the
/// same rows: every Show, by month and by week.
#[test]
fn duckdb_agrees_with_the_local_engine() {
    use super::super::test_support::{DuckConn, source};
    use crate::data::cohort::{CohortSpec, collect, wide};
    use crate::data::{CellValue, ColumnInfo, DataTable};

    let mut t = DataTable::empty();
    t.columns = [("who", "Utf8"), ("when", "Date32"), ("amount", "Float64")]
        .iter()
        .map(|(n, d)| ColumnInfo {
            name: (*n).into(),
            data_type: (*d).into(),
        })
        .collect();
    t.rows = [
        ("a", "2024-01-05", 10.0),
        ("a", "2024-02-10", 20.0),
        ("b", "2024-01-20", 30.0),
        ("b", "2024-03-01", 40.0),
        ("c", "2024-02-02", 50.0),
        ("c", "2024-02-20", 5.0),
    ]
    .iter()
    .map(|(w, d, a)| {
        vec![
            CellValue::String((*w).into()),
            CellValue::Date((*d).into()),
            CellValue::Float(*a),
        ]
    })
    .collect();
    let shape = |w: &DataTable| {
        let names: Vec<String> = w.columns.iter().map(|c| c.name.clone()).collect();
        (names, w.rows.clone())
    };
    for period in [Period::Month, Period::Week] {
        for show in Show::ALL {
            let local = collect(
                &t,
                &CohortSpec {
                    who: 0,
                    when: 1,
                    period,
                    show,
                    sum_col: Some(2),
                },
            )
            .unwrap();
            let mut conn = DuckConn::new(t.clone());
            let server = run(
                &mut conn,
                &source(),
                &spec(Some("amount"), period, show),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(
                shape(&wide(&server)),
                shape(&wide(&local)),
                "{period:?} {show:?}"
            );
        }
    }
}
