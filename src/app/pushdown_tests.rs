use super::*;
use crate::app::state::DbOrigin;
use octa::db::{DEFAULT_QUERY_TIMEOUT_SECS, DbAuth, DbEngine};

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
        datetimeoffset_as_timestamp: false,
        query_timeout_secs: DEFAULT_QUERY_TIMEOUT_SECS,
        ssh: None,
        tunnel_port: None,
    }
}

fn db_tab(has_more: bool) -> TabState {
    let mut tab = TabState::new(Default::default());
    tab.table.columns = vec![octa::data::ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    tab.table.rows = vec![vec![octa::data::CellValue::Int(1)]];
    // A db tab keeps `total_rows` set as its "more may exist" flag.
    tab.table.total_rows = has_more.then_some(1);
    tab.db_origin = Some(DbOrigin {
        conn_id: "c1".into(),
        catalog: None,
        schema: "public".into(),
        table: "orders".into(),
        identity: None,
    });
    tab
}

#[test]
fn a_partial_db_tab_goes_to_the_server() {
    let src = server_source_for(&db_tab(true), true, &[conn()]).expect("server");
    assert_eq!(src.table, "orders");
    assert_eq!(src.from_sql(), "\"public\".\"orders\"");
}

/// The non-goal: nothing without a `db_origin` ever reaches the server,
/// even when its table is partial.
#[test]
fn file_and_result_tabs_never_go_to_the_server() {
    let mut file_tab = TabState::new(Default::default());
    file_tab.table.total_rows = Some(10_000_000);
    file_tab.table.rows = vec![vec![octa::data::CellValue::Int(1)]];
    assert!(file_tab.source_has_more());
    assert!(server_source_for(&file_tab, true, &[conn()]).is_none());
}

#[test]
fn a_complete_db_tab_the_setting_off_or_a_lost_connection_stay_local() {
    assert!(server_source_for(&db_tab(false), true, &[conn()]).is_none());
    assert!(server_source_for(&db_tab(true), false, &[conn()]).is_none());
    assert!(server_source_for(&db_tab(true), true, &[]).is_none());
}

/// The dialog's poll: waiting, a worker that died without an answer, a
/// Cancel, a server error, and "Run on loaded rows".
#[test]
fn the_value_frequency_poll_reads_each_outcome() {
    let mut vf = VfServer {
        key: (0, "id".into(), Some(50), BinningMode::None),
        load: crate::app::state::DbLoadJob::new(String::new()),
        slot: Arc::new(Mutex::new(None)),
        result: None,
        local: false,
    };
    assert!(matches!(vf.poll(), ServerVf::Pending));
    vf.load.finished.store(true, Ordering::Release);
    assert!(matches!(vf.poll(), ServerVf::Failed(m) if m == t("pushdown.stopped")));
    vf.load.cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(vf.poll(), ServerVf::Cancelled));
    vf.local = true;
    assert!(matches!(vf.poll(), ServerVf::Local));

    vf.local = false;
    vf.result = None;
    vf.load.cancelled.store(false, Ordering::Relaxed);
    *vf.slot.lock().unwrap() = Some(Err("permission denied".into()));
    assert!(matches!(vf.poll(), ServerVf::Failed(m) if m == "permission denied"));

    vf.result = None;
    *vf.slot.lock().unwrap() = Some(Ok(Arc::new(ValueFrequency {
        column_name: "id".into(),
        rows: Vec::new(),
        nulls: 2,
        total_non_null: 3,
        unique_count: 3,
        binned: false,
    })));
    let ServerVf::Ready(a, 5) = vf.poll() else {
        panic!("ready with 5 rows")
    };
    let ServerVf::Ready(b, _) = vf.poll() else {
        panic!("still ready")
    };
    // Every frame shares the one answer instead of copying it.
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn several_tabs_go_to_the_server_only_together_on_one_connection() {
    let mut other = db_tab(false);
    other.db_origin.as_mut().unwrap().table = "customers".into();
    let srcs = server_sources_for(&[&db_tab(true), &other], true, &[conn()]).expect("server");
    assert_eq!(
        srcs.iter().map(|s| s.table.as_str()).collect::<Vec<_>>(),
        ["orders", "customers"]
    );
    // Every tab complete: the loaded rows are every row already.
    assert!(server_sources_for(&[&db_tab(false), &other], true, &[conn()]).is_none());
    // A file tab in the set: no server path, and the dialog says why.
    let file = TabState::new(Default::default());
    assert!(server_sources_for(&[&db_tab(true), &file], true, &[conn()]).is_none());
    assert!(mixed_sources(&[&db_tab(true), &file], true, &[conn()]));
    // Another connection.
    let mut far = db_tab(true);
    far.db_origin.as_mut().unwrap().conn_id = "c2".into();
    let mut c2 = conn();
    c2.id = "c2".into();
    assert!(server_sources_for(&[&db_tab(true), &far], true, &[conn(), c2]).is_none());
    // The setting off: no server, and no "mixed" note either.
    assert!(!mixed_sources(&[&db_tab(true), &file], false, &[conn()]));
    // File tabs only, one stopped at the row cap (`usize::MAX` is a CSV's
    // "more, size unknown"): no database, so no note about one.
    let mut capped = TabState::new(Default::default());
    capped.table.rows = vec![vec![octa::data::CellValue::Int(1)]];
    capped.table.total_rows = Some(usize::MAX);
    assert!(capped.table.is_partial());
    assert!(!mixed_sources(&[&capped, &file], true, &[conn()]));
    // A self-join on one partial table goes to the server.
    let tab = db_tab(true);
    assert_eq!(
        server_sources_for(&[&tab, &tab], true, &[conn()]).map(|v| v.len()),
        Some(2)
    );
}

#[test]
fn a_dialog_task_reads_each_outcome_once() {
    let task: ServerTask<u8> = ServerTask {
        load: crate::app::state::DbLoadJob::new(String::new()),
        slot: Arc::new(Mutex::new(None)),
    };
    assert!(matches!(task.poll(), TaskPoll::Pending));
    *task.slot.lock().unwrap() = Some(Ok(7));
    assert!(matches!(task.poll(), TaskPoll::Ready(7)));
    task.load.finished.store(true, Ordering::Release);
    assert!(matches!(task.poll(), TaskPoll::Failed(m) if m == t("pushdown.stopped")));
    task.load.cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(task.poll(), TaskPoll::Cancelled));
}

#[test]
fn a_local_task_hands_back_its_result_or_says_it_stopped() {
    let wait = |task: &ServerTask<u8>| loop {
        match task.poll() {
            TaskPoll::Pending => std::thread::sleep(std::time::Duration::from_millis(5)),
            other => return other,
        }
    };
    assert!(matches!(wait(&spawn_local_task(|| 7)), TaskPoll::Ready(7)));
    let crashed = spawn_local_task(|| -> u8 { panic!("boom") });
    assert!(matches!(wait(&crashed), TaskPoll::Failed(m) if m == t("pushdown.stopped")));
}

fn idle_task() -> ServerTask<u8> {
    ServerTask {
        load: crate::app::state::DbLoadJob::new(String::new()),
        slot: Arc::new(Mutex::new(None)),
    }
}

/// A Cancel is only reported once the worker is done: until then the
/// worker may still own the connection.
#[test]
fn a_cancel_is_not_reported_while_the_worker_still_runs() {
    let task = idle_task();
    task.load.cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(task.poll(), TaskPoll::Pending));
    task.load.finished.store(true, Ordering::Release);
    assert!(matches!(task.poll(), TaskPoll::Cancelled));
}

/// Closing the dialog stops a statement still running, and never fires
/// the handle once the worker is done: the connection may be running the
/// next job's statement by then.
#[test]
fn dropping_a_dialog_task_cancels_only_a_running_statement() {
    let fired = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let armed = |task: &ServerTask<u8>| {
        let fired = fired.clone();
        *task.load.cancel.lock().unwrap() = Some(Box::new(move || {
            fired.fetch_add(1, Ordering::Relaxed);
        }));
    };

    let running = idle_task();
    armed(&running);
    let stop = running.load.cancelled.clone();
    drop(running);
    assert_eq!(fired.load(Ordering::Relaxed), 1);
    assert!(stop.load(Ordering::Relaxed));

    let done = idle_task();
    armed(&done);
    done.load.finished.store(true, Ordering::Release);
    drop(done);
    assert_eq!(fired.load(Ordering::Relaxed), 1);
}

#[test]
fn the_dialog_note_names_the_local_parts_and_the_engine() {
    let note = DialogNote {
        unsaved: true,
        local: vec!["Ignore punctuation".into()],
        loaded: 200,
        engine: Some(DbEngine::Mssql),
        rescored: true,
    };
    let lines = note.lines();
    assert_eq!(lines[0], t("pushdown.dialog_server"));
    assert!(lines[1].contains("Ignore punctuation") && lines[1].contains("200"));
    assert!(lines[1].contains(DbEngine::Mssql.label()));
    assert_eq!(lines[2], t("pushdown.keys_rescored_note"));
    assert_eq!(lines[3], t("pushdown.note_unsaved"));
    assert_eq!(
        DialogNote::default().lines(),
        vec![t("pushdown.dialog_server")]
    );
}

#[test]
fn the_cost_question_states_rows_pairs_and_money() {
    use octa::db::pushdown::row_estimate::RowCount;
    assert_eq!(column_pairs(&[3, 4, 5]), 3 * 4 + 3 * 5 + 4 * 5);
    assert_eq!(column_pairs(&[7]), 0);
    let lines = cost_lines(
        &[
            (
                "orders".into(),
                RowCount {
                    rows: 4_812_331,
                    estimate: true,
                },
            ),
            (
                "customers".into(),
                RowCount {
                    rows: 12,
                    estimate: false,
                },
            ),
        ],
        47,
    );
    let n = octa::ui::status_bar::format_number;
    assert!(lines[0].contains("orders") && lines[0].contains(&n(4_812_331)));
    assert_eq!(
        lines[0],
        t("pushdown.keys_cost_table_estimate")
            .replace("{table}", "orders")
            .replace("{rows}", &n(4_812_331))
    );
    assert_eq!(
        lines[1],
        t("pushdown.keys_cost_table")
            .replace("{table}", "customers")
            .replace("{rows}", "12")
    );
    assert!(lines[2].contains("47"));
    assert_eq!(lines[3], t("pushdown.keys_cost_money"));
}

#[test]
fn the_fetch_question_states_both_counts() {
    let lines = fetch_prompt_lines(200, 4_812_331);
    assert!(lines[0].contains("200"));
    assert!(lines[0].contains(&octa::ui::status_bar::format_number(4_812_331)));
    assert_eq!(lines[1], t("pushdown.fetch_why"));
}

#[test]
fn a_partial_db_tab_carries_a_partial_note() {
    assert_eq!(db_tab(true).partial_note(), Some((1, None)));
    assert_eq!(db_tab(false).partial_note(), None);
}
#[test]
fn the_note_names_the_rows_and_the_local_parts() {
    let note = PushdownNote {
        total: 4_812_331,
        loaded: 200,
        engine: DbEngine::MySql,
        local: LocalParts {
            engine: vec!["median".into(), "q25".into()],
            by_design: vec!["pii_flag".into()],
            failed: vec![],
        },
        unsaved_edits: true,
        source: SourceKey {
            conn_id: "c1".into(),
            catalog: None,
            schema: "s".into(),
            table: "t".into(),
        },
        more: false,
    };
    let lines = note.lines();
    assert!(
        lines
            .server
            .contains(&octa::ui::status_bar::format_number(4_812_331))
    );
    let local = lines.local.expect("local line");
    assert!(local.contains("median, q25") && local.contains("200") && local.contains("MySQL"));
    // Each list gets its own reason, never the engine's for the by-design one.
    let by_design = t("pushdown.note_local_by_design").replace("{parts}", "pii_flag");
    let by_design = by_design.replace("{loaded}", "200");
    assert!(local.contains(&by_design), "{local}");
    let only_by_design = PushdownNote {
        local: LocalParts {
            engine: vec![],
            by_design: vec!["pii_flag".into()],
            ..Default::default()
        },
        ..note.clone()
    };
    let line = only_by_design.lines().local.expect("local line");
    assert!(!line.contains("MySQL"), "{line}");
    assert!(lines.unsaved.is_some());
    // A refused query gets its own line, with its own key.
    let failed = PushdownNote {
        local: LocalParts {
            failed: vec!["outlier_count".into()],
            ..Default::default()
        },
        ..note
    };
    let line = failed.lines().local.expect("local line");
    let want = t("pushdown.note_local_failed")
        .replace("{parts}", "outlier_count")
        .replace("{loaded}", "200");
    assert_eq!(line, want);
    assert!(!line.contains("MySQL"), "{line}");
}

#[test]
fn a_cut_result_says_it_has_more() {
    let mut note = PushdownNote {
        total: 4_812_331,
        loaded: 200,
        engine: DbEngine::Postgres,
        local: LocalParts::default(),
        unsaved_edits: false,
        source: SourceKey {
            conn_id: "c1".into(),
            catalog: None,
            schema: "public".into(),
            table: "t".into(),
        },
        more: false,
    };
    assert!(note.lines().more.is_none());
    note.more = true;
    assert!(
        note.lines()
            .more
            .unwrap()
            .contains(&octa::ui::status_bar::format_number(
                octa::formats::initial_load_rows().saturating_sub(1).max(1)
            ))
    );
}
