//! Calculated columns on the server: the window SQL for what
//! `data::calculated` computes, so a partly loaded database table gets the
//! column over every row. A database has no row order of its own, so the
//! kinds that follow one need an order column here; ClickHouse has no LAG
//! (its lagInFrame needs a frame the others do not), so the two changes and
//! previous / next stay local there. `None` means "compute on the loaded rows with the note".
//! Text ranks follow the server's collation (case-blind on MySQL and SQL
//! Server), where the local engine compares exactly; the docs say so.

use crate::data::ColumnInfo;
use crate::data::calculated::{CalcExtras, CalcKind, CalcSpec};
use crate::db::DbEngine;

use super::dialect::{as_float, sum_operand};

#[derive(Debug, Clone, PartialEq)]
pub struct ServerCalc {
    pub name: String,
    pub kind: CalcKind,
    pub value: Option<String>,
    /// The tab's type for `value`: a decimal adds and subtracts exactly.
    pub value_type: String,
    pub order: Option<String>,
    pub descending: bool,
    pub groups: Vec<String>,
    pub extras: CalcExtras,
}

impl ServerCalc {
    /// `spec` named `name`; `columns` are the tab's, which the spec's
    /// indices point into.
    pub fn of(spec: &CalcSpec, columns: &[ColumnInfo], name: &str) -> Self {
        let n = |c: usize| columns.get(c).map(|c| c.name.clone());
        let value = spec.value.filter(|_| spec.kind.needs_value());
        Self {
            name: name.to_string(),
            kind: spec.kind,
            value: value.and_then(n),
            value_type: value
                .and_then(|c| columns.get(c))
                .map(|c| c.data_type.clone())
                .unwrap_or_default(),
            order: spec.order.filter(|_| spec.kind.uses_order()).and_then(n),
            descending: spec.descending,
            groups: spec.groups.iter().filter_map(|&c| n(c)).collect(),
            extras: spec.extras,
        }
    }

    /// Every column the expression names.
    pub fn columns(&self) -> impl Iterator<Item = &String> {
        self.value
            .iter()
            .chain(self.order.iter())
            .chain(self.groups.iter())
    }
}

pub fn calc_sql(e: DbEngine, c: &ServerCalc) -> Option<String> {
    let q = |n: &str| e.quote_ident(n);
    let part = if c.groups.is_empty() {
        String::new()
    } else {
        let g: Vec<String> = c.groups.iter().map(|g| q(g)).collect();
        format!("PARTITION BY {} ", g.join(", "))
    };
    let dir = if c.descending { " DESC" } else { "" };
    let order_by = |col: &str| {
        let col = q(col);
        format!("ORDER BY CASE WHEN {col} IS NULL THEN 1 ELSE 0 END, {col}{dir}")
    };
    let num = |col: &str| as_float(e, &q(col));
    // A running total and a difference stay exact on a decimal column; the
    // percentages divide, and a double is what the local engine gives too.
    let exact = |col: &str| sum_operand(e, &q(col), &c.value_type);
    match c.kind {
        CalcKind::Rank | CalcKind::DenseRank => {
            let f = if c.kind == CalcKind::Rank {
                "RANK"
            } else {
                "DENSE_RANK"
            };
            Some(format!(
                "{f}() OVER ({part}{})",
                order_by(c.value.as_deref()?)
            ))
        }
        CalcKind::RowNumber => Some(format!(
            "ROW_NUMBER() OVER ({part}{})",
            order_by(c.order.as_deref()?)
        )),
        CalcKind::PercentOfTotal => {
            let v = num(c.value.as_deref()?);
            Some(format!(
                "100.0 * {v} / NULLIF(SUM({v}) OVER ({}), 0)",
                part.trim_end()
            ))
        }
        CalcKind::RunningTotal => {
            let v = exact(c.value.as_deref()?);
            Some(format!(
                "SUM({v}) OVER ({part}{} ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)",
                order_by(c.order.as_deref()?)
            ))
        }
        CalcKind::DiffPrev | CalcKind::PctChangePrev if e == DbEngine::ClickHouse => None,
        CalcKind::DiffPrev | CalcKind::PctChangePrev => {
            let col = c.value.as_deref()?;
            let v = if c.kind == CalcKind::DiffPrev {
                exact(col)
            } else {
                num(col)
            };
            let prev = format!("LAG({v}) OVER ({part}{})", order_by(c.order.as_deref()?));
            Some(if c.kind == CalcKind::DiffPrev {
                format!("{v} - {prev}")
            } else {
                format!("100.0 * ({v} - {prev}) / NULLIF({prev}, 0)")
            })
        }
        CalcKind::Previous | CalcKind::Next if e == DbEngine::ClickHouse => None,
        CalcKind::Previous | CalcKind::Next => {
            let f = if c.kind == CalcKind::Previous {
                "LAG"
            } else {
                "LEAD"
            };
            Some(format!(
                "{f}({}, {}) OVER ({part}{})",
                q(c.value.as_deref()?),
                c.extras.offset(),
                order_by(c.order.as_deref()?)
            ))
        }
        // ponytail: a visit number is a running count over a LAG, a window
        // inside a window, which one SELECT expression cannot hold. Computed
        // on the loaded rows; a subquery path if a partly loaded table needs it.
        CalcKind::Session => None,
        CalcKind::Abc => {
            let v = num(c.value.as_deref()?);
            let (a, b) = c.extras.abc();
            let total = format!("NULLIF(SUM({v}) OVER ({}), 0)", part.trim_end());
            // The share before the row, as the local engine counts it.
            // ponytail: a negative value is refused locally and not here, the
            // server would have to read the whole column first.
            let before = format!(
                "(SUM({v}) OVER ({part}ORDER BY CASE WHEN {v} IS NULL THEN 1 ELSE 0 END, {v} DESC \
                 ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) - {v})"
            );
            Some(format!(
                "CASE WHEN {v} IS NULL OR {total} IS NULL THEN NULL \
                 WHEN 100.0 * {before} / {total} < {a} THEN 'A' \
                 WHEN 100.0 * {before} / {total} < {b} THEN 'B' ELSE 'C' END"
            ))
        }
    }
}

#[cfg(test)]
#[path = "calc_tests.rs"]
mod tests;
