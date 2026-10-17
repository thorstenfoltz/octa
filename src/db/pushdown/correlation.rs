//! Correlation on the server.
//!
//! Pearson is one statement: `CORR` (or the six sums) for every column pair.
//! Spearman is the Pearson correlation of average ranks, which must be taken
//! over the rows where *both* columns are present. Ranking each column once
//! over its own non-NULL rows is the same thing exactly when the two columns
//! are NULL in the same rows, which a count query tells us; those pairs share
//! one statement, and only the rest get a statement of their own.
//!
//! The caller picks the columns (`numeric_columns` over the loaded rows, so a
//! NUMERIC that reaches the tab as text still takes part). A column whose
//! server values do not cast to a number makes the server raise, which surfaces.
//!
//! Derived-table aliases are `octa_*` so they never collide with a source
//! column (ClickHouse resolves aliases ahead of columns).

use std::sync::atomic::AtomicBool;

use crate::data::ColumnInfo;
use crate::data::correlation::{CorrMatrix, CorrMethod};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{as_number, avg_rank, pearson_read, pearson_select, subquery_alias};
use super::{MAX_SELECT_ITEMS, ServerSource, cell_i64, check_cancel, width_chunks};

/// Answer `pairs` with one statement over `x(i)` per column, from `target`.
fn pair_statement(
    c: &mut dyn DbConnector,
    e: DbEngine,
    target: &str,
    pairs: &[(usize, usize)],
    x: &dyn Fn(usize) -> String,
    matrix: &mut [Vec<Option<f64>>],
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    if pairs.is_empty() {
        return Ok(());
    }
    let sel: Vec<(String, usize)> = pairs
        .iter()
        .map(|&(i, j)| pearson_select(e, &x(i), &x(j)))
        .collect();
    // A wide table has n*(n+1)/2 pairs: split them under the select cap.
    let widths: Vec<usize> = sel.iter().map(|(_, w)| *w).collect();
    for range in width_chunks(&widths) {
        let body = sel[range.clone()]
            .iter()
            .map(|(s, _)| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        check_cancel(cancel)?;
        let t = c.query(&format!("SELECT {body} FROM {target}"))?;
        let mut at = 0;
        for (&(i, j), (_, w)) in pairs[range.clone()].iter().zip(&sel[range]) {
            let r = pearson_read(&t, at, *w);
            matrix[i][j] = r;
            matrix[j][i] = r;
            at += w;
        }
    }
    Ok(())
}

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    columns: &[ColumnInfo],
    method: CorrMethod,
    cancel: &AtomicBool,
) -> anyhow::Result<CorrMatrix> {
    let e = src.engine();
    let from = src.from_sql();
    let quoted: Vec<String> = columns.iter().map(|c| e.quote_ident(&c.name)).collect();
    // Each column as a double; a Boolean as 1.0/0.0 (see `as_number`).
    let num: Vec<String> = columns
        .iter()
        .zip(&quoted)
        .map(|(c, q)| as_number(e, q, &c.data_type))
        .collect();
    let n = columns.len();
    let mut matrix = vec![vec![None; n]; n];
    let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i..n).map(move |j| (i, j))).collect();
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    if n == 0 {
        return Ok(CorrMatrix {
            columns: names,
            matrix,
        });
    }

    match method {
        CorrMethod::Pearson => {
            let x = |i: usize| num[i].clone();
            pair_statement(c, e, &from, &pairs, &x, &mut matrix, cancel)?;
        }
        CorrMethod::Spearman => {
            // Non-NULL count per column, then per pair.
            let mut counts: Vec<String> = quoted.iter().map(|q| format!("COUNT({q})")).collect();
            counts.extend(pairs.iter().map(|&(i, j)| {
                format!(
                    "SUM(CASE WHEN {} IS NOT NULL AND {} IS NOT NULL THEN 1 ELSE 0 END)",
                    quoted[i], quoted[j]
                )
            }));
            let mut got: Vec<i64> = Vec::with_capacity(counts.len());
            for chunk in counts.chunks(MAX_SELECT_ITEMS) {
                check_cancel(cancel)?;
                let t = c.query(&format!("SELECT {} FROM {from}", chunk.join(", ")))?;
                got.extend((0..chunk.len()).map(|k| t.get(0, k).and_then(cell_i64).unwrap_or(0)));
            }
            let k = |col: usize| got[col];
            let (same, apart): (Vec<_>, Vec<_>) = pairs
                .iter()
                .enumerate()
                .partition(|(p, (i, j))| k(n + p) == k(*i) && k(n + p) == k(*j));
            let same: Vec<(usize, usize)> = same.into_iter().map(|(_, p)| *p).collect();
            let apart: Vec<(usize, usize)> = apart.into_iter().map(|(_, p)| *p).collect();

            if !same.is_empty() {
                let ranks: Vec<String> = num
                    .iter()
                    .enumerate()
                    .map(|(i, x)| format!("{} AS octa_r{i}", avg_rank(e, x)))
                    .collect();
                let target = format!(
                    "(SELECT {} FROM {from}){}",
                    ranks.join(", "),
                    subquery_alias(e, "octa_s")
                );
                let r = |i: usize| format!("octa_r{i}");
                pair_statement(c, e, &target, &same, &r, &mut matrix, cancel)?;
            }

            // ponytail: one statement per pair whose NULLs fall in different
            // rows; a wide table with scattered NULLs is many round trips.
            // UNION ALL them into one statement if that ever hurts.
            for &(i, j) in &apart {
                let (qi, qj) = (&quoted[i], &quoted[j]);
                let target = format!(
                    "(SELECT {} AS octa_rx, {} AS octa_ry FROM {from} \
                     WHERE {qi} IS NOT NULL AND {qj} IS NOT NULL){}",
                    avg_rank(e, &num[i]),
                    avg_rank(e, &num[j]),
                    subquery_alias(e, "octa_s")
                );
                let xy = |k: usize| if k == i { "octa_rx" } else { "octa_ry" }.to_string();
                pair_statement(c, e, &target, &[(i, j)], &xy, &mut matrix, cancel)?;
            }
        }
    }
    Ok(CorrMatrix {
        columns: names,
        matrix,
    })
}

#[cfg(test)]
#[path = "correlation_tests.rs"]
mod tests;
