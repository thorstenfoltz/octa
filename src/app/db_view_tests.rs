use super::*;
use octa::data::conditional_format::CondOp;
use octa::data::{CellValue, ColumnInfo, SearchResultMode};
use octa::db::{DEFAULT_QUERY_TIMEOUT_SECS, DbAuth, DbEngine};

pub(crate) fn conn() -> DbConnection {
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
pub(crate) fn db_tab() -> TabState {
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

#[test]
fn only_a_database_tab_with_more_rows_or_a_view_goes_to_the_server() {
    let conns = [conn()];
    let tab = db_tab();
    assert!(view_source_for(&tab, true, &conns).is_some());
    assert!(
        view_source_for(&tab, false, &conns).is_none(),
        "setting off"
    );
    assert!(
        view_source_for(&tab, true, &[]).is_none(),
        "connection gone"
    );
    let mut complete = db_tab();
    complete.table.total_rows = None;
    assert!(view_source_for(&complete, true, &conns).is_none());
    complete.server_view = Some(ServerView::default());
    assert!(
        view_source_for(&complete, true, &conns).is_some(),
        "a view stays on the server"
    );
    let mut file = TabState::new(SearchMode::Plain);
    file.table.total_rows = Some(usize::MAX);
    assert!(
        view_source_for(&file, true, &conns).is_none(),
        "the non-goal"
    );
}

#[test]
fn filters_split_into_the_server_part_and_the_rest() {
    let mut tab = db_tab();
    tab.column_filters
        .insert(0, ["b".to_string(), "a".to_string()].into_iter().collect());
    tab.predicate_filters = vec![
        PredicateFilter {
            col: 1,
            op: CondOp::Gt,
            value: "5".into(),
            case_sensitive: false,
        },
        PredicateFilter {
            col: 0,
            op: CondOp::Gt,
            value: "x".into(),
            case_sensitive: false,
        },
    ];
    tab.search_text = "ab".into();
    let (view, local) = wanted_view(&tab, true);
    assert_eq!(view.filters.len(), 3);
    assert_eq!(
        view.filters[0],
        ViewFilter::Values {
            column: "name".into(),
            values: vec!["a".into(), "b".into()]
        }
    );
    assert!(matches!(view.filters[2], ViewFilter::Contains { .. }));
    assert_eq!(
        local,
        LocalLeftovers {
            search: false,
            predicates: vec![1]
        }
    );

    tab.search_mode = SearchMode::Regex;
    let (view, local) = wanted_view(&tab, true);
    assert_eq!(view.filters.len(), 2);
    assert!(local.search);

    tab.search_mode = SearchMode::Wildcard;
    tab.search_whole_word = true;
    assert!(wanted_view(&tab, true).1.search, "whole word stays local");
    assert!(
        !wanted_view(&tab, false).1.search,
        "highlight mode hides nothing"
    );
}

#[test]
fn the_server_applies_filters_unless_the_user_kept_the_loaded_rows() {
    let conns = [conn()];
    let mut tab = db_tab();
    assert!(server_leftovers(&tab, true, &conns, SearchResultMode::Filter).is_some());
    tab.view_hold = Some(ViewHold::LoadedRows(ServerView::default()));
    assert!(server_leftovers(&tab, true, &conns, SearchResultMode::Filter).is_none());
}

#[test]
fn a_snapshot_puts_the_settings_back() {
    let mut tab = db_tab();
    tab.search_text = "x".into();
    tab.server_sort = vec![SortKey {
        column: "n".into(),
        ascending: false,
        text: false,
    }];
    let snap = ViewUiSnapshot::of(&tab);
    tab.search_text.clear();
    tab.server_sort.clear();
    tab.column_filters
        .insert(1, HashSet::from(["1".to_string()]));
    snap.restore(&mut tab);
    assert_eq!(ViewUiSnapshot::of(&tab), snap);
    assert!(tab.filter_dirty);
}

#[test]
fn an_empty_value_set_and_a_stale_search_scope_match_local() {
    // Select none, then Apply: a filter that hides every row, not no filter.
    let mut tab = db_tab();
    tab.column_filters.insert(0, Default::default());
    let (view, _) = wanted_view(&tab, false);
    assert_eq!(
        view.filters,
        vec![ViewFilter::Values {
            column: "name".into(),
            values: vec![]
        }]
    );
    // A scope past the last column searches every column, as in memory.
    tab.column_filters.clear();
    tab.search_text = "ab".into();
    tab.search_scope_col = Some(99);
    let (view, _) = wanted_view(&tab, true);
    let all = tab.table.col_count();
    assert!(
        matches!(&view.filters[0], ViewFilter::Contains { columns, .. } if columns.len() == all),
        "{view:?}"
    );
}

fn page(n: usize) -> octa::data::DataTable {
    let mut t = octa::data::DataTable::empty();
    t.columns = db_tab().table.columns;
    t.rows = (0..n)
        .map(|i| vec![CellValue::String(format!("r{i}")), CellValue::Int(i as i64)])
        .collect();
    t
}

#[test]
fn a_page_of_the_view_replaces_the_loaded_rows() {
    use octa::data::MarkKey;
    let mut tab = db_tab();
    tab.table.row_offset = 40;
    tab.table.sort_rows_by_columns(&[(1, false)]); // an undo entry, structural change
    tab.table
        .marks
        .insert(MarkKey::Row(0), octa::data::MarkColor::Red);
    tab.table
        .marks
        .insert(MarkKey::Column(1), octa::data::MarkColor::Red);
    tab.table_state.selected_cell = Some((0, 0));
    let view = ServerView {
        order: vec![SortKey {
            column: "n".into(),
            ascending: true,
            text: false,
        }],
        filters: vec![],
        derived: Vec::new(),
    };
    tab.apply_view_page(view.clone(), page(3), 3);
    assert_eq!(tab.table.rows.len(), 3);
    assert_eq!(tab.table.row_offset, 0);
    assert_eq!(tab.table.total_rows, Some(3), "a full page may have more");
    assert!(tab.bg_can_load_more);
    assert!(tab.table.undo_stack.is_empty());
    assert!(!tab.table.is_modified());
    assert_eq!(tab.table.marks.len(), 1, "column marks stay, row marks go");
    assert!(tab.table_state.selected_cell.is_none());
    assert_eq!(tab.server_view, Some(view));
    assert!(
        tab.view_ui.is_none(),
        "the sync snapshots matching settings"
    );
    assert!(tab.table_state.selected_rows.is_empty());

    tab.apply_view_page(ServerView::default(), page(2), 3);
    assert_eq!(tab.table.total_rows, None, "a short page is the end");
    assert!(!tab.bg_can_load_more);
    assert_eq!(tab.server_view, None, "the plain table again");
}

#[test]
fn a_writable_tab_gets_a_fresh_baseline() {
    let mut tab = db_tab();
    crate::app::db_browser::baseline_db_meta(&mut tab.table, "t", "public");
    tab.apply_view_page(ServerView::default(), page(4), 10);
    let meta = tab.table.db_meta.as_ref().expect("baseline");
    assert_eq!(
        meta.row_tags,
        (0..4).map(|i| Some(i as i64)).collect::<Vec<_>>()
    );
    assert_eq!(meta.original.len(), 4);
}

#[test]
fn analyses_read_the_filtered_result() {
    let conns = [conn()];
    let mut tab = db_tab();
    let src = crate::app::pushdown::server_source_for(&tab, true, &conns).unwrap();
    assert!(src.filter.is_none());
    tab.server_view = Some(ServerView {
        order: vec![],
        filters: vec![ViewFilter::values("name", ["a".to_string()])],
        derived: Vec::new(),
    });
    let src = crate::app::pushdown::server_source_for(&tab, true, &conns).unwrap();
    assert!(src.from_sql().contains("WHERE"), "{}", src.from_sql());
}

#[test]
fn the_next_page_follows_the_view_with_the_key_as_tie_break() {
    let src = origin_source(&conn(), db_tab().db_origin.as_ref().unwrap());
    let view = ServerView {
        order: vec![SortKey {
            column: "name".into(),
            ascending: true,
            text: true,
        }],
        filters: vec![],
        derived: Vec::new(),
    };
    let key = octa::db::write_back::RowIdentity::Key(vec!["n".into()]);
    let sql = view_page_sql(&src, Some(&key), &view, 100, 200);
    assert!(
        sql.starts_with("SELECT * FROM \"public\".\"t\" ORDER BY"),
        "{sql}"
    );
    assert!(sql.ends_with(", \"n\" LIMIT 100 OFFSET 200"), "{sql}");
}

use std::time::{Duration, Instant};

fn sorted_view() -> ServerView {
    ServerView {
        order: vec![SortKey {
            column: "n".into(),
            ascending: true,
            text: false,
        }],
        filters: vec![],
        derived: Vec::new(),
    }
}

fn searched_view(s: &str) -> ServerView {
    ServerView {
        order: vec![],
        filters: vec![ViewFilter::Contains {
            columns: vec!["name".into()],
            needle: s.into(),
            case_sensitive: false,
        }],
        derived: Vec::new(),
    }
}

#[test]
fn a_tab_in_step_stays_idle_and_remembers_its_settings() {
    let mut tab = db_tab();
    let now = Instant::now();
    assert_eq!(
        sync_step(&mut tab, ServerView::default(), false, false, now),
        SyncStep::Idle
    );
    assert!(tab.view_ui.is_some());
}

#[test]
fn a_new_sort_queries_at_once_and_asks_with_unsaved_edits() {
    let now = Instant::now();
    let mut tab = db_tab();
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, false, now),
        SyncStep::Query(sorted_view())
    );
    assert_eq!(
        sync_step(&mut tab, sorted_view(), true, false, now),
        SyncStep::Ask(sorted_view())
    );
}

#[test]
fn typing_settles_before_a_query() {
    let t0 = Instant::now();
    let mut tab = db_tab();
    assert_eq!(
        sync_step(&mut tab, searched_view("ab"), false, false, t0),
        SyncStep::Settle(SETTLE)
    );
    assert!(matches!(
        sync_step(
            &mut tab,
            searched_view("ab"),
            false,
            false,
            t0 + Duration::from_millis(100)
        ),
        SyncStep::Settle(_)
    ));
    // Another keystroke starts the wait again.
    assert_eq!(
        sync_step(
            &mut tab,
            searched_view("abc"),
            false,
            false,
            t0 + Duration::from_millis(200)
        ),
        SyncStep::Settle(SETTLE)
    );
    assert_eq!(
        sync_step(
            &mut tab,
            searched_view("abc"),
            false,
            false,
            t0 + Duration::from_millis(700)
        ),
        SyncStep::Query(searched_view("abc"))
    );
}

#[test]
fn a_busy_or_refused_tab_waits() {
    let now = Instant::now();
    let mut tab = db_tab();
    tab.view_error = Some("refused".into());
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, false, now),
        SyncStep::Idle
    );
    tab.view_error = None;
    tab.loading_all = true;
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, false, now),
        SyncStep::Idle
    );
}

#[test]
fn kept_loaded_rows_hold_until_the_view_changes() {
    let now = Instant::now();
    let mut tab = db_tab();
    tab.view_hold = Some(ViewHold::LoadedRows(sorted_view()));
    assert_eq!(
        sync_step(&mut tab, sorted_view(), true, false, now),
        SyncStep::Idle
    );
    let other = ServerView {
        order: vec![SortKey {
            column: "n".into(),
            ascending: false,
            text: false,
        }],
        filters: vec![],
        derived: Vec::new(),
    };
    assert_eq!(
        sync_step(&mut tab, other.clone(), true, false, now),
        SyncStep::Ask(other)
    );
}

#[test]
fn save_first_waits_for_the_save_then_queries() {
    let now = Instant::now();
    let mut tab = db_tab();
    tab.view_hold = Some(ViewHold::AwaitSave(sorted_view()));
    assert_eq!(
        sync_step(&mut tab, sorted_view(), true, true, now),
        SyncStep::Idle,
        "saving"
    );
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, false, now),
        SyncStep::Query(sorted_view())
    );
    // A cancelled or failed save keeps the loaded rows instead of asking again.
    tab.view_hold = Some(ViewHold::AwaitSave(sorted_view()));
    assert_eq!(
        sync_step(&mut tab, sorted_view(), true, false, now),
        SyncStep::Idle
    );
    assert_eq!(tab.view_hold, Some(ViewHold::LoadedRows(sorted_view())));
}

#[test]
fn keeping_the_loaded_rows_sorts_them_here() {
    let mut tab = db_tab();
    tab.table.rows = vec![
        vec![CellValue::String("b".into()), CellValue::Int(2)],
        vec![CellValue::String("a".into()), CellValue::Int(1)],
    ];
    tab.hold_loaded_rows(sorted_view());
    assert_eq!(tab.table.rows[0][1], CellValue::Int(1));
    assert_eq!(tab.view_hold, Some(ViewHold::LoadedRows(sorted_view())));
}

#[test]
fn cancel_puts_the_settings_back() {
    let now = Instant::now();
    let mut tab = db_tab();
    assert_eq!(
        sync_step(&mut tab, ServerView::default(), false, false, now),
        SyncStep::Idle
    );
    tab.server_sort = sorted_view().order;
    tab.cancel_view_change(sorted_view());
    assert!(tab.server_sort.is_empty());
    // Without a snapshot there is nothing to return to: keep the loaded rows
    // rather than asking again every frame.
    let mut fresh = db_tab();
    fresh.cancel_view_change(sorted_view());
    assert_eq!(fresh.view_hold, Some(ViewHold::LoadedRows(sorted_view())));
}

#[test]
fn a_pending_save_holds_every_requery() {
    // Not only "Save first": a Save the user started (its confirmation may
    // still be open, and its Cancel keeps the edits) must not see its rows
    // replaced by a sort set meanwhile.
    let mut tab = db_tab();
    let now = Instant::now();
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, true, now),
        SyncStep::Idle
    );
    assert_eq!(
        sync_step(&mut tab, sorted_view(), false, false, now),
        SyncStep::Query(sorted_view())
    );
}

fn unsorted(tab: &mut TabState) {
    tab.table.rows = vec![
        vec![CellValue::String("b".into()), CellValue::Int(2)],
        vec![CellValue::String("a".into()), CellValue::Int(1)],
    ];
}

#[test]
fn every_fallback_hold_sorts_the_loaded_rows() {
    // A failed or cancelled Save first.
    let now = Instant::now();
    let mut tab = db_tab();
    unsorted(&mut tab);
    tab.view_hold = Some(ViewHold::AwaitSave(sorted_view()));
    sync_step(&mut tab, sorted_view(), true, false, now);
    assert_eq!(
        tab.table.rows[0][1],
        CellValue::Int(1),
        "sorted as the banner says"
    );
    // Cancel without a snapshot.
    let mut fresh = db_tab();
    unsorted(&mut fresh);
    fresh.cancel_view_change(sorted_view());
    assert_eq!(fresh.table.rows[0][1], CellValue::Int(1));
}

#[test]
fn cancel_puts_the_search_options_back_too() {
    let now = Instant::now();
    let mut tab = db_tab();
    sync_step(&mut tab, ServerView::default(), false, false, now);
    tab.search_case_sensitive = true;
    tab.search_whole_word = true;
    tab.search_mode = SearchMode::Regex;
    tab.search_scope_col = Some(1);
    tab.cancel_view_change(sorted_view());
    assert!(!tab.search_case_sensitive);
    assert!(!tab.search_whole_word);
    assert_eq!(tab.search_mode, SearchMode::Plain);
    assert_eq!(tab.search_scope_col, None);
}

#[test]
fn the_snapshot_follows_local_only_changes_while_in_step() {
    let now = Instant::now();
    let mut tab = db_tab();
    sync_step(&mut tab, ServerView::default(), false, false, now);
    // A highlight-mode search hides nothing, so the view stays in step.
    tab.search_text = "a".into();
    tab.filter_dirty = true;
    sync_step(&mut tab, ServerView::default(), false, false, now);
    tab.server_sort = sorted_view().order;
    tab.cancel_view_change(sorted_view());
    assert_eq!(
        tab.search_text, "a",
        "Cancel keeps what was already in step"
    );
    assert!(tab.server_sort.is_empty());
}

#[test]
fn a_server_sort_names_its_columns_and_moves_no_row() {
    let mut tab = db_tab();
    tab.table.rows = vec![
        vec![CellValue::String("b".into()), CellValue::Int(2)],
        vec![CellValue::String("a".into()), CellValue::Int(1)],
    ];
    tab.sort_on_server(&[(0, false), (1, true), (9, true)]);
    assert_eq!(
        tab.server_sort,
        vec![
            SortKey {
                column: "name".into(),
                ascending: false,
                text: true
            },
            SortKey {
                column: "n".into(),
                ascending: true,
                text: false
            },
        ]
    );
    assert_eq!(
        tab.table.rows[0][1],
        CellValue::Int(2),
        "rows stay put until the server answers"
    );
    assert!(tab.table.undo_stack.is_empty(), "a view is not an edit");
}

#[test]
fn a_value_list_is_sent_once_and_a_search_settles_first() {
    let t0 = Instant::now();
    let key = (1usize, String::new());
    assert_eq!(values_due(None, &key, t0), ValuesDue::SendNew);
    let searched = (1usize, "ab".to_string());
    assert_eq!(values_due(None, &searched, t0), ValuesDue::Start);
    let waiting = ServerValues {
        key: searched.clone(),
        task: None,
        result: None,
        since: t0,
    };
    assert!(matches!(
        values_due(Some(&waiting), &searched, t0 + Duration::from_millis(100)),
        ValuesDue::Wait(_)
    ));
    assert_eq!(
        values_due(Some(&waiting), &searched, t0 + SETTLE),
        ValuesDue::Send
    );
    let answered = ServerValues {
        key: searched.clone(),
        task: None,
        result: Some(Err("x".into())),
        since: t0,
    };
    assert_eq!(
        values_due(Some(&answered), &searched, t0 + SETTLE),
        ValuesDue::Done
    );
}

fn answered(
    key: (usize, &str),
    labels: &[&str],
    unique: usize,
) -> crate::app::db_view::ServerValues {
    use octa::data::value_frequency::{ValueFrequency, ValueFrequencyRow};
    crate::app::db_view::ServerValues {
        key: (key.0, key.1.to_string()),
        task: None,
        result: Some(Ok(ValueFrequency {
            column_name: "name".into(),
            rows: labels
                .iter()
                .map(|l| ValueFrequencyRow {
                    label: l.to_string(),
                    count: 1,
                })
                .collect(),
            nulls: 0,
            total_non_null: labels.len(),
            unique_count: unique,
            binned: false,
        })),
        since: Instant::now(),
    }
}

#[test]
fn the_popup_takes_only_the_answer_for_what_it_shows() {
    let mut tab = db_tab();
    tab.table_state.facet_col = Some(0);
    // The whole column: its distinct count and the note come with it.
    tab.facet_values = Some(answered((0, ""), &["a", "b"], 9));
    tab.take_popup_values();
    assert_eq!(tab.table_state.facet_unique, 9);
    assert!(tab.table_state.facet_external_note.is_some());
    // A search answers for its matches only.
    tab.table_state.facet_search = " a ".into();
    tab.table_state.facet_external_note = None;
    tab.facet_values = Some(answered((0, "a"), &["a"], 1));
    tab.take_popup_values();
    assert_eq!(tab.table_state.facet_rows.len(), 1);
    assert_eq!(tab.table_state.facet_unique, 9, "still the whole column's");
    assert!(tab.table_state.facet_external_note.is_none());
    // The search was cleared while "b" ran: that answer never lands.
    tab.table_state.facet_search.clear();
    tab.facet_values = Some(answered((0, "b"), &["b"], 1));
    tab.take_popup_values();
    assert!(tab.facet_values.is_none(), "dropped, and so cancelled");
    assert_eq!(tab.table_state.facet_rows, vec![("a".to_string(), 1)]);
}

#[test]
fn a_page_lands_in_the_tab_column_order() {
    // The tab shows `n` first (moved); the server answers in its own order.
    let mut tab = db_tab();
    tab.table.move_column(1, 0);
    let names = column_names(&tab);
    assert_eq!(names, vec!["n".to_string(), "name".to_string()]);
    let rows = rows_in_column_order(page(2), &names);
    assert_eq!(
        rows[1],
        vec![CellValue::Int(1), CellValue::String("r1".into())]
    );
    // A column the page lacks reads as Null.
    let rows = rows_in_column_order(page(1), &["x".to_string(), "name".to_string()]);
    assert_eq!(
        rows[0],
        vec![CellValue::Null, CellValue::String("r0".into())]
    );
    // And the swap uses it.
    tab.apply_view_page(sorted_view(), page(2), 3);
    assert_eq!(tab.table.rows[0][0], CellValue::Int(0), "n under n");
}

#[test]
fn moving_a_column_leaves_the_view_alone() {
    let mut tab = db_tab();
    tab.sync_column_keys(); // as every frame does
    tab.column_filters
        .insert(0, ["a".to_string()].into_iter().collect());
    tab.column_filters
        .insert(1, ["1".to_string()].into_iter().collect());
    tab.search_text = "a".into();
    let (before, _) = wanted_view(&tab, true);
    tab.table.move_column(1, 0);
    tab.sync_column_keys();
    assert_eq!(wanted_view(&tab, true).0, before, "no re-query for a drag");
}

#[test]
fn the_cancel_snapshot_and_search_scope_follow_a_column_move() {
    let now = Instant::now();
    let mut tab = db_tab();
    tab.sync_column_keys();
    tab.column_filters
        .insert(0, ["a".to_string()].into_iter().collect());
    tab.search_scope_col = Some(0);
    // In step: the snapshot is taken with `name` at index 0.
    let wanted = wanted_view(&tab, false).0;
    tab.server_view = Some(wanted.clone());
    sync_step(&mut tab, wanted, false, false, now);
    tab.table.move_column(0, 1);
    tab.sync_column_keys();
    assert_eq!(tab.search_scope_col, Some(1), "the live scope follows");
    tab.column_filters.clear();
    tab.cancel_view_change(sorted_view());
    assert!(
        tab.column_filters.contains_key(&1),
        "back on `name`, now at 1"
    );
    assert_eq!(tab.search_scope_col, Some(1));
}

#[test]
fn a_new_view_ends_a_download_and_restarts_the_recipe_marks() {
    let mut tab = db_tab();
    tab.loading_all = true;
    tab.recipe.push(crate::app::recipe::RecordedStep {
        step: octa::data::recipe::RecipeStep::Sort(octa::data::recipe::Sort { by: vec![] }),
        undo_mark: 5,
        enabled: true,
        pending: None,
    });
    tab.recipe_seen = 5;
    tab.apply_view_page(sorted_view(), page(2), 3);
    assert!(!tab.loading_all, "else the sync waits forever");
    assert_eq!(tab.recipe[0].undo_mark, 0);
    assert_eq!(tab.recipe_seen, 0);
}

fn a_hash() -> octa::db::pushdown::hash::ServerHash {
    octa::db::pushdown::hash::ServerHash {
        name: "h".into(),
        columns: vec!["name".into()],
        algo: octa::data::transform::hash_columns::HashColumnsAlgo::Md5,
        delimiter: "|".into(),
        null_text: String::new(),
        trim: false,
        upper: false,
    }
}

/// The dialog's Apply: the hash goes into what the tab asks for, an empty
/// column waits for it, and the page the sync fetches fills it by name.
fn hashed_tab() -> TabState {
    let mut tab = db_tab();
    tab.server_hashes.push(a_hash());
    tab.server_hash_seen.push("h".into());
    tab.table.insert_column(2, "h".into(), "Utf8".into());
    tab.sync_column_keys();
    tab
}

#[test]
fn a_hash_column_is_asked_for_and_filled_by_the_page() {
    let mut tab = hashed_tab();
    let (wanted, _) = wanted_view(&tab, false);
    assert_eq!(wanted.derived, vec![a_hash()]);
    assert!(!applied_is(&tab, &wanted), "the sync re-reads");
    let mut p = page(2);
    p.columns.push(ColumnInfo {
        name: "h".into(),
        data_type: "Utf8".into(),
    });
    for r in &mut p.rows {
        r.push(CellValue::String("digest".into()));
    }
    // A full page: more may exist, so analyses still go to the server.
    tab.apply_view_page(wanted.clone(), p, 2);
    assert_eq!(
        tab.table.get(1, 2),
        Some(&CellValue::String("digest".into()))
    );
    assert!(applied_is(&tab, &wanted_view(&tab, false).0));
    let conns = [conn()];
    let src = crate::app::pushdown::server_source_for(&tab, true, &conns).unwrap();
    assert_eq!(src.derived, vec![a_hash()], "analyses see the hash");
}

/// Deleting or renaming the hash column ends it on both sides, so the
/// view stays in step and nothing is fetched again.
#[test]
fn a_deleted_or_renamed_hash_column_leaves_the_view() {
    for rename in [false, true] {
        let mut tab = hashed_tab();
        let (wanted, _) = wanted_view(&tab, false);
        tab.server_view = Some(wanted);
        if rename {
            tab.table.rename_column(2, "mine".into());
        } else {
            tab.table.delete_column(2);
        }
        tab.sync_column_keys();
        assert!(tab.server_hashes.is_empty(), "rename {rename}");
        assert_eq!(tab.server_view, None, "rename {rename}");
        assert!(applied_is(&tab, &wanted_view(&tab, false).0));
        assert_eq!(
            tab.server_hash_seen,
            vec!["h".to_string()],
            "never written back"
        );
    }
}

#[test]
fn a_hash_cell_cannot_be_typed_into() {
    let mut tab = hashed_tab();
    tab.table_state.begin_edit(0, 0, "a".into());
    assert_eq!(tab.close_hash_editor(), None, "an ordinary column edits");
    tab.table_state.begin_edit(0, 2, String::new());
    assert!(tab.close_hash_editor().is_some());
    assert!(tab.table_state.editing_cell.is_none());
}
