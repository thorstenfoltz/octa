use super::*;
use octa::data::chart::{ChartConfig, ChartKind};
use octa::data::{CellValue, ColumnInfo, SearchMode};
use octa::db::{DEFAULT_QUERY_TIMEOUT_SECS, DbAuth, DbEngine};

use crate::app::state::DbOrigin;

fn conn() -> DbConnection {
    DbConnection {
        id: "c1".into(),
        name: "test".into(),
        engine: DbEngine::Postgres,
        host: String::new(),
        port: 5432,
        database: String::new(),
        username: String::new(),
        auth: DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        query_timeout_secs: DEFAULT_QUERY_TIMEOUT_SECS,
        ssh: None,
        tunnel_port: None,
    }
}

/// A partial database tab: `name` (text), `n` (Int64), one page loaded.
fn db_tab() -> TabState {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table.columns = vec![
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    tab.table.rows = vec![vec![CellValue::String("a".into()), CellValue::Int(1)]];
    tab.table.total_rows = Some(1);
    tab.db_origin = Some(DbOrigin {
        conn_id: "c1".into(),
        catalog: None,
        schema: "public".into(),
        table: "t".into(),
        identity: None,
    });
    tab
}

fn key(x: usize) -> ChartKey {
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(x),
        ..ChartConfig::default()
    };
    ChartKey::of(&cfg, ChartLimits::default())
}

fn server() -> ChartServer {
    chart_server_for(&db_tab(), true, &[conn()]).expect("a database chart")
}

#[test]
fn only_a_partial_database_tab_charts_on_the_server() {
    let conns = [conn()];
    assert!(chart_server_for(&db_tab(), true, &conns).is_some());
    assert!(
        chart_server_for(&db_tab(), false, &conns).is_none(),
        "setting off"
    );
    let mut file = db_tab();
    file.db_origin = None;
    assert!(
        chart_server_for(&file, true, &conns).is_none(),
        "a file tab"
    );
    let cs = server();
    assert_eq!(cs.reads, vec![ColumnRead::Other, ColumnRead::Number]);
    assert_eq!(cs.loaded, 1);
}

#[test]
fn the_first_chart_goes_out_at_once_and_a_change_waits_for_the_pause() {
    let mut cs = server();
    let t0 = Instant::now();
    assert!(matches!(
        chart_step(&mut cs, &key(1), t0),
        ChartStep::Query(_)
    ));
    cs.pending = Some(key(1));
    assert!(
        matches!(chart_step(&mut cs, &key(1), t0), ChartStep::Idle),
        "in flight"
    );
    cs.pending = None;
    cs.shown = Some((
        key(1),
        ServerChart::Drawn {
            chart: Err(ChartError::EmptyAfterFilter),
            total: 0,
        },
    ));
    assert!(
        matches!(chart_step(&mut cs, &key(1), t0), ChartStep::Idle),
        "answered"
    );
    assert!(matches!(
        chart_step(&mut cs, &key(0), t0),
        ChartStep::Settle(_)
    ));
    assert!(matches!(
        chart_step(&mut cs, &key(0), t0 + SETTLE / 2),
        ChartStep::Settle(_)
    ));
    assert!(matches!(
        chart_step(&mut cs, &key(0), t0 + SETTLE),
        ChartStep::Query(_)
    ));
}

#[test]
fn a_refusal_waits_for_try_again_and_loaded_rows_stop_asking() {
    let mut cs = server();
    let t0 = Instant::now();
    cs.shown = Some((
        key(0),
        ServerChart::Drawn {
            chart: Err(ChartError::EmptyAfterFilter),
            total: 42,
        },
    ));
    cs.error = Some((key(1), "denied".into()));
    assert!(matches!(chart_step(&mut cs, &key(1), t0), ChartStep::Idle));
    // No previous chart or "over all rows" note under the error.
    assert!(matches!(cs.show(&key(1)), ChartShow::Waiting));
    assert!(cs.note(&key(1)).is_none());
    cs.error = None; // Try again
    cs.shown = None;
    assert!(matches!(
        chart_step(&mut cs, &key(1), t0),
        ChartStep::Query(_)
    ));
    cs.local = true; // Use the loaded rows
    assert!(matches!(chart_step(&mut cs, &key(0), t0), ChartStep::Idle));
    assert!(matches!(cs.show(&key(0)), ChartShow::Local));
    assert!(cs.note(&key(0)).is_none());
}

/// The answer lands under the key its task was asked for; a newer query
/// replaces the task (dropping it cancels the older statement), so an older
/// answer can never land.
#[test]
fn an_answer_lands_under_the_key_it_was_asked_for() {
    let mut cs = server();
    cs.pending = Some(key(1));
    cs.task = Some(crate::app::pushdown::spawn_local_task(|| {
        ServerChart::NotExpressible
    }));
    // A newer query replaces it before it is read.
    cs.pending = Some(key(0));
    cs.task = Some(crate::app::pushdown::spawn_local_task(|| {
        ServerChart::Drawn {
            chart: Err(ChartError::EmptyAfterFilter),
            total: 7,
        }
    }));
    let start = Instant::now();
    while cs.poll() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the worker finished"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(cs.pending.is_none() && cs.task.is_none());
    assert_eq!(cs.shown.as_ref().map(|(k, _)| k.clone()), Some(key(0)));
    assert_eq!(cs.note(&key(0)).map(|n| n.total), Some(7));
}

#[test]
fn the_old_chart_stays_until_the_new_one_arrives() {
    let mut cs = server();
    assert!(
        matches!(cs.show(&key(1)), ChartShow::Waiting),
        "nothing yet"
    );
    cs.shown = Some((
        key(1),
        ServerChart::Drawn {
            chart: Err(ChartError::EmptyAfterFilter),
            total: 42,
        },
    ));
    assert!(
        matches!(cs.show(&key(0)), ChartShow::Drawn(_)),
        "previous result"
    );
    assert_eq!(cs.note(&key(1)).map(|n| n.total), Some(42));
    let incomplete = ChartKey::of(&ChartConfig::default(), ChartLimits::default());
    assert!(
        cs.note(&incomplete).is_none(),
        "drawing locally: no database note"
    );
    cs.shown = Some((key(1), ServerChart::NotExpressible));
    assert!(matches!(cs.show(&key(1)), ChartShow::NotExpressible));
    assert!(
        matches!(cs.show(&key(0)), ChartShow::Waiting),
        "not for another key"
    );
    assert!(
        matches!(cs.show(&incomplete), ChartShow::Local),
        "local message"
    );
}

/// Back to the shown settings while another chart is fetched: that query is
/// dropped (cancelled), so its answer cannot replace the shown one.
#[test]
fn going_back_to_the_shown_chart_drops_the_other_query() {
    let mut cs = server();
    cs.shown = Some((
        key(1),
        ServerChart::Drawn {
            chart: Err(ChartError::EmptyAfterFilter),
            total: 3,
        },
    ));
    cs.pending = Some(key(0));
    cs.task = Some(crate::app::pushdown::spawn_local_task(|| {
        ServerChart::NotExpressible
    }));
    assert!(matches!(
        chart_step(&mut cs, &key(1), Instant::now()),
        ChartStep::Idle
    ));
    assert!(cs.pending.is_none() && cs.task.is_none());
    assert!(!cs.poll());
    assert_eq!(cs.shown.as_ref().map(|(k, _)| k.clone()), Some(key(1)));
}
