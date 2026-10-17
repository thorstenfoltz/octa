use super::super::test_support::DuckConn;
use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

pub(crate) fn text_table(cols: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|c| ColumnInfo {
            name: (*c).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| match *v {
                    "NULL" => CellValue::Null,
                    v => CellValue::String(v.into()),
                })
                .collect()
        })
        .collect();
    t
}

fn orders_and_customers() -> (DataTable, DataTable) {
    let orders = text_table(
        &["id", "cust", "status"],
        &[
            &["1", "c1", "open"],
            &["2", "c2", "open"],
            &["3", "c1 ", "shut"],
            &["4", "c3", "open"],
            &["5", "", "open"],
            &["6", "c9", "NULL"],
        ],
    );
    let customers = text_table(
        &["id", "active"],
        &[
            &["c1", "open"],
            &["c2", "shut"],
            &["c3", "open"],
            &["c4", "open"],
        ],
    );
    (orders, customers)
}

/// The in-memory finder over every row and the server path must agree
/// exactly, order included.
#[test]
fn the_server_ranks_exactly_like_the_in_memory_finder() {
    use super::super::test_support::source_named;
    let (orders, customers) = orders_and_customers();
    let want = crate::data::join_keys::suggest_keys(&[&orders, &customers], usize::MAX);
    let tables = vec![
        (source_named("data"), orders.columns.clone()),
        (source_named("customers"), customers.columns.clone()),
    ];
    let mut c = DuckConn::with_tables(orders, vec![("customers", customers)]);
    let got = run(&mut c, &tables, &AtomicBool::new(false)).unwrap();
    assert!(!got.is_empty());
    assert_eq!(got, want);
}

/// Re-scoring the pairs the full ranking found gives the full ranking;
/// a pair left out is not reported.
#[test]
fn rescore_checks_only_the_given_pairs() {
    use super::super::test_support::source_named;
    let (orders, customers) = orders_and_customers();
    let want = crate::data::join_keys::suggest_keys(&[&orders, &customers], usize::MAX);
    let pairs: Vec<ColPair> = want.iter().map(|k| (k.left, k.right)).collect();
    let tables = vec![
        (source_named("data"), orders.columns.clone()),
        (source_named("customers"), customers.columns.clone()),
    ];
    let mut c = DuckConn::with_tables(orders, vec![("customers", customers)]);
    let stop = AtomicBool::new(false);
    assert_eq!(rescore(&mut c, &tables, &pairs, &stop).unwrap(), want);
    let one = rescore(&mut c, &tables, &pairs[..1], &stop).unwrap();
    assert_eq!(one, want[..1].to_vec());
}

/// A column whose stats the server refuses, (0, 0), must stay out of
/// every match statement: one refused column would fail them all.
#[test]
fn a_refused_column_stays_out_of_the_match_statements() {
    use super::super::test_support::source_named;
    let (orders, customers) = orders_and_customers();
    let want = crate::data::join_keys::suggest_keys(&[&orders, &customers], usize::MAX);
    let mut cols = orders.columns.clone();
    cols.push(ColumnInfo {
        name: "ghost".into(),
        data_type: "Utf8".into(),
    });
    let tables = vec![
        (source_named("data"), cols),
        (source_named("customers"), customers.columns.clone()),
    ];
    let mut c = DuckConn::with_tables(orders, vec![("customers", customers)]);
    let stop = AtomicBool::new(false);
    assert_eq!(run(&mut c, &tables, &stop).unwrap(), want);
    let ghost = ((0, 3), (1, 0));
    let mut pairs: Vec<ColPair> = want.iter().map(|k| (k.left, k.right)).collect();
    pairs.push(ghost);
    assert_eq!(rescore(&mut c, &tables, &pairs, &stop).unwrap(), want);
    let matches: Vec<&String> = c.log.iter().filter(|q| q.contains("INNER JOIN")).collect();
    assert!(!matches.is_empty());
    assert!(matches.iter().all(|q| !q.contains("ghost")));
}

#[test]
fn stats_count_distinct_trimmed_keys_and_skip_blanks() {
    let t = text_table(
        &["k"],
        &[&["a"], &[" a "], &["b"], &[""], &["NULL"], &["  "]],
    );
    let mut c = DuckConn::new(t);
    let e = dialect::key_text(DbEngine::Postgres, "\"k\"");
    let got = expr_stats(&mut c, "\"data\"", &[e], &AtomicBool::new(false)).unwrap();
    assert_eq!(got, vec![(2, 3)]);
}

#[test]
fn shared_counts_every_column_pair_in_one_statement() {
    let a = text_table(&["x", "y"], &[&["1", "p"], &["2", "q"], &["3", "r"]]);
    let b = text_table(&["z"], &[&["2"], &["3"], &["q"], &["9"]]);
    let mut c = DuckConn::with_tables(a, vec![("other", b)]);
    let k = |col: &str| dialect::key_text(DbEngine::Postgres, &format!("\"{col}\""));
    let (ax, bx) = ([k("x"), k("y")], [k("z")]);
    let got = shared_counts(
        &mut c,
        DbEngine::Postgres,
        KeySide {
            from: "\"data\"",
            exprs: &ax,
        },
        KeySide {
            from: "\"other\"",
            exprs: &bx,
        },
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(got.get(&(0, 0)), Some(&2));
    assert_eq!(got.get(&(1, 0)), Some(&1));
    assert_eq!(c.log.len(), 1);
}

#[test]
fn diagonal_pairs_only_matching_indices() {
    let a = text_table(&["x"], &[&["A"], &["b"]]);
    let b = text_table(&["x"], &[&["a"], &["b"]]);
    let mut c = DuckConn::with_tables(a, vec![("other", b)]);
    let raw = "CAST(\"x\" AS VARCHAR)".to_string();
    let low = "LOWER(CAST(\"x\" AS VARCHAR))".to_string();
    let exprs = [raw, low];
    let got = shared_counts(
        &mut c,
        DbEngine::Postgres,
        KeySide {
            from: "\"data\"",
            exprs: &exprs,
        },
        KeySide {
            from: "\"other\"",
            exprs: &exprs,
        },
        true,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(got.get(&(0, 0)), Some(&1));
    assert_eq!(got.get(&(1, 1)), Some(&2));
    assert_eq!(got.get(&(0, 1)), None);
}

#[test]
fn a_chunk_never_answers_more_rows_than_the_cap() {
    for row_cap in [1, 2, 1_000, 4_095, 4_096, 2_000_000, usize::MAX] {
        let s = chunk_size(row_cap);
        assert!(s * s <= row_cap, "{s} x {s} > {row_cap}");
        assert!((1..=MAX_UNION_BRANCHES).contains(&s));
    }
    assert_eq!(chunk_size(2_000_000), MAX_UNION_BRANCHES);
}

#[test]
fn wide_tables_are_split_into_branch_chunks() {
    let n = MAX_UNION_BRANCHES + 1;
    let cols: Vec<String> = (0..n).map(|i| format!("c{i}")).collect();
    let names: Vec<&str> = cols.iter().map(String::as_str).collect();
    let row: Vec<&str> = vec!["1"; n];
    let a = text_table(&names, &[&row]);
    let b = text_table(&["z"], &[&["1"]]);
    let mut c = DuckConn::with_tables(a, vec![("other", b)]);
    let exprs: Vec<String> = cols
        .iter()
        .map(|col| dialect::key_text(DbEngine::Postgres, &format!("\"{col}\"")))
        .collect();
    let z = vec![dialect::key_text(DbEngine::Postgres, "\"z\"")];
    let got = shared_counts(
        &mut c,
        DbEngine::Postgres,
        KeySide {
            from: "\"data\"",
            exprs: &exprs,
        },
        KeySide {
            from: "\"other\"",
            exprs: &z,
        },
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(c.log.len(), 2);
    assert_eq!(got.len(), n);
    assert!(
        c.log
            .iter()
            .all(|q| q.matches("UNION ALL").count() < MAX_UNION_BRANCHES)
    );
}
