use super::*;

#[test]
fn a_cloud_tab_refreshes_the_object_not_its_downloaded_copy() {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    tab.table.source_path = Some("/tmp/octa-download-123.csv".into());
    tab.cloud_origin = Some(crate::app::state::CloudOrigin {
        conn_id: "s3".into(),
        key: "data/sales.csv".into(),
    });
    assert_eq!(
        tab.refresh_source().as_deref(),
        Some("cloud:s3:data/sales.csv")
    );
}

#[test]
fn a_file_tab_refreshes_its_path_and_a_computed_tab_nothing() {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    assert_eq!(tab.refresh_source(), None);
    tab.table.source_path = Some("/data/a.csv".into());
    assert_eq!(tab.refresh_source().as_deref(), Some("/data/a.csv"));
}

#[test]
fn only_the_awaited_source_claims_the_tab() {
    let target = ReloadTarget {
        tab: 2,
        source: "/data/a.csv".into(),
        discard_edits: false,
    };
    assert!(target.claims("/data/a.csv", 3));
    // Another file opened meanwhile gets a tab of its own.
    assert!(!target.claims("/data/b.csv", 3));
    // The tab was closed in between: nothing to land in.
    assert!(!target.claims("/data/a.csv", 2));
}

#[test]
fn closing_a_tab_keeps_the_indices_on_the_same_tabs() {
    assert_eq!(shifted(1, 3), Some(1));
    assert_eq!(shifted(3, 3), None);
    assert_eq!(shifted(4, 3), Some(3));
}

fn app() -> OctaApp {
    OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    )
}

#[test]
fn a_failed_read_frees_the_refresh_so_the_next_one_can_start() {
    let mut app = app();
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: db_source("c", None, "public", "t"),
        discard_edits: false,
    });
    // A file failing does not touch a database refresh.
    app.fail_reload("/data/a.csv");
    assert!(app.reload_target.is_some());
    app.fail_reload("db:");
    assert!(app.reload_target.is_none());
}

#[test]
fn a_read_landing_on_an_edited_tab_is_dropped_not_placed() {
    let mut app = app();
    app.tabs[0].table = octa::data::DataTable::empty();
    app.tabs[0].raw_content_modified = true;
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: "/data/a.csv".into(),
        discard_edits: false,
    });
    assert_eq!(app.take_reload_slot("/data/a.csv"), ReloadSlot::Dropped);
    assert!(app.reload_target.is_none());
    assert_eq!(app.take_reload_slot("/data/a.csv"), ReloadSlot::NotClaimed);
    assert!(app.status_message.is_some());
}

#[test]
fn a_confirmed_discard_replaces_an_edited_tab() {
    let mut app = app();
    app.tabs[0].raw_content_modified = true;
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: "/data/a.csv".into(),
        discard_edits: true,
    });
    assert_eq!(app.take_reload_slot("/data/a.csv"), ReloadSlot::Replaced);
}

#[test]
fn a_file_failure_does_not_clear_a_sibling_file_target() {
    let mut app = app();
    app.reload_target = Some(ReloadTarget {
        tab: 0,
        source: "/d/a.csv.bak".into(),
        discard_edits: false,
    });
    app.fail_reload("/d/a.csv");
    assert!(app.reload_target.is_some());
}

fn app_with_tabs(n: usize) -> OctaApp {
    let mut app = app();
    while app.tabs.len() < n {
        app.tabs
            .push(TabState::new(app.settings.default_search_mode));
    }
    app
}

#[test]
fn move_tab_keeps_active_and_held_indices_on_their_tabs() {
    let mut app = app_with_tabs(4);
    let ids: Vec<_> = app.tabs.iter().map(|t| t.id).collect();
    app.active_tab = 0;
    app.pending_close_tab = Some(2);
    app.tab_multi_selection = [0, 3].into_iter().collect();
    app.move_tab(0, 2); // [b c a d]
    assert_eq!(
        app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
        [ids[1], ids[2], ids[0], ids[3]]
    );
    assert_eq!(app.active_tab, 2);
    assert_eq!(app.pending_close_tab, Some(1));
    assert_eq!(app.tab_multi_selection, [2, 3].into_iter().collect());
}

#[test]
fn reorder_tabs_follows_the_order_and_ignores_a_bad_one() {
    let mut app = app_with_tabs(3);
    let ids: Vec<_> = app.tabs.iter().map(|t| t.id).collect();
    app.active_tab = 2;
    app.reorder_tabs(&[0, 0, 1]); // not a permutation
    assert_eq!(app.tabs[1].id, ids[1]);
    app.reorder_tabs(&[2, 0, 1]);
    assert_eq!(
        app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
        [ids[2], ids[0], ids[1]]
    );
    assert_eq!(app.active_tab, 0);
}

fn save_as(tab: usize) -> crate::app::state::SaveAsPrompt {
    crate::app::state::SaveAsPrompt {
        tab_idx: tab,
        path: "/out/c.csv".into(),
        ask_rows: false,
        ask_open: true,
        filtered: false,
        open: false,
        visible: 0,
        total: 0,
        csv: None,
        duration_note: false,
        upload: None,
    }
}

fn number_warning(tab: usize) -> crate::app::state::NumberWarning {
    crate::app::state::NumberWarning {
        tab_idx: tab,
        entries: Vec::new(),
    }
}

fn date_picker(tab: usize) -> crate::app::state::DateAmbiguity {
    crate::app::state::DateAmbiguity {
        tab_idx: tab,
        col_idx: 0,
        col_name: "when".into(),
        samples: Vec::new(),
        date_candidates: Vec::new(),
        datetime_candidates: Vec::new(),
    }
}

#[test]
fn a_banner_and_a_save_prompt_follow_a_moved_tab_and_go_when_it_closes() {
    let mut app = app_with_tabs(3);
    app.pending_number_warning = Some(number_warning(2));
    app.pending_save_as = Some(save_as(2));
    app.move_tab(2, 0); // [c a b]
    assert_eq!(app.pending_number_warning.as_ref().unwrap().tab_idx, 0);
    assert_eq!(app.pending_save_as.as_ref().unwrap().tab_idx, 0);
    app.close_tab(1); // a: both stay on c
    assert_eq!(app.pending_number_warning.as_ref().unwrap().tab_idx, 0);
    assert_eq!(app.pending_save_as.as_ref().unwrap().tab_idx, 0);
    app.close_tab(0); // c itself
    assert!(app.pending_number_warning.is_none());
    assert!(app.pending_save_as.is_none());
}

#[test]
fn grouping_that_reorders_tabs_keeps_a_date_picker_on_its_tab() {
    // The reviewer's case: a+c grouped moves c beside a, so a warning still
    // naming position 2 would Dismiss into b.
    let mut app = app_with_tabs(3);
    let c = app.tabs[2].id;
    app.pending_date_pickers.push_back(date_picker(2));
    app.pending_number_warning = Some(number_warning(2));
    app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 2]); // [a c b]
    assert_eq!(app.tabs[1].id, c);
    assert_eq!(app.pending_date_pickers[0].tab_idx, 1);
    assert_eq!(app.pending_number_warning.as_ref().unwrap().tab_idx, 1);
    app.close_tab(1);
    assert!(app.pending_date_pickers.is_empty());
    assert!(app.pending_number_warning.is_none());
}

#[test]
fn dialogs_follow_their_tabs_and_lose_only_a_closed_one() {
    let mut app = app_with_tabs(4);
    app.join_dialog = Some(crate::app::state::JoinState {
        left_tab: 0,
        right_tab: 3,
        conds: Vec::new(),
        join_type: octa::data::join::JoinType::Left,
        spatial: Some(crate::app::state::SpatialDraft {
            nearest: false,
            within_km_text: String::new(),
            layers: [1, 2].into_iter().collect(),
        }),
        error: None,
        size: Default::default(),
    });
    app.open_workbook_dialog();
    app.workbook_dialog.as_mut().unwrap().names =
        vec!["a".into(), "b".into(), "c".into(), "d".into()];
    app.move_tab(3, 0); // [d a b c]
    let join = app.join_dialog.as_ref().unwrap();
    assert_eq!((join.left_tab, join.right_tab), (1, 0));
    let layers: Vec<usize> = join
        .spatial
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .copied()
        .collect();
    assert_eq!(layers, [2, 3]);
    assert_eq!(
        app.workbook_dialog.as_ref().unwrap().names,
        ["d", "a", "b", "c"]
    );
    app.close_tab(2); // b: a layer goes, the join stays
    let join = app.join_dialog.as_ref().unwrap();
    let layers: Vec<usize> = join
        .spatial
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .copied()
        .collect();
    assert_eq!(layers, [2]);
    assert_eq!(app.workbook_dialog.as_ref().unwrap().names, ["d", "a", "c"]);
    app.close_tab(0); // d, the right side: the join cannot work without it
    assert!(app.join_dialog.is_none());
}

#[test]
fn a_per_tab_list_follows_its_tabs() {
    let mut v = vec!['a', 'b', 'c'];
    // c moved to the front, then a fourth tab not in the list yet.
    follow_tabs(&mut v, &|i| [1, 2, 0].get(i).copied(), 4, |_| '-');
    assert_eq!(v, ['c', 'a', 'b', '-']);
    follow_tabs(&mut v, &|i| shifted(i, 1), 3, |_| '?');
    assert_eq!(v, ['c', 'b', '-']);
}
