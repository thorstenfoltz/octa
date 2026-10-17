use super::super::join_keys::tests::text_table;
use super::super::test_support::{DuckConn, source};
use super::*;

#[test]
fn the_server_finds_the_same_lookups() {
    // cust -> name holds except one row; cust -> city holds; NULL and ''
    // are one key value, as in memory.
    let t = text_table(
        &["cust", "name", "city", "amount"],
        &[
            &["1", "ada", "x", "10"],
            &["1", "ada", "x", "11"],
            &["1", "adx", "x", "12"],
            &["2", "bob", "y", "13"],
            &["2", "bob", "y", "14"],
            &["3", "cy", "z", "15"],
            &["3", "cy", "z", "16"],
            &["NULL", "eve", "w", "17"],
            &["", "eve", "w", "18"],
            &["4", "dee", "v", "19"],
            &["4", "dee", "v", "20"],
            &["4", "dee", "v", "21"],
        ],
    );
    let stop = AtomicBool::new(false);
    let want = crate::data::lookups::find_lookups(&t, 0.8, &stop);
    assert!(!want.is_empty());
    let cols = t.columns.clone();
    let n = t.row_count();
    let mut c = DuckConn::new(t);
    let got = run(&mut c, &source(), &cols, n, 0.8, &stop).unwrap();
    assert_eq!(got, want);
}

fn customers_flat() -> DataTable {
    // cust 1 has two names (breaks), cust 2 is clean, cust 3 has two
    // cities (breaks), NULL and '' keys are one key with two names
    // (breaks only because they are one; grouped raw, each is clean).
    text_table(
        &["cust", "name", "city"],
        &[
            &["1", "ada", "x"],
            &["1", "ada", "x"],
            &["1", "adx", "x"],
            &["2", "bob", "y"],
            &["2", "bob", "y"],
            &["3", "cy", "z"],
            &["3", "cy", "q"],
            &["NULL", "eve", "w"],
            &["", "eva", "w"],
        ],
    )
}

fn sorted_texts(t: &DataTable) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = t
        .rows
        .iter()
        .map(|r| r.iter().map(|v| v.to_string()).collect())
        .collect();
    rows.sort();
    rows
}

#[test]
fn breaking_rows_match_the_in_memory_rows() {
    let t = customers_flat();
    let want = t.clone_with_rows(&crate::data::lookups::breaking_rows(&t, 0, &[1, 2]));
    let mut c = DuckConn::new(t);
    let deps = ["name".to_string(), "city".to_string()];
    let (got, capped) = breaking_rows(
        &mut c,
        &source(),
        "cust",
        &deps,
        usize::MAX,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(!capped);
    assert_eq!(sorted_texts(&got), sorted_texts(&want));
    // Cust 1 (3 rows), cust 3 (2) and the NULL/'' key (2): the NULL key
    // only breaks because NULL and '' are one key, and both its rows must
    // come back through the NULL-safe join.
    assert_eq!(got.row_count(), 7);
    let blank_keys = got
        .rows
        .iter()
        .filter(|r| r[0].to_string().is_empty())
        .count();
    assert_eq!(blank_keys, 2);
    let (two, capped) =
        breaking_rows(&mut c, &source(), "cust", &deps, 2, &AtomicBool::new(false)).unwrap();
    assert!(capped);
    assert_eq!(two.row_count(), 2);
}

/// Every connector stops reading at the row cap, so a fetch asking for
/// `row_cap + 1` rows got `row_cap` back and never saw its cut.
#[test]
fn the_fetch_cap_leaves_room_to_see_a_cut() {
    for row_cap in [2, 1_000, 2_000_000] {
        let cap = fetch_cap(row_cap);
        assert!(cap < row_cap, "LIMIT {} exceeds the cap {row_cap}", cap + 1);
    }
    assert_eq!(fetch_cap(1), 1);
    assert!(
        cap_clause(DbEngine::Postgres, fetch_cap(usize::MAX)).is_empty(),
        "Unlimited fetches every row"
    );
}

#[test]
fn a_real_x_key_does_not_join_the_null_key() {
    // The join compares `COALESCE(key, 'x')` and the NULL-ness apart; the
    // NULL key breaks (NULL and '' are one key, two names), the key 'x'
    // does not, and its rows must stay out.
    let t = text_table(
        &["cust", "name"],
        &[&["NULL", "a"], &["", "b"], &["x", "c"], &["x", "c"]],
    );
    let mut c = DuckConn::new(t);
    let (got, _) = breaking_rows(
        &mut c,
        &source(),
        "cust",
        &["name".to_string()],
        usize::MAX,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        sorted_texts(&got),
        [["", "a"], ["", "b"]].map(|r| r.map(String::from).to_vec())
    );
}

#[test]
fn sql_server_keys_ignore_trailing_spaces() {
    // `_BIN2` GROUP BY pads, so each dependent's query may return either
    // spelling of one key; the lookup must merge them.
    assert_eq!(group_key(DbEngine::Mssql, "b2 "), "b2");
    assert_eq!(group_key(DbEngine::Mssql, "b2"), "b2");
    assert_eq!(group_key(DbEngine::MySql, "b2 "), "b2 ");
    assert_eq!(group_key(DbEngine::Postgres, "b2 "), "b2 ");
}

#[test]
fn the_split_matches_the_in_memory_split() {
    // No ties: the server breaks ties by value, which the docs state.
    let t = text_table(
        &["cust", "name", "amount"],
        &[
            &["1", "ada", "10"],
            &["1", "ada", "11"],
            &["1", "adx", "12"],
            &["2", "bob", "13"],
            &["2", "bob", "14"],
            &["3", "cy", "15"],
        ],
    );
    let want = crate::data::lookups::split_out(&t, 0, &[1]);
    let mut c = DuckConn::new(t);
    let (got, capped) = split_out(
        &mut c,
        &source(),
        "cust",
        &["name".to_string()],
        usize::MAX,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(!capped);
    assert_eq!(got.resolved_keys, want.resolved_keys);
    assert_eq!(sorted_texts(&got.lookup), sorted_texts(&want.lookup));
    assert_eq!(sorted_texts(&got.main), sorted_texts(&want.main));
    assert_eq!(
        got.main.columns.iter().map(|c| &c.name).collect::<Vec<_>>(),
        ["cust", "amount"]
    );
    assert_eq!(
        got.lookup
            .columns
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>(),
        ["cust", "name"]
    );
}

#[test]
fn the_split_counts_null_and_empty_keys_as_one() {
    // As Task 6 and the in-memory engine do: one lookup row for both.
    let t = customers_flat();
    let want = crate::data::lookups::split_out(&t, 0, &[1, 2]);
    let mut c = DuckConn::new(t);
    let deps = ["name".to_string(), "city".to_string()];
    let (got, _) = split_out(
        &mut c,
        &source(),
        "cust",
        &deps,
        usize::MAX,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(got.lookup.row_count(), want.lookup.row_count());
    assert_eq!(got.resolved_keys, want.resolved_keys);
    // Cust 1 (names), cust 3 (cities) and the NULL/'' key (names). Grouped
    // raw, NULL and '' would be two clean keys and this would be 2 of 5.
    assert_eq!(got.resolved_keys, 3);
    assert_eq!(got.lookup.row_count(), 4);
    let (one, capped) =
        split_out(&mut c, &source(), "cust", &deps, 1, &AtomicBool::new(false)).unwrap();
    assert!(capped);
    assert_eq!(one.lookup.row_count(), 1);
    assert_eq!(one.main.row_count(), 1);
}
