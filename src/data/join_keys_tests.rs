use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table(cols: &[&str], rows: Vec<Vec<&str>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|c| ColumnInfo {
            name: (*c).to_string(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|v| CellValue::String(v.to_string()))
                .collect()
        })
        .collect();
    t
}

#[test]
fn score_counts_matches_score_pair() {
    let l: HashSet<String> = ["1", "2", "3"].iter().map(|s| s.to_string()).collect();
    let r: HashSet<String> = ["2", "3", "9", "10"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(score_pair(&l, 3, &r, 8), score_counts(3, 3, 4, 8, 2));
    assert_eq!(score_counts(3, 3, 4, 8, 0), None);
    assert_eq!(score_counts(0, 0, 4, 8, 0), None);
}

#[test]
fn a_tie_is_broken_from_the_child_side_whichever_way_round_it_is_read() {
    // Both tables number their rows from 1, which is what makes two
    // candidates score identically: `customers.id` against `orders.id` is
    // a coincidence (order numbers 1..4 simply exist), `customers.id`
    // against `orders.customer_id` is the real key. Reading orphans from
    // the parent side cannot tell them apart - all four customer ids are
    // present either way - so both directions have to be reported.
    let int_col = |name: &str| ColumnInfo {
        name: name.to_string(),
        data_type: "Int64".into(),
    };
    let mut customers = DataTable::empty();
    customers.columns = vec![int_col("id")];
    customers.rows = (1..=4i64).map(|i| vec![CellValue::Int(i)]).collect();
    let mut orders = DataTable::empty();
    orders.columns = vec![int_col("id"), int_col("customer_id")];
    orders.rows = (1..=1000i64)
        .map(|i| vec![CellValue::Int(i), CellValue::Int(1 + (i - 1) % 4)])
        .collect();

    let out = suggest_keys(&[&customers, &orders], DEFAULT_SAMPLE_ROWS);
    let find = |right_col: usize| {
        out.iter()
            .find(|k| k.left == (0, 0) && k.right == (1, right_col))
            .unwrap_or_else(|| panic!("no candidate for right column {right_col}"))
    };
    let coincidence = find(0);
    let real_key = find(1);

    // The tie itself: identical scores, and identical orphan counts in the
    // direction the caller did not choose.
    assert_eq!(
        coincidence.score, real_key.score,
        "the two must actually tie"
    );
    assert_eq!(coincidence.left_orphans, 0);
    assert_eq!(real_key.left_orphans, 0);

    // The other direction settles it.
    assert_eq!(coincidence.right_orphans, 996);
    assert_eq!(real_key.right_orphans, 0);
    assert_eq!(coincidence.right_values, 1000);
    assert_eq!(real_key.right_values, 4);
}

#[test]
fn the_real_key_ranks_first() {
    let orders = table(
        &["cust_id", "status"],
        vec![vec!["c1", "open"], vec!["c2", "open"], vec!["c3", "shut"]],
    );
    let customers = table(
        &["id", "active"],
        vec![
            vec!["c1", "yes"],
            vec!["c2", "yes"],
            vec!["c3", "no"],
            vec!["c9", "no"],
        ],
    );

    let out = suggest_keys(&[&orders, &customers], DEFAULT_SAMPLE_ROWS);
    assert!(!out.is_empty());
    let best = &out[0];
    assert_eq!(best.left, (0, 0));
    assert_eq!(best.right, (1, 0));
    assert!(best.overlap > 0.99, "overlap was {}", best.overlap);
}

/// A low-cardinality column that happens to overlap must not beat a key.
#[test]
fn low_cardinality_decoys_lose() {
    let a = table(
        &["id", "flag"],
        vec![vec!["1", "yes"], vec!["2", "no"], vec!["3", "yes"]],
    );
    let b = table(
        &["key", "flag"],
        vec![vec!["1", "yes"], vec!["2", "no"], vec!["3", "no"]],
    );
    let out = suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS);
    assert_eq!(out[0].left, (0, 0), "expected id -> key first, got {out:?}");
    assert_eq!(out[0].right, (1, 0));
}

#[test]
fn disjoint_tables_produce_nothing() {
    let a = table(&["x"], vec![vec!["a"], vec!["b"]]);
    let b = table(&["y"], vec![vec!["c"], vec!["d"]]);
    assert!(suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS).is_empty());
}

/// Built for N, not for two: three tables give all three pairings.
#[test]
fn three_tables_are_compared_pairwise() {
    let a = table(&["id"], vec![vec!["1"], vec!["2"]]);
    let b = table(&["id"], vec![vec!["1"], vec!["2"]]);
    let c = table(&["id"], vec![vec!["1"], vec!["2"]]);
    let out = suggest_keys(&[&a, &b, &c], DEFAULT_SAMPLE_ROWS);
    let pairs: std::collections::HashSet<(usize, usize)> =
        out.iter().map(|k| (k.left.0, k.right.0)).collect();
    assert_eq!(pairs.len(), 3, "expected 0-1, 0-2, 1-2, got {pairs:?}");
}

#[test]
fn empty_input_is_not_an_error() {
    assert!(suggest_keys(&[], DEFAULT_SAMPLE_ROWS).is_empty());
    let a = table(&["x"], vec![vec!["1"]]);
    assert!(suggest_keys(&[&a], DEFAULT_SAMPLE_ROWS).is_empty());
}

/// Blank cells are not values: a column of empties suggests nothing.
#[test]
fn empty_cells_are_ignored() {
    let a = table(&["id", "note"], vec![vec!["1", ""], vec!["2", "  "]]);
    let b = table(&["id", "note"], vec![vec!["1", ""], vec!["2", ""]]);
    let out = suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS);
    assert!(
        out.iter().all(|k| k.left.1 == 0 && k.right.1 == 0),
        "the empty note columns must not pair: {out:?}"
    );
}

/// The sample bounds the work: with a sample of 1 only the first row of
/// each table is considered.
#[test]
fn sample_size_bounds_the_scan() {
    let a = table(&["id"], vec![vec!["1"], vec!["2"], vec!["3"]]);
    let b = table(&["id"], vec![vec!["9"], vec!["2"], vec!["3"]]);
    assert!(
        suggest_keys(&[&a, &b], 1).is_empty(),
        "row 1 shares nothing"
    );
    assert!(!suggest_keys(&[&a, &b], 3).is_empty());
}
