use super::*;
use octa::data::ColumnInfo;
use std::collections::HashSet;

fn tab_with_columns(names: &[&str]) -> TabState {
    let mut tab = TabState::new(data::SearchMode::Plain);
    tab.table.columns = names
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".into(),
        })
        .collect();
    assert!(tab.sync_column_keys(), "first sync records the names");
    tab
}

fn rows(n: usize) -> Vec<Vec<data::CellValue>> {
    vec![vec![data::CellValue::Int(1)]; n]
}

fn db_tab() -> TabState {
    let mut tab = TabState::new(data::SearchMode::Plain);
    tab.db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "c".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
        identity: None,
    });
    tab.table.rows = rows(100);
    tab
}

/// A database tab marks "more may exist" with `total_rows` equal to the
/// rows it holds, which `is_partial` reads as complete.
#[test]
fn a_database_tab_with_more_pages_has_more() {
    let mut tab = db_tab();
    tab.table.total_rows = Some(100);
    assert!(!tab.table.is_partial());
    assert!(tab.source_has_more());
}

#[test]
fn a_database_tab_read_to_the_end_has_no_more() {
    let mut tab = db_tab();
    tab.table.total_rows = None;
    assert!(!tab.source_has_more());
}

#[test]
fn a_capped_file_has_more_and_a_whole_one_does_not() {
    let mut tab = TabState::new(data::SearchMode::Plain);
    tab.table.rows = rows(100);
    tab.table.total_rows = Some(110);
    assert!(tab.source_has_more());
    tab.table.total_rows = None;
    assert!(!tab.source_has_more());
    // A file tab whose count equals its rows is complete; only a database
    // tab reads that as a flag.
    tab.table.total_rows = Some(100);
    assert!(!tab.source_has_more());
}

#[test]
fn sheets_of_one_workbook_get_tabs_that_can_be_told_apart() {
    // Three sheets of one file are three tabs, and the file name is the
    // same on all three, so the sheet has to be in the label.
    let mut tab = TabState::new(data::SearchMode::Plain);
    tab.table.source_path = Some("/tmp/book.xlsx".to_string());
    assert_eq!(tab.title_display(), "book.xlsx");

    tab.sheet_name = Some("Costs".to_string());
    assert_eq!(tab.title_display(), "book.xlsx - Costs");

    // The modified marker still has to survive - that is why this is its
    // own field and not `custom_tab_label`, which swallows it.
    tab.table.insert_row(0);
    assert_eq!(tab.title_display(), "book.xlsx - Costs *");

    // A name the user typed themselves still wins over both.
    tab.user_tab_name = Some("mine".to_string());
    assert_eq!(tab.title_display(), "mine *");
}

#[test]
fn a_database_tab_saves_in_place_even_without_a_source_path() {
    // A live-database tab deliberately has no file on disk, so any save
    // path that gates on `source_path` alone silently routes it to the
    // Save As picker instead of the write-back dialog.
    let mut tab = TabState::new(data::SearchMode::Plain);
    assert!(!tab.saves_in_place(), "a blank tab has nowhere to save");

    tab.table.source_path = Some("/tmp/x.csv".to_string());
    assert!(tab.saves_in_place(), "a file-backed tab saves to its file");

    let mut db = TabState::new(data::SearchMode::Plain);
    db.db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "conn-1".to_string(),
        catalog: None,
        schema: "public".to_string(),
        table: "people".to_string(),
        identity: Some(octa::db::write_back::RowIdentity::Key(vec![
            "id".to_string(),
        ])),
    });
    assert!(
        db.table.source_path.is_none(),
        "db tabs never carry a source path"
    );
    assert!(
        db.saves_in_place(),
        "Ctrl+S on a db tab must reach the write-back dialog, not Save As"
    );
}

#[test]
fn deleting_a_column_renumbers_the_filters_that_follow_it() {
    let mut tab = tab_with_columns(&["id", "name", "status"]);
    tab.column_filters
        .insert(2, HashSet::from(["active".to_string()]));
    tab.hidden_columns.insert(1);
    tab.predicate_filters
        .push(octa::data::predicate_filter::PredicateFilter {
            col: 2,
            op: octa::data::conditional_format::CondOp::Eq,
            value: "active".into(),
            case_sensitive: false,
        });

    tab.table.columns.remove(0); // the user deletes `id`
    assert!(tab.sync_column_keys());

    assert!(
        tab.column_filters.contains_key(&1),
        "filter follows `status`"
    );
    assert!(tab.hidden_columns.contains(&0), "hidden follows `name`");
    assert_eq!(tab.predicate_filters[0].col, 1);
}

#[test]
fn moving_a_column_moves_its_filter() {
    let mut tab = tab_with_columns(&["id", "name", "status"]);
    tab.column_filters
        .insert(0, HashSet::from(["7".to_string()]));
    tab.table.move_column(0, 2); // id goes last
    assert!(tab.sync_column_keys());
    assert!(tab.column_filters.contains_key(&2));
}

#[test]
fn a_column_that_is_gone_loses_its_filter() {
    let mut tab = tab_with_columns(&["id", "status"]);
    tab.column_filters
        .insert(1, HashSet::from(["active".to_string()]));
    tab.table.columns.pop(); // `status` deleted
    assert!(tab.sync_column_keys());
    assert!(
        tab.column_filters.is_empty(),
        "a filter must never survive onto another column"
    );
}

#[test]
fn an_unchanged_column_list_is_a_no_op() {
    let mut tab = tab_with_columns(&["id", "status"]);
    assert!(!tab.sync_column_keys());
}

/// The welcome tab is asked first (no file, so no repository) and the
/// first file opened reuses that tab. The answer must follow the file:
/// it once stuck at "no repository" and greyed out Cell history.
#[test]
fn git_location_follows_the_file_when_a_blank_tab_is_reused() {
    let dir = tempfile::tempdir().unwrap();
    let git = |a: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(a)
            .output()
    };
    let Ok(out) = git(&["init", "-q"]) else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let file = dir.path().join("p.csv");
    std::fs::write(&file, "a\n1\n").unwrap();
    let mut tab = TabState::new(octa::data::SearchMode::default());
    assert_eq!(tab.git_location(), None, "the blank tab has no file");
    tab.table.source_path = Some(file.to_string_lossy().into_owned());
    assert_eq!(
        tab.git_location().map(|(_, rel)| rel),
        Some("p.csv".to_string())
    );
}
