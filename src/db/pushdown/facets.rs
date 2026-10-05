//! The column filter's value list from the server: one column's most common
//! values over the whole table, or only those whose text contains a search
//! term. The popup searches its loaded values with
//! `label.to_lowercase().contains(search.trim().to_lowercase())`; this is the
//! same test, run where every value is.

use std::sync::atomic::AtomicBool;

use crate::data::ColumnInfo;
use crate::data::value_frequency::{BinningMode, ValueFrequency};
use crate::db::DbConnector;

use super::ServerSource;
use super::view::{ServerView, ViewFilter};

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    col: &ColumnInfo,
    search: &str,
    top_n: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<ValueFrequency> {
    let needle = search.trim();
    let mut src = src.clone();
    if !needle.is_empty() {
        let contains = ServerView {
            order: Vec::new(),
            filters: vec![ViewFilter::Contains {
                columns: vec![col.name.clone()],
                needle: needle.to_string(),
                case_sensitive: false,
            }],
            derived: Vec::new(),
        }
        .where_sql(src.engine());
        src.filter = match (src.filter.take(), contains) {
            (Some(a), Some(b)) => Some(format!("({a}) AND ({b})")),
            (a, b) => a.or(b),
        };
    }
    super::value_frequency::run(c, &src, col, Some(top_n), BinningMode::None, cancel)
}

#[cfg(test)]
mod tests {
    use super::super::join_keys::tests::text_table;
    use super::super::test_support::{DuckConn, source};
    use super::*;
    use crate::data::value_frequency::compute_value_frequency;

    /// Counts 4, 3, 2, 1 so no two values tie.
    fn table() -> crate::data::DataTable {
        let mut rows: Vec<&[&str]> = Vec::new();
        for (v, n) in [("Banana", 4), ("apple", 3), ("Pineapple", 2), ("cherry", 1)] {
            for _ in 0..n {
                rows.push(match v {
                    "Banana" => &["Banana"],
                    "apple" => &["apple"],
                    "Pineapple" => &["Pineapple"],
                    _ => &["cherry"],
                });
            }
        }
        rows.push(&["NULL"]);
        text_table(&["fruit"], &rows)
    }

    fn pairs(vf: &ValueFrequency) -> Vec<(String, usize)> {
        vf.rows.iter().map(|r| (r.label.clone(), r.count)).collect()
    }

    #[test]
    fn the_list_matches_the_loaded_rows_list() {
        let t = table();
        let col = t.columns[0].clone();
        let want = compute_value_frequency(&t, 0, Some(50), BinningMode::None).unwrap();
        let mut c = DuckConn::new(t);
        let got = run(&mut c, &source(), &col, "", 50, &AtomicBool::new(false)).unwrap();
        assert_eq!(pairs(&got), pairs(&want));
        assert_eq!(got.unique_count, want.unique_count);
        assert_eq!(got.nulls, want.nulls);
    }

    #[test]
    fn a_search_finds_the_same_values_the_popup_would() {
        let t = table();
        let col = t.columns[0].clone();
        let all = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
        let want: Vec<(String, usize)> = pairs(&all)
            .into_iter()
            .filter(|(l, _)| l.to_lowercase().contains("apple"))
            .collect();
        let mut c = DuckConn::new(t);
        let got = run(
            &mut c,
            &source(),
            &col,
            "  APPLE ",
            50,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(pairs(&got), want);
        assert_eq!(got.unique_count, 2);
    }

    #[test]
    fn a_search_keeps_the_tab_filter() {
        let t = table();
        let col = t.columns[0].clone();
        let mut src = source();
        src.filter = Some("\"fruit\" <> 'apple'".into());
        let mut c = DuckConn::new(t);
        let got = run(&mut c, &src, &col, "apple", 50, &AtomicBool::new(false)).unwrap();
        assert_eq!(pairs(&got), vec![("Pineapple".to_string(), 2)]);
    }
}
