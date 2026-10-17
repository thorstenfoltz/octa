use super::*;

fn tab() -> TabState {
    TabState::new(Default::default())
}

fn typed(tab: &mut TabState, texts: &[&str]) {
    tab.set_sql_editors(texts.iter().map(|t| (t.to_string(), None)).collect());
}

fn queries_of(t: &TabState) -> Vec<String> {
    t.sql_editors().into_iter().map(|(q, _)| q).collect()
}

fn file(path: &str, saved: &str) -> SqlFile {
    SqlFile {
        path: path.into(),
        saved: saved.into(),
        crlf: false,
    }
}

#[test]
fn activating_swaps_panes_without_losing_text() {
    let mut t = tab();
    typed(&mut t, &["a", "b", "c"]);
    t.activate_sql_pane(2);
    assert_eq!(t.sql.query, "c");
    t.activate_sql_pane(0);
    assert_eq!(t.sql.query, "a");
    assert_eq!(queries_of(&t), ["a", "b", "c"]);
}

#[test]
fn closing_the_active_pane_activates_a_neighbour_and_keeps_the_rest() {
    let mut t = tab();
    typed(&mut t, &["a", "b", "c"]);
    t.activate_sql_pane(1);
    t.close_sql_pane(1);
    assert_eq!(queries_of(&t), ["a", "c"]);
    assert_eq!(t.sql.query, "a");
    // Closing one left of the active pane shifts its index.
    t.activate_sql_pane(1);
    t.close_sql_pane(0);
    assert_eq!(t.sql_active_pane, 0);
    assert_eq!(t.sql.query, "c");
    // The last pane stays.
    t.close_sql_pane(0);
    assert_eq!(t.sql_pane_count(), 1);
}

#[test]
fn a_pane_is_found_by_id_wherever_it_sits() {
    let mut t = tab();
    typed(&mut t, &["a", "b"]);
    let id_b = t.sql_pane_id(1);
    t.sql_pane_by_id_mut(id_b).expect("b").query.push('!');
    assert_eq!(queries_of(&t), ["a", "b!"]);
    assert!(t.has_sql_pane(id_b));
    assert!(!t.has_sql_pane(0), "the placeholder is never a pane");
}

#[test]
fn a_closed_tabs_queries_land_only_in_blank_editors() {
    let mut t = tab();
    t.adopt_sql_editors(vec![
        ("select 1".into(), None),
        ("  ".into(), None),
        ("select 2".into(), None),
    ]);
    assert_eq!(queries_of(&t), ["select 1", "select 2"]);

    let mut busy = tab();
    busy.sql.query = "mine".into();
    busy.adopt_sql_editors(vec![("select 1".into(), None)]);
    assert_eq!(queries_of(&busy), ["mine"]);
}

#[test]
fn starting_a_run_clears_what_the_last_one_showed() {
    let mut p = SqlPane::new();
    p.error = Some("boom".into());
    p.result_total = Some(3);
    p.start_run();
    assert!(p.error.is_none() && p.result.is_none() && p.result_total.is_none());
    assert!(p.running_since.is_some());
}

#[test]
fn a_plan_fills_the_result_grid_and_a_run_clears_it() {
    let mut pane = SqlPane::new();
    assert_eq!(pane.plan_view, PlanView::Tree);
    let plan = octa::sql::explain::Plan {
        steps: vec![octa::sql::explain::PlanStep {
            depth: 0,
            kind: octa::sql::explain::StepKind::ReadTable,
            server_name: "SEQ_SCAN".into(),
            table: Some("data".into()),
            estimated_rows: Some(3.0),
            detail: String::new(),
            fact: None,
        }],
        ..Default::default()
    };
    pane.plan_view = PlanView::Server;
    pane.show_plan(plan);
    assert!(pane.plan.is_some());
    assert_eq!(pane.result.as_ref().map(|t| t.row_count()), Some(1));
    // The chosen view outlives the plan it was chosen for.
    pane.start_run();
    assert!(pane.plan.is_none());
    assert_eq!(pane.plan_view, PlanView::Server);
}

#[test]
fn a_file_lands_in_a_blank_editor_or_beside_a_busy_one() {
    let mut t = tab();
    t.put_sql_file("a.sql".into(), "select 1".into(), false);
    assert_eq!(t.sql_pane_count(), 1);
    assert_eq!(t.sql.query, "select 1");
    t.put_sql_file("b.sql".into(), "select 2".into(), false);
    assert_eq!(queries_of(&t), ["select 1", "select 2"]);
    assert_eq!(t.sql_active_pane, 1);
    // An emptied editor that still has its file is not blank.
    t.sql.query.clear();
    t.put_sql_file("c.sql".into(), "select 3".into(), false);
    assert_eq!(t.sql_pane_count(), 3);
}

#[test]
fn an_editor_is_unsaved_while_its_text_differs_from_the_file() {
    let mut t = tab();
    assert!(!t.sql.is_dirty(), "no file, nothing to save");
    t.put_sql_file("a.sql".into(), "select 1".into(), false);
    assert!(!t.sql.is_dirty());
    t.sql.query.push('0');
    assert!(t.sql.is_dirty());
    t.sql.query.pop();
    assert!(!t.sql.is_dirty(), "typing it back is not a change");
}

#[test]
fn a_closed_tabs_editors_bring_their_files() {
    let mut t = tab();
    t.adopt_sql_editors(vec![
        (String::new(), Some(file("a.sql", ""))),
        ("  ".into(), None),
    ]);
    assert_eq!(t.sql_pane_count(), 1);
    assert_eq!(
        t.sql.file.as_ref().map(|f| f.path.clone()),
        Some(PathBuf::from("a.sql"))
    );
    // A tab whose only editor has a file is not blank, so it adopts nothing.
    t.adopt_sql_editors(vec![("select 9".into(), None)]);
    assert_eq!(queries_of(&t), [""]);
}

#[test]
fn sql_files_round_trip_their_line_endings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("q.sql");
    std::fs::write(&path, "\u{feff}select 1\r\nfrom t\r\n").unwrap();
    let (text, crlf) = read_sql_file(&path).unwrap();
    assert_eq!(text, "select 1\nfrom t\n");
    assert!(crlf);
    write_sql_file(&path, &text, crlf).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"select 1\r\nfrom t\r\n");

    write_sql_file(&path, "select 2\n", false).unwrap();
    assert_eq!(
        read_sql_file(&path).unwrap(),
        ("select 2\n".to_string(), false)
    );
}

#[test]
fn a_file_that_is_not_text_says_which_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bin.sql");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    let err = format!("{:#}", read_sql_file(&path).unwrap_err());
    assert!(err.contains("bin.sql"), "{err}");
}
