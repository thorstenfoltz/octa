//! An SQL editor only goes when the user closes it: closing, refreshing or
//! reusing its tab leaves it whole.

use std::sync::{Arc, Mutex};

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::sql::TableOrigin;

use crate::app::refresh::{ReloadSlot, ReloadTarget};
use crate::app::sql_panel::{ServerQueryDone, SqlServerJob};
use crate::app::state::{OctaApp, TabState};

fn table(names: &[&str]) -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        }],
        rows: names
            .iter()
            .map(|n| vec![CellValue::String((*n).into())])
            .collect(),
        ..DataTable::empty()
    }
}

/// `app.tabs` = one tab per name, each with a table, the first active.
fn app_with_tabs(names: &[&str]) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for name in names {
        let mut tab = TabState::new(octa::data::SearchMode::default());
        tab.table = table(&[name]);
        app.tabs.push(tab);
    }
    app
}

/// One frame in which the window was asked to close: what the app told it.
fn frame_with_close_request(app: &mut OctaApp) -> Vec<egui::ViewportCommand> {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = ctx.run_ui(input, |ui| app.handle_shortcuts(ui.ctx()));
    output.textures_delta.clear();
    output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default()
}

#[test]
fn closing_the_window_asks_about_a_modified_tab_in_the_background() {
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[1].raw_content_modified = true;
    let commands = frame_with_close_request(&mut app);
    assert!(
        commands.contains(&egui::ViewportCommand::CancelClose),
        "the window must stay open for the answer"
    );
    assert!(app.show_close_confirm);
    assert_eq!(app.active_tab, 1, "the prompt is about the tab on screen");
}

#[test]
fn closing_the_window_with_nothing_modified_is_left_alone() {
    let mut app = app_with_tabs(&["a", "b"]);
    let commands = frame_with_close_request(&mut app);
    assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
    assert!(!app.show_close_confirm);
}

#[test]
fn quitting_with_nothing_modified_asks_nothing() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b"]);
    app.request_exit(&ctx);
    assert!(!app.show_close_confirm);
    assert!(app.confirmed_close);
}

/// The answer "Don't Save" as the prompt gives it.
fn answer_dont_save(app: &mut OctaApp, ctx: &egui::Context) {
    app.show_close_confirm = false;
    app.next_exit_tab(ctx);
}

#[test]
fn quitting_asks_about_every_modified_tab_and_closes_none_of_them() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b", "c"]);
    app.tabs[0].raw_content_modified = true;
    app.tabs[2].raw_content_modified = true;
    app.active_tab = 1;

    app.request_exit(&ctx);
    assert!(app.show_close_confirm);
    assert_eq!(app.active_tab, 0);
    assert!(
        app.pending_close_tab.is_none(),
        "it is Octa that is closing"
    );

    answer_dont_save(&mut app, &ctx);
    assert!(app.show_close_confirm);
    assert_eq!(app.active_tab, 2);
    assert!(!app.confirmed_close);

    answer_dont_save(&mut app, &ctx);
    assert!(!app.show_close_confirm);
    assert!(app.confirmed_close);
    assert_eq!(
        app.tabs.len(),
        3,
        "a tab closed here would be missing from Reopen last session"
    );
}

#[test]
fn quitting_asks_about_the_tab_on_screen_first() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[0].raw_content_modified = true;
    app.tabs[1].raw_content_modified = true;
    app.active_tab = 1;
    app.request_exit(&ctx);
    assert_eq!(app.active_tab, 1);
}

#[test]
fn a_tab_saved_while_the_quit_waits_is_not_asked_about() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b", "c"]);
    app.tabs[0].raw_content_modified = true;
    app.tabs[1].raw_content_modified = true;
    app.request_exit(&ctx);
    app.tabs[1].raw_content_modified = false;
    answer_dont_save(&mut app, &ctx);
    assert!(!app.show_close_confirm);
    assert!(app.confirmed_close);
}

/// A background save hands over to the next tab when it lands. After a Cancel
/// that must do nothing: tabs nobody was asked about would go unsaved.
#[test]
fn a_save_landing_after_cancel_does_not_finish_the_quit() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[0].raw_content_modified = true;
    app.tabs[1].raw_content_modified = true;
    app.request_exit(&ctx);
    app.dismiss_close_prompt();
    assert!(!app.show_close_confirm);

    app.next_exit_tab(&ctx);
    assert!(!app.show_close_confirm);
    assert!(!app.confirmed_close);
}

#[test]
fn quitting_takes_over_from_a_tab_close_waiting_for_its_answer() {
    let ctx = egui::Context::default();
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[1].raw_content_modified = true;
    app.pending_close_tab = Some(1);
    app.show_close_confirm = true;
    app.close_queue = vec![app.tabs[1].id];
    app.request_exit(&ctx);
    assert!(app.pending_close_tab.is_none());
    assert!(app.close_queue.is_empty());
    assert!(app.show_close_confirm);
}

fn with_editor(tab: &mut TabState, query: &str) -> u64 {
    tab.sql_panel_open = true;
    tab.sql.query = query.into();
    tab.sql.id
}

/// A server query in flight from `pane_id`, finished as `done`.
fn server_job_done(app: &mut OctaApp, pane_id: u64, done: ServerQueryDone) {
    app.sql_server_job = Some(SqlServerJob {
        pane_id,
        query: "select 1".into(),
        started: std::time::Instant::now(),
        result: Arc::new(Mutex::new(Some((done, "select 1".to_string())))),
        cancel: Arc::new(Mutex::new(None)),
    });
}

/// Land whatever the local job of tab `idx` finishes with.
fn wait_for_local_job(app: &mut OctaApp, idx: usize) {
    let ctx = egui::Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while app.tabs[idx].sql_job.is_some() {
        assert!(std::time::Instant::now() < deadline, "SQL job never landed");
        app.drive_sql_jobs(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The report: closing a tab with a server query running threw its editor
/// away, and the result arrived with "its tab was closed".
#[test]
fn a_running_server_query_lands_in_its_editor_after_its_tab_closed() {
    let mut app = app_with_tabs(&["a", "b"]);
    // A machine with no file dialog (CI) starts with a warning about that.
    app.status_message = None;
    let id = with_editor(&mut app.tabs[0], "select 1");
    app.tabs[0].sql_target = Some("conn".into());
    app.tabs[0].sql.start_run();

    app.close_tab(0);

    assert_eq!(app.tabs.len(), 1, "the tab itself does close");
    let tab = &app.tabs[0];
    assert_eq!(tab.sql.id, id, "the same editor, not a copy of its text");
    assert_eq!(tab.sql.query, "select 1");
    assert_eq!(tab.sql_target.as_deref(), Some("conn"));
    assert!(tab.sql_panel_open);
    assert!(tab.sql.running_since.is_some(), "still shows as running");

    server_job_done(
        &mut app,
        id,
        ServerQueryDone::Rows(Box::new(table(&["answer"]))),
    );
    app.drain_sql_server_job();
    let tab = &app.tabs[0];
    assert!(tab.sql.running_since.is_none());
    assert_eq!(tab.sql.result.as_ref().map(|t| t.row_count()), Some(1));
    assert!(app.status_message.is_none(), "no complaint about the tab");
}

#[test]
fn the_message_only_comes_when_the_editor_itself_was_closed() {
    let mut app = app_with_tabs(&["a"]);
    app.tabs[0].add_sql_pane();
    let id = app.tabs[0].sql.id;
    app.tabs[0].sql.start_run();
    let active = app.tabs[0].sql_active_pane;
    app.tabs[0].close_sql_pane(active);

    server_job_done(&mut app, id, ServerQueryDone::Affected(1));
    app.drain_sql_server_job();
    assert_eq!(
        app.status_message.as_ref().map(|(m, _)| m.as_str()),
        Some(octa::i18n::t("sql.server_editor_gone").as_str())
    );
}

#[test]
fn every_editor_moves_with_its_result_and_file() {
    let mut app = app_with_tabs(&["a", "b"]);
    let tab = &mut app.tabs[0];
    with_editor(tab, "one");
    tab.sql.result = Some(table(&["kept"]));
    tab.add_sql_pane();
    tab.put_sql_file("q.sql".into(), "two".into(), false);

    app.close_tab(0);

    let tab = &app.tabs[0];
    assert_eq!(tab.sql_queries(), ["one", "two"]);
    assert_eq!(tab.sql_active_pane, 1, "the same editor stays active");
    assert!(tab.sql.file.is_some());
    assert!(tab.sql_panes[0].result.is_some());
}

#[test]
fn a_next_tab_using_its_own_editors_keeps_them_and_a_new_tab_takes_the_closed_ones() {
    let mut app = app_with_tabs(&["a", "b"]);
    let id = with_editor(&mut app.tabs[0], "closed tab's");
    with_editor(&mut app.tabs[1], "b's own");

    app.close_tab(0);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[0].sql_queries(), ["b's own"]);
    let kept = &app.tabs[1];
    assert_eq!(kept.sql.id, id);
    assert_eq!(kept.sql.query, "closed tab's");
    assert_eq!(kept.table.col_count(), 0, "the closed table is gone");
    assert!(kept.sql_maximised && kept.sql_panel_open);
    assert_eq!(app.active_tab, 1, "the editor stays on screen");
}

#[test]
fn a_next_tab_that_cannot_show_the_panel_does_not_take_the_editors() {
    let mut app = app_with_tabs(&["a", "b"]);
    with_editor(&mut app.tabs[0], "q");
    app.tabs[1].view_mode = octa::data::ViewMode::Raw;

    app.close_tab(0);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[0].sql_queries(), [""]);
    assert_eq!(app.tabs[1].sql.query, "q");
    assert_eq!(app.active_tab, 1);
}

/// A background close used to lose the editors silently, and moved the
/// window to another tab as well.
#[test]
fn closing_a_background_tab_keeps_its_editors_and_leaves_the_view_alone() {
    let mut app = app_with_tabs(&["a", "b", "c"]);
    let id = with_editor(&mut app.tabs[0], "q");
    let open = with_editor(&mut app.tabs[1], "on screen");
    app.active_tab = 1;

    app.close_tab(0);

    assert_eq!(app.active_tab, 0, "b is still the tab on screen");
    assert_eq!(app.tabs[app.active_tab].sql.id, open);
    assert_eq!(app.tabs.len(), 3, "b, c and the editors' own tab");
    assert_eq!(app.tabs[2].sql.id, id);
    assert!(!app.tabs[1].sql_panel_open, "c is left as it was");
}

#[test]
fn closing_a_background_tab_with_nothing_in_its_editors_just_closes_it() {
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[0].sql_panel_open = true;
    app.active_tab = 1;
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert!(!app.tabs[0].sql_panel_open);
}

#[test]
fn closing_the_last_tab_keeps_the_editors_on_a_blank_one() {
    let mut app = app_with_tabs(&["a"]);
    let id = with_editor(&mut app.tabs[0], "q");
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].sql.id, id);
    assert_eq!(app.tabs[0].table.col_count(), 0);
    assert!(app.tabs[0].sql_maximised);

    // And again: that tab is the editors' only home.
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].sql.id, id);
}

/// Otherwise a tab of nothing but editors could never be closed.
#[test]
fn a_tab_of_nothing_but_editors_closes_when_no_other_tab_can_take_them() {
    let mut app = app_with_tabs(&["a"]);
    with_editor(&mut app.tabs[0], "a's own");
    app.tabs
        .insert(0, TabState::new(octa::data::SearchMode::default()));
    with_editor(&mut app.tabs[0], "q");
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].sql_queries(), ["a's own"]);
}

#[test]
fn a_panel_closed_first_goes_with_the_tab_unless_a_query_still_runs_in_it() {
    let mut app = app_with_tabs(&["a", "b"]);
    with_editor(&mut app.tabs[0], "q");
    app.tabs[0].sql_panel_open = false;
    app.close_tab(0);
    assert_eq!(app.tabs[0].sql_queries(), [""]);
    assert!(!app.tabs[0].sql_panel_open);

    let mut app = app_with_tabs(&["a", "b"]);
    let id = with_editor(&mut app.tabs[0], "q");
    app.tabs[0].sql_panel_open = false;
    app.tabs[0].sql.start_run();
    app.close_tab(0);
    assert_eq!(app.tabs[0].sql.id, id);
    assert!(app.tabs[0].sql_panel_open, "shown again, it holds a run");
}

#[test]
fn switched_off_the_editor_closes_with_its_tab() {
    let mut app = app_with_tabs(&["a", "b"]);
    app.settings.sql_keep_open = false;
    let id = with_editor(&mut app.tabs[0], "q");
    app.tabs[0].sql.start_run();
    app.close_tab(0);
    assert_eq!(app.tabs.len(), 1);
    assert!(!app.tabs[0].has_sql_pane(id));
    assert!(!app.tabs[0].sql_panel_open);
}

/// `data` must not linger: the closed table's rows would answer a query on a
/// tab with no table at all.
#[test]
fn a_blank_tab_taking_the_editors_has_no_data_but_keeps_what_was_attached() {
    let mut app = app_with_tabs(&["a"]);
    with_editor(&mut app.tabs[0], "select 1");
    crate::app::sql_panel::ensure_workspace(&mut app.tabs[0]);
    let tab = &mut app.tabs[0];
    let ws = tab.sql_workspace.as_mut().unwrap();
    ws.add_table(
        "extra",
        &table(&["x"]),
        TableOrigin::TabClone("extra".into()),
    )
    .unwrap();
    ws.set_active_table(&table(&["rows"])).unwrap();

    app.close_tab(0);

    let ws = app.tabs[0].sql_workspace.as_mut().unwrap();
    let names: Vec<_> = ws
        .list_tables()
        .iter()
        .map(|t| t.sql_name.clone())
        .collect();
    assert_eq!(names, ["extra"]);
    assert!(ws.execute("SELECT * FROM data").is_err());
    assert!(ws.execute("SELECT * FROM extra").is_ok());
}

#[test]
fn a_tab_with_tables_added_to_its_workspace_is_not_free_to_take_editors() {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    assert!(tab.sql_can_host());
    crate::app::sql_panel::ensure_workspace(&mut tab);
    let ws = tab.sql_workspace.as_mut().unwrap();
    ws.set_active_table(&table(&["rows"])).unwrap();
    assert!(tab.sql_can_host(), "`data` alone is not the user's");
    ws_add(&mut tab, "extra");
    assert!(!tab.sql_can_host());
    tab.sql_auto_registered.push("extra".into());
    assert!(tab.sql_can_host(), "Octa put the other tabs' copies there");
}

fn ws_add(tab: &mut TabState, name: &str) {
    tab.sql_workspace
        .as_mut()
        .unwrap()
        .add_table(name, &table(&["x"]), TableOrigin::TabClone(name.into()))
        .unwrap();
}

/// A local SELECT survives the close too, where it used to be interrupted.
#[test]
fn a_running_local_query_lands_in_its_editor_after_its_tab_closed() {
    let mut app = app_with_tabs(&["a", "b"]);
    let id = with_editor(&mut app.tabs[0], "SELECT count(*) FROM data");
    app.start_sql_job(0, None);
    wait_for_local_job(&mut app, 0);
    app.tabs[0].sql.start_run();
    app.start_sql_job(0, Some("SELECT count(*) AS n FROM data".into()));

    app.close_tab(0);
    assert_eq!(app.tabs[0].sql.id, id);
    wait_for_local_job(&mut app, 0);

    let tab = &app.tabs[0];
    assert_eq!(tab.sql.error, None);
    assert_eq!(
        tab.sql.result.as_ref().map(|t| t.rows[0][0].to_string()),
        Some("1".to_string())
    );
    assert!(tab.sql.running_since.is_none());
}

/// The query kept the workspace busy when the editors moved, so `data` could
/// not follow them then. It must once the query landed, or a tab goes on
/// showing the rows of the one that closed.
#[test]
fn data_follows_the_tab_the_editor_moved_to_once_its_query_lands() {
    let mut app = app_with_tabs(&["a", "b"]);
    app.tabs[1].table = table(&["b1", "b2"]);
    with_editor(&mut app.tabs[0], "SELECT count(*) FROM data");
    app.start_sql_job(0, None);
    wait_for_local_job(&mut app, 0);
    app.tabs[0].sql.start_run();
    app.start_sql_job(0, Some("SELECT count(*) AS n FROM data".into()));

    app.close_tab(0);
    wait_for_local_job(&mut app, 0);

    let tab = &app.tabs[0];
    assert_eq!(
        tab.sql.result.as_ref().map(|t| t.rows[0][0].to_string()),
        Some("1".to_string()),
        "the query itself read the closed tab's one row"
    );
    let data_rows = tab.sql_workspace.as_ref().and_then(|ws| {
        ws.list_tables()
            .iter()
            .find(|t| t.sql_name == "data")
            .map(|t| t.row_count)
    });
    assert_eq!(data_rows, Some(2), "`data` is this tab's table now");
}

/// A write read the closed table: folding its rows back would replace the
/// table of whichever tab the editor moved to.
#[test]
fn a_write_still_running_never_folds_over_the_tab_its_editor_moved_to() {
    let mut app = app_with_tabs(&["a", "b"]);
    with_editor(&mut app.tabs[0], "UPDATE data SET name = 'overwritten'");
    app.start_sql_job(0, None);
    wait_for_local_job(&mut app, 0);
    app.tabs[0].sql.start_run();
    app.start_sql_job(0, Some("UPDATE data SET name = 'overwritten'".into()));

    app.close_tab(0);
    assert!(!app.tabs[0].holds_rows(), "b stays editable meanwhile");
    wait_for_local_job(&mut app, 0);

    let tab = &app.tabs[0];
    assert_eq!(tab.table.rows[0][0], CellValue::String("b".into()));
    assert_eq!(
        tab.sql.error.as_deref(),
        Some(octa::i18n::t("sql.write_table_gone").as_str())
    );
}

/// A refresh replaces the tab: its editors, even with the setting off, are
/// not part of what gets read again. Auto-refresh and a tab memory reload
/// come through here too.
#[test]
fn a_refresh_keeps_the_editors_and_their_panel_and_results() {
    let mut app = app_with_tabs(&["a"]);
    app.settings.sql_keep_open = false;
    let id = with_editor(&mut app.tabs[0], "q");
    let tab = &mut app.tabs[0];
    tab.sql_maximised = true;
    tab.sql_target = Some("conn".into());
    tab.sql.result = Some(table(&["kept"]));
    tab.table.source_path = Some("/data/a.csv".into());
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: "/data/a.csv".into(),
        discard_edits: false,
    });

    assert_eq!(app.take_reload_slot("/data/a.csv"), ReloadSlot::Replaced);
    app.apply_loaded_table(std::path::PathBuf::from("/data/a.csv"), table(&["fresh"]));

    let tab = &app.tabs[0];
    assert_eq!(tab.table.rows[0][0], CellValue::String("fresh".into()));
    assert_eq!(tab.sql.id, id);
    assert_eq!(tab.sql.query, "q");
    assert_eq!(tab.sql_target.as_deref(), Some("conn"));
    assert!(tab.sql_panel_open && tab.sql_maximised);
    assert!(tab.sql.result.is_some());
}

/// The next new file is not a refresh.
#[test]
fn a_refresh_only_spares_the_read_it_was_for() {
    let mut app = app_with_tabs(&["a"]);
    with_editor(&mut app.tabs[0], "q");
    app.tabs[0].sql.result = Some(table(&["kept"]));
    app.tabs[0].table.source_path = Some("/data/a.csv".into());
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: "/data/a.csv".into(),
        discard_edits: false,
    });
    assert_eq!(app.take_reload_slot("/data/a.csv"), ReloadSlot::Replaced);
    app.apply_loaded_table(std::path::PathBuf::from("/data/a.csv"), table(&["fresh"]));
    assert!(!app.tabs[0].is_reload_slot);
}

/// A database or API tab's refresh, and any result tab landing on a blank
/// tab, go through the blank-tab reuse: `tabs[active] = new_tab`.
#[test]
fn a_result_tab_reusing_a_blank_tab_keeps_its_editors() {
    let mut app = app_with_tabs(&[]);
    app.tabs
        .push(TabState::new(octa::data::SearchMode::default()));
    let id = with_editor(&mut app.tabs[0], "q");

    let mut result = TabState::new(octa::data::SearchMode::default());
    result.table = table(&["rows"]);
    app.push_result_tab(result);

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].table.row_count(), 1);
    assert_eq!(app.tabs[0].sql.id, id);
    assert!(app.tabs[0].sql_panel_open);
}

/// A database refresh lands through that same reuse, and used to replace the
/// tab `take_reload_slot` had just kept: auto-refresh stopped after one read,
/// the pin, the name and the tab's identity went, and so did its editors.
#[test]
fn a_database_refresh_replaces_only_the_rows() {
    let mut app = app_with_tabs(&["a"]);
    let source = crate::app::refresh::db_source("conn", None, "public", "t");
    let pane = with_editor(&mut app.tabs[0], "");
    let tab = &mut app.tabs[0];
    let tab_id = tab.id;
    tab.sql_maximised = true;
    tab.sql_target = Some("other".into());
    tab.pinned = true;
    tab.custom_tab_label = Some("renamed".into());
    tab.auto_refresh = Some(crate::app::live::AutoRefresh {
        every: std::time::Duration::from_secs(5),
        next_at: std::time::Instant::now(),
        before: None,
        return_to: None,
    });
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: source.clone(),
        discard_edits: false,
    });

    assert_eq!(app.take_reload_slot(&source), ReloadSlot::Replaced);
    let mut read = TabState::new(octa::data::SearchMode::default());
    read.table = table(&["fresh", "rows"]);
    read.custom_tab_label = Some("public.t".into());
    read.sql_target = Some("conn".into());
    app.push_result_tab(read);

    assert_eq!(app.tabs.len(), 1);
    let tab = &app.tabs[0];
    assert_eq!(tab.table.row_count(), 2);
    assert_eq!(tab.id, tab_id);
    assert!(tab.pinned);
    assert_eq!(tab.custom_tab_label.as_deref(), Some("renamed"));
    assert!(tab.auto_refresh.is_some(), "the next tick still comes");
    assert_eq!(tab.sql.id, pane, "an empty editor stays too");
    assert_eq!(tab.sql_target.as_deref(), Some("other"));
    assert!(tab.sql_panel_open && tab.sql_maximised);
    assert!(!tab.is_reload_slot);
}

#[test]
fn a_database_tab_on_a_blank_tab_keeps_its_own_connection_when_nothing_was_typed() {
    let mut app = app_with_tabs(&[]);
    app.tabs
        .push(TabState::new(octa::data::SearchMode::default()));
    app.tabs[0].sql_panel_open = true;

    let mut result = TabState::new(octa::data::SearchMode::default());
    result.table = table(&["rows"]);
    result.sql_target = Some("conn".into());
    app.push_result_tab(result);

    assert_eq!(app.tabs[0].sql_target.as_deref(), Some("conn"));
    assert!(app.tabs[0].sql_panel_open, "the open panel stays open");
}

/// A file opening into a tab whose panel is hidden wiped the hidden text.
#[test]
fn a_file_opening_into_a_tab_keeps_the_text_of_a_hidden_panel() {
    let mut app = app_with_tabs(&[]);
    app.tabs
        .push(TabState::new(octa::data::SearchMode::default()));
    with_editor(&mut app.tabs[0], "q");
    app.tabs[0].sql_panel_open = false;
    app.apply_loaded_table(std::path::PathBuf::from("/data/a.csv"), table(&["rows"]));
    assert_eq!(app.tabs[0].sql.query, "q");
    assert!(!app.tabs[0].sql_panel_open, "and it stays hidden");
}

/// JSON opens in the tree, which has no room for the panel: the panel only
/// hides there, so the editors still count as open when the tab closes.
#[test]
fn a_view_without_room_for_the_panel_hides_it_rather_than_closing_it() {
    let mut app = app_with_tabs(&[]);
    app.tabs
        .push(TabState::new(octa::data::SearchMode::default()));
    let id = with_editor(&mut app.tabs[0], "q");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.json");
    std::fs::write(&path, "{\"name\": \"rows\"}").unwrap();
    let mut json = table(&["rows"]);
    json.format_name = Some("JSON".into());
    app.apply_loaded_table(path, json);
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::JsonTree);
    assert!(app.tabs[0].sql_panel_open);

    app.tabs
        .push(TabState::new(octa::data::SearchMode::default()));
    app.close_tab(0);
    assert_eq!(app.tabs[0].sql.id, id);
}
