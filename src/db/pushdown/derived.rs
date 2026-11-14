//! The columns a database tab asks the server to add after the table's own:
//! hash columns and calculated columns. Both live in the tab's server view by
//! name, so a column moving never breaks them, and every analysis reads the
//! table with them added.

use crate::db::DbEngine;

use super::calc::{ServerCalc, calc_sql};
use super::hash::{ServerHash, hash_sql};

#[derive(Debug, Clone, PartialEq)]
pub enum ServerColumn {
    Hash(ServerHash),
    Calc(ServerCalc),
}

impl ServerColumn {
    pub fn name(&self) -> &str {
        match self {
            ServerColumn::Hash(h) => &h.name,
            ServerColumn::Calc(c) => &c.name,
        }
    }

    /// The columns the expression reads.
    pub fn references(&self) -> Vec<&str> {
        match self {
            ServerColumn::Hash(h) => h.columns.iter().map(String::as_str).collect(),
            ServerColumn::Calc(c) => c.columns().map(String::as_str).collect(),
        }
    }

    /// The expression. A calculation the engine cannot say never gets here
    /// (the dialog computes it locally); `NULL` is the defensive answer.
    pub fn sql(&self, e: DbEngine) -> String {
        match self {
            ServerColumn::Hash(h) => hash_sql(e, h),
            ServerColumn::Calc(c) => calc_sql(e, c).unwrap_or_else(|| "NULL".to_string()),
        }
    }
}

/// The table with `cols` added after its own columns. The table alias has
/// no `AS` (Oracle), and `octa_t.*` because Oracle refuses a bare `*` beside
/// other select items. Window functions here see every row of the table:
/// the tab's filters apply outside this subquery.
pub fn select_with(e: DbEngine, table_sql: &str, cols: &[ServerColumn]) -> String {
    let added: String = cols
        .iter()
        .map(|c| format!(", {} AS {}", c.sql(e), e.quote_ident(c.name())))
        .collect();
    format!("SELECT octa_t.*{added} FROM {table_sql} octa_t")
}
