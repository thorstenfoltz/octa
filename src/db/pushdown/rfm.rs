//! Customer segments (RFM) on a database tab: the server only adds up, in one
//! `GROUP BY` customer (latest date as epoch seconds, orders, the amount
//! through `dialect::sum_operand`, so a decimal column sums exactly). The
//! marks and names come from the same `data::rfm::table` as the local rows,
//! so the two paths cannot disagree. On a server only NULL is empty.

use std::sync::atomic::AtomicBool;

use crate::data::CellValue;
use crate::data::decimal::{Exact, server_decimal_cell};
use crate::data::rfm::{Customer, Totals, day_of_seconds};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{epoch_seconds, sum_operand};
use super::{ServerSource, cell_f64, cell_i64, check_cancel};

/// What to add up, by column name (quoted inside).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerRfm<'a> {
    pub customer: &'a str,
    /// A typed date or timestamp column.
    pub date: &'a str,
    pub amount: &'a str,
    /// The tab's type for `amount`: a decimal adds exactly.
    pub amount_type: &'a str,
    /// `None`: every row is one order. A row without an order number is an
    /// order of its own.
    pub order: Option<&'a str>,
    /// Purchases after this day are left out.
    pub as_of: Option<chrono::NaiveDate>,
}

/// The two statements: per customer (`octa_c`, `octa_t`, `octa_n`,
/// `octa_s`), and the skipped rows.
pub fn sql(e: DbEngine, from: &str, spec: &ServerRfm) -> [String; 2] {
    let q = |n: &str| e.quote_ident(n);
    let (c, d) = (q(spec.customer), q(spec.date));
    let orders = spec.order.map_or("COUNT(*)".to_string(), |o| {
        let o = q(o);
        format!("COUNT(DISTINCT {o}) + SUM(CASE WHEN {o} IS NULL THEN 1 ELSE 0 END)")
    });
    let secs = epoch_seconds(e, &d);
    // Before the day after As of, compared as epoch seconds: a plain number
    // on every engine.
    let before = spec.as_of.map_or(String::new(), |day| {
        let next = day.succ_opt().unwrap_or(day);
        let cut = next
            .and_hms_opt(0, 0, 0)
            .map_or(0, |t| t.and_utc().timestamp());
        format!(" AND {secs} < {cut}")
    });
    [
        format!(
            "SELECT {c} AS octa_c, MAX({secs}) AS octa_t, {orders} AS octa_n, \
             SUM({amount}) AS octa_s FROM {from} \
             WHERE {c} IS NOT NULL AND {d} IS NOT NULL{before} GROUP BY {c}",
            amount = sum_operand(e, &q(spec.amount), spec.amount_type),
        ),
        format!("SELECT COUNT(*) FROM {from} WHERE {c} IS NULL OR {d} IS NULL"),
    ]
}

/// A server sum as the local engine's `Sum` writes it (`Exact::to_cell`):
/// whole numbers as `Int`, a decimal's digits exact, nothing summed as 0.
fn amount_cell(v: Option<&CellValue>) -> CellValue {
    match v {
        None | Some(CellValue::Null) => CellValue::Int(0),
        Some(CellValue::Float(f)) => Exact::of_f64(*f).map_or(CellValue::Float(*f), Exact::to_cell),
        Some(CellValue::String(s)) => Exact::parse(s.trim())
            .map(Exact::to_cell)
            .unwrap_or_else(|| server_decimal_cell(s)),
        Some(other) => other.clone(),
    }
}

/// Run both statements and gather what `data::rfm::table` needs.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    spec: &ServerRfm,
    cancel: &AtomicBool,
) -> anyhow::Result<Totals> {
    let [main, skipped_sql] = sql(src.engine(), &src.from_sql(), spec);
    check_cancel(cancel)?;
    let t = c.query(&main)?;
    check_cancel(cancel)?;
    let k = c.query(&skipped_sql)?;
    let customers = t
        .rows
        .iter()
        .filter_map(|row| {
            Some(Customer {
                id: row.first()?.to_string(),
                last: day_of_seconds(row.get(1).and_then(cell_f64)?)?,
                orders: row.get(2).and_then(cell_i64).unwrap_or(0),
                amount: amount_cell(row.get(3)),
            })
        })
        .collect();
    Ok(Totals {
        customers,
        skipped: k.get(0, 0).and_then(cell_i64).unwrap_or(0).max(0) as usize,
    })
}

#[cfg(test)]
#[path = "rfm_tests.rs"]
mod tests;
