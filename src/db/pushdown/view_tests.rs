use super::super::join_keys::tests::text_table;
use super::super::test_support::DuckConn;
use super::*;
use crate::data::conditional_format::CondOp;
use crate::data::predicate_filter::{PredicateFilter, row_passes};
use crate::data::search::RowMatcher;
use crate::data::{CellValue, DataTable, SearchMode};
use crate::db::DbConnector;

/// `id`, `name` (text with case, spaces, NULL and ''), `n` (Int64 with a NULL).
fn table() -> DataTable {
    let mut t = text_table(
        &["id", "name", "n"],
        &[
            &["1", "Apple", "5"],
            &["2", "apple pie", "50"],
            &["3", "Banana", "500"],
            &["4", "", "7"],
            &["5", "NULL", "NULL"],
            &["6", "50% off", "-3"],
            &["7", "a_b", "12"],
            &["8", "Cherry", "1000"],
            &["9", "apple", "5"],
        ],
    );
    // `text_table` reads "NULL" as a missing cell; make `id` and `n` numbers.
    t.columns[0].data_type = "Int64".into();
    t.columns[2].data_type = "Int64".into();
    for r in &mut t.rows {
        for c in [0, 2] {
            if let CellValue::String(s) = &r[c] {
                let i: i64 = s.parse().expect("int");
                r[c] = CellValue::Int(i);
            }
        }
    }
    t
}

/// The ids `page_sql` returns from the DuckDB stand-in.
fn server_ids(view: &ServerView) -> Vec<i64> {
    let mut c = DuckConn::new(table());
    let sql = page_sql(DbEngine::Postgres, view, "\"data\"", &["id".into()], 100, 0);
    let t = c.query(&sql).expect(&sql);
    (0..t.row_count())
        .map(|r| match t.get(r, 0) {
            Some(CellValue::Int(i)) => *i,
            other => panic!("id cell {other:?}"),
        })
        .collect()
}

/// The ids the in-memory filter keeps, in table order.
fn local_ids(keep: impl Fn(&DataTable, usize) -> bool) -> Vec<i64> {
    let t = table();
    (0..t.row_count())
        .filter(|&r| keep(&t, r))
        .map(|r| match t.get(r, 0) {
            Some(CellValue::Int(i)) => *i,
            _ => unreachable!(),
        })
        .collect()
}

fn sorted(mut v: Vec<i64>) -> Vec<i64> {
    v.sort_unstable();
    v
}

#[test]
fn a_value_filter_keeps_the_same_rows() {
    for allowed in [
        vec!["Apple"],
        vec!["apple"],
        vec!["", "Banana"],
        vec!["50% off", "a_b"],
        vec![],
    ] {
        let f = ViewFilter::values("name", allowed.iter().map(|s| s.to_string()));
        let want = local_ids(|t, r| {
            let s = t.get(r, 1).map(|v| v.to_string()).unwrap_or_default();
            allowed.contains(&s.as_str())
        });
        let view = ServerView {
            order: vec![],
            filters: vec![f],
            derived: Vec::new(),
        };
        assert_eq!(sorted(server_ids(&view)), want, "{allowed:?}");
    }
}

#[test]
fn comparisons_keep_the_same_rows() {
    let cases = [
        ("name", "Utf8", CondOp::Eq, "apple", false),
        ("name", "Utf8", CondOp::Eq, "apple", true),
        ("name", "Utf8", CondOp::Ne, "apple", false),
        ("name", "Utf8", CondOp::Contains, "an", false),
        ("name", "Utf8", CondOp::Contains, "%", false),
        ("name", "Utf8", CondOp::NotContains, "p", false),
        ("name", "Utf8", CondOp::Empty, "", false),
        ("name", "Utf8", CondOp::NotEmpty, "", false),
        ("n", "Int64", CondOp::Gt, "50", false),
        ("n", "Int64", CondOp::Lt, "50", false),
        ("n", "Int64", CondOp::Le, "-3", false),
        ("n", "Int64", CondOp::Ge, "1000", false),
    ];
    for (col, ty, op, value, cs) in cases {
        let idx = if col == "name" { 1 } else { 2 };
        let f = ViewFilter::compare(col, ty, op, value, cs).expect("expressible");
        let p = PredicateFilter {
            col: idx,
            op,
            value: value.into(),
            case_sensitive: cs,
        };
        let want = local_ids(|t, r| row_passes(std::slice::from_ref(&p), t, r));
        let view = ServerView {
            order: vec![],
            filters: vec![f],
            derived: Vec::new(),
        };
        assert_eq!(
            sorted(server_ids(&view)),
            want,
            "{col} {op:?} {value:?} cs={cs}"
        );
    }
}

#[test]
fn ordering_compares_on_text_stay_local() {
    assert!(ViewFilter::compare("name", "Utf8", CondOp::Gt, "5", false).is_none());
    assert!(ViewFilter::compare("n", "Int64", CondOp::Gt, "abc", false).is_none());
    assert!(ViewFilter::compare("n", "Int64", CondOp::Eq, "abc", false).is_some());
}

#[test]
fn plain_and_wildcard_search_keep_the_same_rows() {
    let cols = vec!["name".to_string(), "n".to_string()];
    for (mode, q, cs) in [
        (SearchMode::Plain, "app", false),
        (SearchMode::Plain, "App", true),
        (SearchMode::Plain, "50", false),
        (SearchMode::Plain, "%", false),
        (SearchMode::Wildcard, "a*e", false),
        (SearchMode::Wildcard, "?ana*", false),
        (SearchMode::Wildcard, "a_b", false),
        (SearchMode::Wildcard, "50\\*", false),
        (SearchMode::Wildcard, "*", false),
    ] {
        let m = RowMatcher::with_options(q, mode, cs, false);
        let want = local_ids(|t, r| {
            [1, 2]
                .iter()
                .any(|&c| t.get(r, c).is_some_and(|v| m.matches(&v.to_string())))
        });
        let f = match mode {
            SearchMode::Plain => ViewFilter::Contains {
                columns: cols.clone(),
                needle: q.into(),
                case_sensitive: cs,
            },
            _ => ViewFilter::Wildcard {
                columns: cols.clone(),
                pattern: q.into(),
                case_sensitive: cs,
            },
        };
        let view = ServerView {
            order: vec![],
            filters: vec![f],
            derived: Vec::new(),
        };
        assert_eq!(sorted(server_ids(&view)), want, "{mode:?} {q:?} cs={cs}");
    }
}

#[test]
fn the_sort_matches_octas_stable_sort() {
    for keys in [
        vec![(1usize, true)],
        vec![(1, false)],
        vec![(2, true)],
        vec![(2, false)],
        vec![(1, true), (2, false)],
    ] {
        let mut t = table();
        t.sort_rows_by_columns(&keys);
        let want: Vec<i64> = t
            .rows
            .iter()
            .map(|r| match &r[0] {
                CellValue::Int(i) => *i,
                _ => unreachable!(),
            })
            .collect();
        let cols = table().columns;
        let order = keys
            .iter()
            .map(|&(c, asc)| SortKey {
                column: cols[c].name.clone(),
                ascending: asc,
                text: is_text_type(&cols[c].data_type),
            })
            .collect();
        let view = ServerView {
            order,
            filters: vec![],
            derived: Vec::new(),
        };
        assert_eq!(server_ids(&view), want, "{keys:?}");
    }
}

#[test]
fn pages_follow_on_from_each_other() {
    let view = ServerView {
        order: vec![SortKey {
            column: "name".into(),
            ascending: true,
            text: true,
        }],
        filters: vec![],
        derived: Vec::new(),
    };
    let mut c = DuckConn::new(table());
    let mut seen: Vec<i64> = Vec::new();
    for offset in [0, 3, 6] {
        let t = c
            .query(&page_sql(
                DbEngine::Postgres,
                &view,
                "\"data\"",
                &["id".into()],
                3,
                offset,
            ))
            .unwrap();
        seen.extend((0..t.row_count()).map(|r| match t.get(r, 0) {
            Some(CellValue::Int(i)) => *i,
            other => panic!("id cell {other:?}"),
        }));
    }
    // Three pages in a row are the one sorted result, no row twice or missing.
    assert_eq!(seen, server_ids(&view));
    assert_eq!(sorted(seen), (1..=9).collect::<Vec<_>>());
}

#[test]
fn every_engine_spells_paging_and_null_order() {
    let view = ServerView {
        order: vec![SortKey {
            column: "a".into(),
            ascending: true,
            text: false,
        }],
        filters: vec![],
        derived: Vec::new(),
    };
    let page = |e| page_sql(e, &view, "t", &["id".into()], 10, 20);
    assert!(
        page(DbEngine::Postgres)
            .ends_with("ORDER BY \"a\" ASC NULLS FIRST, \"id\" LIMIT 10 OFFSET 20")
    );
    assert!(
        page(DbEngine::Mssql)
            .ends_with("ORDER BY [a] ASC, [id] OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY")
    );
    let ora = page_sql(
        DbEngine::Oracle,
        &ServerView {
            order: vec![SortKey {
                column: "a".into(),
                ascending: true,
                text: true,
            }],
            filters: vec![],
            derived: Vec::new(),
        },
        "t",
        &[],
        10,
        0,
    );
    assert!(ora.contains("NLSSORT("), "{ora}");
    assert!(page(DbEngine::Oracle).contains("OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"));
    assert!(page(DbEngine::Trino).ends_with("OFFSET 20 LIMIT 10"));
    assert!(page(DbEngine::Athena).ends_with("OFFSET 20 LIMIT 10"));
    assert!(page(DbEngine::MySql).contains("ORDER BY `a` ASC, `id` LIMIT"));
    let unsorted = page_sql(DbEngine::Mssql, &ServerView::default(), "t", &[], 10, 0);
    assert!(unsorted.contains("ORDER BY (SELECT NULL) OFFSET 0 ROWS"));
    assert_eq!(
        page_sql(DbEngine::Postgres, &ServerView::default(), "t", &[], 10, 0),
        "SELECT * FROM t LIMIT 10 OFFSET 0"
    );
}

#[test]
fn a_long_value_list_is_split_for_oracle() {
    let f = ViewFilter::values("c", (0..2500).map(|i| i.to_string()));
    let w = ServerView {
        order: vec![],
        filters: vec![f],
        derived: Vec::new(),
    }
    .where_sql(DbEngine::Oracle)
    .unwrap();
    assert_eq!(w.matches(" IN (").count(), 3);
}

#[test]
fn like_escapes_follow_the_engine() {
    assert_eq!(like_pattern("50%_a*b?\\*", '!', false), "%50!%!_a%b_*%");
    assert_eq!(like_pattern("x[1]", '!', true), "%x![1]%");
    assert_eq!(like_pattern("x[1]", '!', false), "%x[1]%");
    assert_eq!(like_pattern("a!b", '!', false), "%a!!b%");
    let bq = like_sql(DbEngine::BigQuery, "`c`", "5%", true);
    assert!(bq.ends_with(r"LIKE '%5\\%%'"), "{bq}");
    assert!(!bq.contains("ESCAPE"));
}

#[test]
fn mysql_wildcard_compares_characters_not_bytes() {
    let sql = like_sql(DbEngine::MySql, "`c`", "M?ller", false);
    assert!(sql.contains("utf8mb4_0900_bin"), "{sql}");
    assert!(!sql.contains("AS BINARY"), "{sql}");
}

#[test]
fn an_empty_needle_matches_everything() {
    assert_eq!(contains_sql(DbEngine::Oracle, "\"c\"", "", false), "1 = 1");
}
