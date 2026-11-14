use std::sync::atomic::Ordering;

use super::super::ChatTurnMode;
use crate::app::state::OctaApp;
use octa::ui::settings::chat_profiles::{ChatModelProfile, active_profile};

fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.settings.show_readonly_notice = false;
    octa::ui::settings::chat_profiles::ensure_profiles(&mut app.settings);
    app
}

fn profile(app: &OctaApp, allow_writes: bool) -> ChatModelProfile {
    ChatModelProfile {
        allow_writes,
        ..active_profile(&app.settings).clone()
    }
}

#[test]
fn a_database_tab_says_when_it_holds_only_part_of_its_table() {
    let mut app = app();
    app.tabs[0].table.columns = vec![octa::data::ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    app.tabs[0].table.rows = vec![vec![octa::data::CellValue::Int(1)]];
    let snapshot = |app: &OctaApp| app.build_tool_context(false).open_tabs.remove(0);
    assert!(snapshot(&app).db.is_none(), "a file tab has no server");

    app.tabs[0].db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "c".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
        identity: None,
    });
    assert!(snapshot(&app).db.is_none(), "this tab holds every row");

    // What the sidebar leaves on a page that came back full.
    app.tabs[0].table.total_rows = Some(1);
    let db = snapshot(&app).db.expect("more may exist");
    assert_eq!(db.table, "s.t");
    assert!(!db.edited);
    assert!(db.source.is_none(), "no saved connection to ask");

    let conn = octa::db::DbConnection::file(
        "c".into(),
        "Warehouse",
        octa::db::DbEngine::DuckDb,
        "/tmp/x.duckdb",
        false,
    );
    app.settings.db_connections = vec![conn];
    let db = snapshot(&app).db.unwrap();
    assert_eq!(db.connection, "Warehouse");
    assert_eq!(db.source.expect("the server can be asked").table, "t");

    app.settings.db_pushdown = false;
    assert!(
        snapshot(&app).db.unwrap().source.is_none(),
        "the user's call"
    );
    app.settings.db_pushdown = true;

    app.settings.db_connections[0].engine = octa::db::DbEngine::Sqlite;
    assert!(
        snapshot(&app).db.unwrap().source.is_none(),
        "too little SQL"
    );
    app.settings.db_connections[0].engine = octa::db::DbEngine::DuckDb;

    app.tabs[0].raw_content_modified = true;
    let db = snapshot(&app).db.unwrap();
    assert!(db.edited);
    assert!(
        db.source.is_none(),
        "an edit would vanish from every answer"
    );
}

#[test]
fn read_only_mode_takes_writing_away_from_every_profile() {
    let mut app = app();
    let writer = profile(&app, true);
    assert!(app.assistant_may_write(&writer, ChatTurnMode::Data));

    app.toggle_readonly();
    assert!(!app.assistant_may_write(&writer, ChatTurnMode::Data));
    let ctx = app.build_tool_context(app.assistant_may_write(&writer, ChatTurnMode::Data));
    assert!(
        ctx.read_only,
        "tools, SQL and database connections all read this"
    );
    assert!(!ctx.allow_existing_writes);

    app.toggle_readonly();
    assert!(
        app.assistant_may_write(&writer, ChatTurnMode::Data),
        "off again, back again"
    );
}

#[test]
fn a_profile_without_writes_stays_without_and_so_do_turns_without_tools() {
    let app = app();
    assert!(!app.assistant_may_write(&profile(&app, false), ChatTurnMode::Data));
    assert!(!app.assistant_may_write(&profile(&app, true), ChatTurnMode::Plain));
    assert!(!app.assistant_may_write(&profile(&app, true), ChatTurnMode::Plan));
}

/// The label beside the profile picker reads the check a reply runs under, so
/// it cannot say one thing while the assistant does another.
#[test]
fn the_label_beside_the_picker_says_what_the_next_message_may_do() {
    let mut app = app();
    let label = |app: &OctaApp| app.writes_status_keys().0;

    for p in &mut app.settings.chat_profiles {
        p.allow_writes = true;
    }
    assert_eq!(label(&app), "chat.writes_on");

    app.chat.mode = ChatTurnMode::Plain;
    assert_eq!(label(&app), "chat.writes_off", "a turn without tools");
    app.chat.mode = ChatTurnMode::Plan;
    assert_eq!(label(&app), "chat.writes_off", "a plan changes nothing");
    app.chat.mode = ChatTurnMode::Data;

    app.toggle_readonly();
    assert_eq!(label(&app), "chat.writes_off", "F8 overrides the profile");
    app.toggle_readonly();
    assert_eq!(label(&app), "chat.writes_on");

    for p in &mut app.settings.chat_profiles {
        p.allow_writes = false;
    }
    assert_eq!(label(&app), "chat.writes_off");
}

#[test]
fn the_label_and_its_tooltip_are_real_strings() {
    let mut app = app();
    for allow in [true, false] {
        for p in &mut app.settings.chat_profiles {
            p.allow_writes = allow;
        }
        let (label, hint) = app.writes_status_keys();
        assert_ne!(octa::i18n::t(label), label);
        assert_ne!(octa::i18n::t(hint), hint);
    }
}

fn app_with_running_reply(allow_writes: bool) -> OctaApp {
    let mut app = app();
    for p in &mut app.settings.chat_profiles {
        p.allow_writes = allow_writes;
    }
    app.chat
        .session
        .lock()
        .unwrap()
        .running
        .store(true, Ordering::Relaxed);
    app
}

fn was_cancelled(app: &OctaApp) -> bool {
    app.chat
        .session
        .lock()
        .unwrap()
        .cancel
        .load(Ordering::Relaxed)
}

/// A reply holds the permission it started with, so switching Read-only mode on
/// has to stop one that may write, or it would keep writing under it.
#[test]
fn switching_read_only_mode_on_stops_a_reply_that_may_write() {
    let mut app = app_with_running_reply(true);
    app.toggle_readonly();
    assert!(was_cancelled(&app));
}

#[test]
fn a_reply_that_cannot_write_is_left_to_finish() {
    let mut app = app_with_running_reply(false);
    app.toggle_readonly();
    assert!(!was_cancelled(&app));
}

#[test]
fn switching_read_only_mode_off_stops_nothing() {
    let mut app = app();
    app.toggle_readonly();
    let session = app.chat.session.clone();
    session
        .lock()
        .unwrap()
        .running
        .store(true, Ordering::Relaxed);
    for p in &mut app.settings.chat_profiles {
        p.allow_writes = true;
    }
    app.toggle_readonly();
    assert!(!was_cancelled(&app));
}

#[test]
fn writing_mode_runs_no_tools() {
    assert!(!ChatTurnMode::Writing.runs_tools());
    assert!(ChatTurnMode::Writing.is_writing());
    assert!(!ChatTurnMode::Plain.is_writing());
}

#[test]
fn writing_needs_a_tab_that_holds_text() {
    let mut app = app();
    assert!(app.writing_target().is_none(), "an empty tab has no text");
    app.tabs[0].raw_content = Some("hello\n".into());
    let (_name, text) = app.writing_target().expect("a text tab is a target");
    assert_eq!(text, "hello\n");
}

#[test]
fn accepting_a_text_follows_the_profile_and_read_only_mode() {
    let mut app = app();
    let set = |app: &mut OctaApp, allow: bool| {
        for p in &mut app.settings.chat_profiles {
            p.allow_writes = allow;
        }
    };
    set(&mut app, false);
    assert_eq!(app.writing_blocked(), Some("greyed.writing_no_writes"));
    set(&mut app, true);
    assert_eq!(app.writing_blocked(), None);
    app.readonly_mode = true;
    assert_eq!(app.writing_blocked(), Some("greyed.writing_no_writes"));
}

#[test]
fn the_writes_label_in_writing_mode_follows_the_same_check() {
    let mut app = app();
    app.chat.mode = ChatTurnMode::Writing;
    for p in &mut app.settings.chat_profiles {
        p.allow_writes = true;
    }
    assert_eq!(app.writes_status_keys().0, "chat.writes_on");
    app.readonly_mode = true;
    assert_eq!(app.writes_status_keys().0, "chat.writes_off");
}

/// Make the active profile one that runs through a vendor's program.
fn use_cli_profile(app: &mut OctaApp) {
    let id = app.settings.chat_active_profile.clone();
    for p in &mut app.settings.chat_profiles {
        if p.id == id {
            p.kind = octa::ui::settings::ChatProviderKind::CodexCli;
            p.base_url = "octa-no-such-program-xyz".into();
        }
    }
}

#[test]
fn a_subscription_profile_needs_no_key_and_names_its_program() {
    use octa::ui::settings::ChatProviderKind as K;
    for kind in [K::ClaudeCli, K::CopilotCli, K::CodexCli] {
        assert!(!kind.needs_api_key());
        assert!(kind.cli_program().is_some());
    }
    assert!(K::Anthropic.cli_program().is_none());
    assert!(K::Anthropic.needs_api_key());
    assert!(!K::Ollama.needs_api_key());
}

/// Subscriptions do what API profiles do (user, 2026-11-11): the turn goes
/// out in every mode, and the tools travel in the prompt (`providers/cli.rs`).
#[test]
fn a_subscription_profile_sends_a_turn_in_every_mode() {
    for mode in [ChatTurnMode::Data, ChatTurnMode::Plan, ChatTurnMode::Plain] {
        let mut app = app();
        use_cli_profile(&mut app);
        app.chat.mode = mode;
        app.chat.input = "count the rows".into();
        app.send_chat_message(&eframe::egui::Context::default());
        assert_eq!(
            app.chat.session.lock().unwrap().messages.len(),
            1,
            "the question went out in {mode:?}"
        );
        assert!(app.chat.input.is_empty(), "and the box is empty again");
        assert_eq!(app.chat.mode, mode, "the mode stays what was picked");
        app.cancel_chat();
    }
}

#[test]
fn a_subscription_profile_sends_a_writing_turn_without_tools() {
    let mut app = app();
    use_cli_profile(&mut app);
    app.tabs[0].raw_content = Some("hello\n".into());
    app.chat.mode = ChatTurnMode::Writing;
    app.chat.input = "make it formal".into();
    app.send_chat_message(&eframe::egui::Context::default());
    assert_eq!(
        app.chat.session.lock().unwrap().messages.len(),
        1,
        "the question went out"
    );
    assert_eq!(
        app.chat.writing_sent.as_ref().map(|s| s.base.as_str()),
        Some("hello\n")
    );
    app.cancel_chat();
}
