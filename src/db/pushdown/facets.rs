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
#[path = "facets_tests.rs"]
mod tests;
