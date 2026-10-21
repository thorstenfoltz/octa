use super::*;
use crate::data::{CellValue, ColumnInfo};
use crate::db::pushdown::test_support::{DuckConn, source};

/// The server holds 100 rows, the tab 10. The counted columns must
/// describe the 100; the pattern columns are named as local.
#[test]
fn counts_come_from_the_server_and_patterns_are_named_local() {
    let mut full = DataTable::empty();
    full.columns = vec![ColumnInfo {
        name: "x".into(),
        data_type: "Int64".into(),
    }];
    full.rows = (0..100)
        .map(|i| {
            vec![if i < 25 {
                CellValue::Null
            } else {
                CellValue::Int(i % 10)
            }]
        })
        .collect();
    full.rows[99] = vec![CellValue::Int(10_000)];
    let mut loaded = full.clone();
    loaded.rows.truncate(10);

    let mut c = DuckConn::new(full);
    let (report, local) = run(
        &mut c,
        &source(),
        &loaded,
        100,
        Default::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let ids = crate::data::quality::quality_column_ids();
    let col = |id: &str| ids.iter().position(|x| *x == id).unwrap();
    let cell = |id: &str| report.table.get(0, col(id)).unwrap().to_string();
    assert_eq!(cell("null_percentage"), "25");
    // 11 distinct values (0..=9 and 10000) over 75 present cells.
    assert!(cell("distinct_ratio").starts_with("0.1466"));
    assert_eq!(cell("outlier_count"), "1");
    for id in [
        "type_consistency",
        "pii_flag",
        "pii_kind",
        "benford_verdict",
        "calendar_verdict",
        "shape_verdict",
    ] {
        assert!(
            local.by_design.iter().any(|l| l == id),
            "{id} missing from {local:?}"
        );
    }
    // Those plus one entry per section tab, by the title on the tab.
    assert_eq!(
        local.by_design.len(),
        PATTERN_COLUMNS.len() + report.sections.len()
    );
    for sec in &report.sections {
        let title = crate::i18n::t(&sec.title_key);
        assert!(local.by_design.contains(&title), "{title}");
    }
    assert!(local.engine.is_empty());
    // Score from the server counts over 100 rows, with the loaded rows'
    // type consistency.
    let tc = crate::db::pushdown::cell_f64(report.table.get(0, col("type_consistency")).unwrap())
        .unwrap();
    let want = crate::data::quality::column_score(0.25, 11.0 / 75.0, tc, 1, 100);
    assert_eq!(
        crate::db::pushdown::cell_f64(report.table.get(0, col("score")).unwrap()),
        Some(want)
    );
}

/// MySQL has no percentile: the loaded outlier count stays, named local,
/// and is penalised against the loaded rows, not the server's total.
#[test]
fn without_quartiles_the_loaded_outliers_are_scored_over_loaded_rows() {
    let mut full = DataTable::empty();
    full.columns = vec![ColumnInfo {
        name: "x".into(),
        data_type: "Int64".into(),
    }];
    full.rows = (0..100).map(|i| vec![CellValue::Int(i % 10)]).collect();
    let mut loaded = DataTable::empty();
    loaded.columns = full.columns.clone();
    loaded.rows = (0..10)
        .map(|i| vec![CellValue::Int(if i == 9 { 10_000 } else { i % 3 })])
        .collect();

    let mut src = source();
    src.conn.engine = crate::db::DbEngine::MySql;
    let mut c = DuckConn::new(full);
    let (report, local) = run(
        &mut c,
        &src,
        &loaded,
        100,
        Default::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let ids = crate::data::quality::quality_column_ids();
    let cell = |id: &str| {
        let i = ids.iter().position(|x| *x == id).unwrap();
        report.table.get(0, i).unwrap().to_string()
    };
    assert_eq!(local.engine, vec!["outlier_count".to_string()]);
    let outliers: usize = cell("outlier_count").parse().unwrap();
    assert!(outliers >= 1);
    let tc: f64 = cell("type_consistency").parse().unwrap();
    let want = crate::data::quality::column_score(0.0, 0.1, tc, outliers, 10);
    assert_eq!(cell("score"), format!("{want}"));
}

/// A refused outlier query blanks only the outlier count; the null
/// percentage and distinct ratio still come from the server.
#[test]
fn a_refused_outlier_query_keeps_the_rest() {
    let mut full = DataTable::empty();
    full.columns = vec![ColumnInfo {
        name: "x".into(),
        data_type: "Int64".into(),
    }];
    full.rows = (0..20)
        .map(|i| {
            vec![if i < 5 {
                CellValue::Null
            } else {
                CellValue::Int(i)
            }]
        })
        .collect();
    let mut c = DuckConn::new(full.clone());
    c.fail_on = vec!["PERCENTILE_CONT"];
    let (report, local) = run(
        &mut c,
        &source(),
        &full,
        20,
        Default::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let ids = crate::data::quality::quality_column_ids();
    let cell = |id: &str| {
        let i = ids.iter().position(|x| *x == id).unwrap();
        report.table.get(0, i).unwrap().to_string()
    };
    assert_eq!(cell("null_percentage"), "25");
    assert_eq!(cell("distinct_ratio"), "1");
    // The server refused: not the engine's gap, so its own list.
    assert!(local.engine.is_empty());
    assert_eq!(local.failed, vec!["outlier_count".to_string()]);
}
