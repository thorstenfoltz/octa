use super::*;

/// The user's request: the bottom line answers in every state, not only while
/// a tab refreshes.
#[test]
fn the_pill_says_on_paused_off_or_why_it_cannot() {
    use octa::i18n::t;
    use octa::ui::status_bar::LiveState;
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );

    let (text, hint, state) = app.live_pill(0);
    assert_eq!(state, LiveState::Unavailable, "a tab with no server");
    assert_eq!(text, t("live.auto_refresh_off"));
    assert_eq!(hint, t("live.auto_refresh_needs_server"), "and why");

    app.tabs[0].db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "c".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
        identity: None,
    });
    let (text, hint, state) = app.live_pill(0);
    assert_eq!(state, LiveState::Off);
    assert_eq!(text, t("live.auto_refresh_off"));
    assert_eq!(hint, t("live.auto_refresh_off_hint"));

    app.tabs[0].auto_refresh = Some(AutoRefresh {
        every: Duration::from_secs(30),
        next_at: Instant::now(),
        before: None,
        return_to: None,
    });
    let (text, hint, state) = app.live_pill(0);
    assert_eq!(state, LiveState::On);
    assert!(text.contains("30"), "{text}");
    assert_eq!(hint, t("live.auto_refresh_pill_hint"));

    app.tabs[0].table.structural_changes = true;
    let (text, _, state) = app.live_pill(0);
    assert_eq!(state, LiveState::Paused);
    assert_eq!(text, t("live.auto_refresh_paused"));
    app.tabs[0].table.structural_changes = false;
    app.tabs[0].auto_refresh = None;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("log.csv");
    std::fs::write(&file, "a\n1\n").unwrap();
    app.tabs[0].follow = Some(start_follow(file.to_str().unwrap()).unwrap());
    let (text, _, state) = app.live_pill(0);
    assert_eq!(state, LiveState::On);
    assert_eq!(text, t("live.following"));
}

#[test]
fn an_interval_below_the_minimum_is_refused() {
    assert_eq!(check_interval(4, false, 5, 60), Err(5));
    assert_eq!(check_interval(5, false, 5, 60), Ok(5));
    assert_eq!(check_interval(30, true, 5, 60), Err(60));
    assert_eq!(check_interval(90, true, 5, 60), Ok(90));
}

#[test]
fn only_the_metered_warehouses_count_as_billed() {
    use octa::db::DbEngine;
    assert!(
        is_billed(DbEngine::BigQuery)
            && is_billed(DbEngine::Athena)
            && is_billed(DbEngine::Snowflake)
    );
    assert!(!is_billed(DbEngine::Postgres) && !is_billed(DbEngine::Databricks));
}

#[test]
fn appended_rows_land_under_the_columns_they_name() {
    use octa::data::{CellValue, ColumnInfo, DataTable};
    let mut tab_table = DataTable::empty();
    tab_table.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "msg".into(),
            data_type: "Utf8".into(),
        },
    ];
    let mut new = DataTable::empty();
    new.columns = vec![
        ColumnInfo {
            name: "msg".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "id".into(),
            data_type: "Utf8".into(),
        },
    ];
    new.rows = vec![vec![
        CellValue::String("hi".into()),
        CellValue::String("7".into()),
    ]];
    let rows = rows_for(&tab_table, &new).unwrap();
    assert_eq!(
        rows,
        vec![vec![CellValue::Int(7), CellValue::String("hi".into())]]
    );
    let mut other = new.clone();
    other.columns[0].name = "x".into();
    other.columns[1].name = "y".into();
    assert!(
        rows_for(&tab_table, &other).is_none(),
        "no shared name means reload"
    );
}
