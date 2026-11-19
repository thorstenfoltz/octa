use super::*;
use crate::app::state::TabState;

#[test]
fn cloud_and_connections_need_their_own_switch() {
    let tabs = vec![
        SavedTab::File {
            path: "a.csv".into(),
        },
        SavedTab::Cloud {
            conn_id: "c".into(),
            key: "k".into(),
        },
        SavedTab::Db {
            conn_id: "d".into(),
            catalog: None,
            schema: "s".into(),
            table: "t".into(),
        },
        SavedTab::Api {
            conn_id: "api".into(),
            path: None,
        },
    ];
    let mut settings = AppSettings {
        restore_session: true,
        ..Default::default()
    };
    assert_eq!(wanted(tabs.clone(), &settings).len(), 1);
    settings.restore_session_cloud = true;
    assert_eq!(wanted(tabs.clone(), &settings).len(), 2);
    settings.restore_session_connections = true;
    assert_eq!(wanted(tabs, &settings).len(), 4);
}

#[test]
fn session_file_round_trips() {
    let file = SessionFile {
        tabs: vec![
            SavedTab::File {
                path: "/x/a.csv".into(),
            },
            SavedTab::Db {
                conn_id: "d".into(),
                catalog: Some("cat".into()),
                schema: "s".into(),
                table: "t".into(),
            },
        ],
        ..Default::default()
    };
    let text = toml::to_string_pretty(&file).unwrap();
    let back: SessionFile = toml::from_str(&text).unwrap();
    assert_eq!(back.tabs, file.tabs);
}

/// A tab opened from an encrypted zip is restored from the zip (which asks
/// for the passphrase again), never from its plain temporary copy.
#[test]
fn a_protected_tab_is_saved_as_its_zip() {
    let temp = std::sync::Arc::new(octa::formats::encrypt::ProtectedTempDir::new().unwrap());
    let temp_file = temp.path().join("report.csv");
    let mut tab = TabState::new(Default::default());
    tab.table.source_path = Some(temp_file.to_string_lossy().into_owned());
    tab.protected_origin = Some(crate::app::state::ProtectedOrigin {
        zip: std::path::PathBuf::from("/data/report.csv.zip"),
        entry: "report.csv".into(),
        passphrase: "pw".into(),
        temp_file,
        _temp: temp,
    });
    match saved_tab(&tab) {
        Some(SavedTab::File { path }) => assert_eq!(path, "/data/report.csv.zip"),
        other => panic!("expected the zip, got {other:?}"),
    }
}

fn file(p: &str) -> SavedTab {
    SavedTab::File { path: p.into() }
}

fn layout_with(a: &str, b: &str) -> SessionLayout {
    SessionLayout {
        groups: vec![SavedGroup {
            name: "G".into(),
            colour: MarkColor::Blue,
            folded: true,
            members: vec![file(a), file(b)],
        }],
        tiles: vec![SavedTiles {
            layout: TileLayout::SideBySide,
            xs: vec![0.3],
            ys: vec![],
            members: vec![file(a), file(b)],
        }],
        colours: vec![],
    }
}

fn app_with_files(paths: &[&str]) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for p in paths {
        let mut t = TabState::new(app.settings.default_search_mode);
        t.table.source_path = Some(p.to_string());
        app.tabs.push(t);
    }
    app
}

#[test]
fn layout_round_trips_and_old_files_load() {
    let file_ = SessionFile {
        tabs: vec![file("/a.csv"), file("/b.csv")],
        layout: layout_with("/a.csv", "/b.csv"),
        ..Default::default()
    };
    let back: SessionFile = toml::from_str(&toml::to_string_pretty(&file_).unwrap()).unwrap();
    assert_eq!(back.layout, file_.layout);
    let old: SessionFile =
        toml::from_str("[[tabs]]\nkind = \"file\"\npath = \"/a.csv\"\n").unwrap();
    assert_eq!(old.layout, SessionLayout::default());
    assert!(old.sql_editors.is_empty());
}

fn editor(query: &str) -> SavedEditor {
    SavedEditor {
        query: query.into(),
        file: None,
    }
}

fn db_tab() -> SavedTab {
    SavedTab::Db {
        conn_id: "warehouse".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
    }
}

fn tab_with_editors(texts: &[&str]) -> TabState {
    let mut tab = TabState::new(Default::default());
    tab.sql_panel_open = true;
    for (i, text) in texts.iter().enumerate() {
        if i > 0 {
            tab.add_sql_pane();
        }
        tab.sql.query = (*text).into();
    }
    tab
}

fn app_with_blank_tab() -> OctaApp {
    let mut app = app_with_files(&[]);
    app.tabs.push(TabState::new(Default::default()));
    app
}

#[test]
fn editor_text_survives_the_session_file_as_written() {
    let query = "select 'it''s'\n  from \"a table\" -- note\n\twhere x = \"\"\"\n";
    let file_ = SessionFile {
        sql_editors: vec![SavedEditors {
            tab: Some(db_tab()),
            editors: vec![
                SavedEditor {
                    query: query.into(),
                    file: Some("/q/a.sql".into()),
                },
                editor("select 2"),
            ],
        }],
        ..Default::default()
    };
    let back: SessionFile = toml::from_str(&toml::to_string_pretty(&file_).unwrap()).unwrap();
    assert_eq!(back.sql_editors, file_.sql_editors);
}

#[test]
fn every_editor_is_saved_in_order_and_blank_ones_are_left_out() {
    let mut tab = tab_with_editors(&["select 1", "   ", "select 3"]);
    tab.table.source_path = Some("/a.csv".into());
    // The active pane sits in `tab.sql`, not in its slot.
    tab.activate_sql_pane(0);
    let saved = saved_editors(&tab).unwrap();
    assert_eq!(saved.tab, Some(file("/a.csv")));
    assert_eq!(saved.editors, [editor("select 1"), editor("select 3")]);
}

#[test]
fn an_editor_with_a_file_is_saved_with_it_even_when_empty() {
    let mut tab = tab_with_editors(&[""]);
    tab.put_sql_file("/q/a.sql".into(), String::new(), false);
    let saved = saved_editors(&tab).unwrap();
    assert_eq!(saved.editors[0].file.as_deref(), Some("/q/a.sql"));
}

#[test]
fn a_panel_the_user_closed_is_not_saved_and_neither_is_an_empty_one() {
    let mut tab = tab_with_editors(&["select 1"]);
    tab.sql_panel_open = false;
    assert!(saved_editors(&tab).is_none());
    assert!(saved_editors(&tab_with_editors(&[""])).is_none());
}

#[test]
fn keep_open_off_means_an_editor_closes_with_its_tab_and_octa_too() {
    let mut app = app_with_files(&[]);
    app.tabs.push(tab_with_editors(&["select 1"]));
    assert_eq!(app.editors_to_save().len(), 1);
    app.settings.sql_keep_open = false;
    assert!(app.editors_to_save().is_empty());
}

#[test]
fn quitting_before_the_editors_landed_saves_them_again() {
    let mut app = app_with_blank_tab();
    let waiting = SavedEditors {
        tab: Some(file("/a.csv")),
        editors: vec![editor("select 1")],
    };
    app.pending_editors = vec![waiting.clone()];
    assert_eq!(app.editors_to_save(), [waiting]);
}

#[test]
fn editors_go_back_to_their_own_tab_when_it_reopens() {
    let mut app = app_with_files(&["/a.csv", "/b.csv"]);
    app.pending_editors = vec![SavedEditors {
        tab: Some(file("/b.csv")),
        editors: vec![editor("select 1"), editor("select 2")],
    }];
    app.adopt_session_editors();
    assert!(app.pending_editors.is_empty());
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[1].sql_queries(), ["select 1", "select 2"]);
    assert!(app.tabs[1].sql_panel_open);
    assert!(!app.tabs[0].sql_panel_open);
}

#[test]
fn editors_wait_while_their_tab_may_still_open() {
    let mut app = app_with_blank_tab();
    app.pending_editors = vec![SavedEditors {
        tab: Some(file("/a.csv")),
        editors: vec![editor("select 1")],
    }];
    app.pending_open_queue.push_back("/a.csv".into());
    app.adopt_session_editors();
    assert_eq!(app.pending_editors.len(), 1);
    assert!(!app.tabs[0].sql_panel_open);

    // The file arrives on the blank tab, as a real load would put it there.
    app.tabs[0].table.source_path = Some("/a.csv".into());
    app.pending_open_queue.clear();
    app.adopt_session_editors();
    assert!(app.pending_editors.is_empty());
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].sql.query, "select 1");
}

/// The user's rule: an editor comes back whatever its tab did, and bringing it
/// back reaches for nothing. A database tab reopens only under its own switch.
#[test]
fn editors_of_a_database_tab_that_stays_closed_come_back_without_connecting() {
    let mut app = app_with_blank_tab();
    app.pending_editors = vec![SavedEditors {
        tab: Some(db_tab()),
        editors: vec![editor("select * from s.t")],
    }];
    app.adopt_session_editors();

    assert!(app.pending_editors.is_empty());
    assert_eq!(app.tabs.len(), 1);
    let tab = &app.tabs[0];
    assert_eq!(tab.sql.query, "select * from s.t");
    assert!(tab.sql_panel_open);
    assert!(tab.db_origin.is_none());
    assert!(tab.sql_target.is_none(), "Run stays local until picked");
    assert!(app.long_tasks.running().is_none());
    assert!(app.db_browser.pending_open.lock().unwrap().is_empty());
}

#[test]
fn editors_with_no_tab_to_go_to_never_land_on_someone_elses_file() {
    let mut app = app_with_files(&["/a.csv"]);
    app.pending_editors = vec![
        SavedEditors {
            tab: None,
            editors: vec![editor("select 1")],
        },
        SavedEditors {
            tab: None,
            editors: vec![editor("select 2")],
        },
    ];
    app.adopt_session_editors();
    assert_eq!(app.tabs.len(), 3, "each source tab's editors stay together");
    assert_eq!(app.active_tab, 0, "the tab on screen stays on screen");
    assert!(app.tabs[0].sql.query.is_empty());
    assert_eq!(app.tabs[1].sql.query, "select 1");
    assert_eq!(app.tabs[2].sql.query, "select 2");
}

#[cfg(unix)]
#[test]
fn the_session_file_is_private_since_a_query_can_hold_a_secret() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    write_session_file(&path, "new");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn adopt_builds_group_and_tiles_once_members_are_open() {
    let mut app = app_with_files(&["/a.csv", "/x.csv", "/b.csv"]);
    app.pending_layout = Some(layout_with("/a.csv", "/b.csv"));
    app.adopt_session_layout();
    assert!(app.pending_layout.is_none());
    assert_eq!(app.tab_groups.len(), 1);
    assert!(app.tab_groups[0].folded);
    let g = app.tab_groups[0].id;
    let at = app.group_members(g);
    assert_eq!(at.len(), 2);
    assert_eq!(at[1], at[0] + 1);
    assert_eq!(app.tile_sets.len(), 1);
    assert_eq!(app.tile_sets[0].xs, vec![0.3]);
    assert_eq!(app.tabs.iter().filter(|t| t.tile.is_some()).count(), 2);
    // The round trip: what was adopted is what would be saved again.
    assert_eq!(app.session_layout(), layout_with("/a.csv", "/b.csv"));
}

#[test]
fn adopt_waits_while_loading_and_drops_a_lone_member_after() {
    let mut app = app_with_files(&["/a.csv"]);
    app.pending_layout = Some(layout_with("/a.csv", "/b.csv"));
    app.pending_open_queue.push_back("/b.csv".into());
    app.adopt_session_layout();
    assert!(app.pending_layout.is_some());
    app.pending_open_queue.clear();
    app.adopt_session_layout();
    assert!(app.pending_layout.is_none());
    assert!(app.tab_groups.is_empty() && app.tile_sets.is_empty());
}

#[test]
fn quitting_before_the_layout_is_adopted_saves_it_again() {
    let mut app = app_with_files(&["/a.csv"]);
    app.pending_layout = Some(layout_with("/a.csv", "/b.csv"));
    assert_eq!(app.layout_to_save(), layout_with("/a.csv", "/b.csv"));
    app.pending_layout = None;
    assert_eq!(app.layout_to_save(), app.session_layout());
}

/// A tab's own colour comes back with Reopen last session, also on a tab in
/// no group, and the same file open twice keeps each copy's colour.
#[test]
fn tab_colours_come_back_with_the_session() {
    let mut before = app_with_files(&["/a.csv", "/b.csv", "/a.csv"]);
    before.tabs[0].colour = Some(MarkColor::Blue);
    before.tabs[2].colour = Some(MarkColor::ALL[0]);
    let layout = before.session_layout();
    assert_eq!(layout.colours.len(), 2);
    let back: SessionLayout = toml::from_str(&toml::to_string_pretty(&layout).unwrap()).unwrap();

    let mut after = app_with_files(&["/a.csv", "/b.csv", "/a.csv"]);
    after.pending_layout = Some(back);
    after.adopt_session_layout();
    let colours: Vec<_> = after.tabs.iter().map(|t| t.colour).collect();
    assert_eq!(
        colours,
        [Some(MarkColor::Blue), None, Some(MarkColor::ALL[0])]
    );
    assert!(after.tab_groups.is_empty());
}
