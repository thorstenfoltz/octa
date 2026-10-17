//! Join key finder on the server, and the key-matching statements the other
//! key analyses share (`rel_measure`, `join_diag`).
//!
//! Every column's distinct keys are unpivoted into `(ci, kv)` rows with
//! `UNION ALL` and joined on `kv`, grouped by column pair: one statement per
//! pair of tables gives every exact shared count. Plain joins only, no
//! correlated subquery, so ClickHouse runs it too.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::AtomicBool;

use super::{ServerSource, cell_i64, check_cancel, dialect, per_column_aggregates};
use crate::data::ColumnInfo;
use crate::data::join_keys::{KeyCandidate, MIN_SCORE, candidate, score_counts, sort_candidates};
use crate::db::{DbConnector, DbEngine};

/// `UNION ALL` branches per side in one statement. Oracle and SQL Server
/// refuse very deep statements long before any documented limit.
pub(crate) const MAX_UNION_BRANCHES: usize = 64;

/// `(distinct non-NULL values, non-NULL rows)` of each expression over `from`.
/// A column the server refuses counts as empty.
pub(crate) fn expr_stats(
    c: &mut dyn DbConnector,
    from: &str,
    exprs: &[String],
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<(usize, usize)>> {
    let engine = c.engine();
    let per_col: Vec<Vec<String>> = exprs
        .iter()
        .map(|e| {
            let e = dialect::exact(engine, e);
            vec![format!("COUNT(DISTINCT {e})"), format!("COUNT({e})")]
        })
        .collect();
    let n = |v: &[crate::data::CellValue], i: usize| {
        v.get(i).and_then(cell_i64).unwrap_or(0).max(0) as usize
    };
    Ok(per_column_aggregates(c, from, &per_col, cancel)?
        .into_iter()
        .map(|cells| cells.map(|v| (n(&v, 0), n(&v, 1))).unwrap_or((0, 0)))
        .collect())
}

/// Branches per chunk when the connector returns at most `row_cap` rows: a
/// chunk pairing answers one row per sharing pair, up to size x size, and
/// rows past the cap would be dropped without a word.
fn chunk_size(row_cap: usize) -> usize {
    MAX_UNION_BRANCHES.min(row_cap.isqrt()).max(1)
}

fn chunks(n: usize) -> Vec<Range<usize>> {
    let size = chunk_size(crate::formats::initial_load_rows());
    (0..n).step_by(size).map(|s| s..(s + size).min(n)).collect()
}

/// `SELECT DISTINCT <i> AS ci, kv ...` per expression, joined by UNION ALL.
fn distinct_keys(engine: DbEngine, from: &str, exprs: &[String], at: usize, side: &str) -> String {
    exprs
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let i = at + i;
            let e = dialect::exact(engine, e);
            format!(
                "SELECT DISTINCT {i} AS ci, kv FROM (SELECT {e} AS kv FROM {from}){} WHERE kv IS NOT NULL",
                dialect::subquery_alias(engine, &format!("{side}{i}"))
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ")
}

/// One side of a key match: the `FROM` target and one expression per key.
#[derive(Clone, Copy)]
pub(crate) struct KeySide<'a> {
    pub(crate) from: &'a str,
    pub(crate) exprs: &'a [String],
}

/// How many distinct non-NULL values expression `i` of `a` shares with
/// expression `j` of `b`, keyed `(i, j)`; pairs sharing nothing are absent.
/// `diagonal` pairs only `i == j` (one normalisation per index).
///
/// Callers drop the expressions whose [`expr_stats`] are `(0, 0)` first: a
/// column the server refuses fails the whole statement, not just its pair.
pub(crate) fn shared_counts(
    c: &mut dyn DbConnector,
    engine: DbEngine,
    a: KeySide,
    b: KeySide,
    diagonal: bool,
    cancel: &AtomicBool,
) -> anyhow::Result<HashMap<(usize, usize), usize>> {
    let pairs: Vec<(Range<usize>, Range<usize>)> = if diagonal {
        chunks(a.exprs.len().min(b.exprs.len()))
            .into_iter()
            .map(|r| (r.clone(), r))
            .collect()
    } else {
        let bs = chunks(b.exprs.len());
        chunks(a.exprs.len())
            .into_iter()
            .flat_map(|ra| bs.iter().map(move |rb| (ra.clone(), rb.clone())))
            .collect()
    };
    let on = if diagonal { " AND a.ci = b.ci" } else { "" };
    let mut out = HashMap::new();
    for (ra, rb) in pairs {
        check_cancel(cancel)?;
        let sql = format!(
            "SELECT a.ci AS ai, b.ci AS bi, COUNT(*) AS n FROM ({}){} INNER JOIN ({}){} \
             ON a.kv = b.kv{on} GROUP BY a.ci, b.ci",
            distinct_keys(engine, a.from, &a.exprs[ra.clone()], ra.start, "xa"),
            dialect::subquery_alias(engine, "a"),
            distinct_keys(engine, b.from, &b.exprs[rb.clone()], rb.start, "xb"),
            dialect::subquery_alias(engine, "b"),
        );
        let t = c.query(&sql)?;
        for r in 0..t.row_count() {
            let n = |i: usize| t.get(r, i).and_then(cell_i64).map(|v| v.max(0) as usize);
            if let (Some(i), Some(j), Some(k)) = (n(0), n(1), n(2)) {
                out.insert((i, j), k);
            }
        }
    }
    Ok(out)
}

/// `(table, column)` on each side of a pairing.
pub type ColPair = ((usize, usize), (usize, usize));

/// The numbers `score_counts` needs for one pairing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Counts {
    pub(crate) lv: usize,
    pub(crate) ls: usize,
    pub(crate) rv: usize,
    pub(crate) rs: usize,
    pub(crate) shared: usize,
}

/// Rank column pairs across every pair of `tables`, best first: the same
/// result `suggest_keys` gives over every row. All tables are on one
/// connection (the caller checks).
pub fn run(
    c: &mut dyn DbConnector,
    tables: &[(ServerSource, Vec<ColumnInfo>)],
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<KeyCandidate>> {
    let Some(engine) = tables.first().map(|(s, _)| s.engine()) else {
        return Ok(Vec::new());
    };
    if tables.len() < 2 {
        return Ok(Vec::new());
    }
    let froms: Vec<String> = tables.iter().map(|(s, _)| s.from_sql()).collect();
    let mut stats = Vec::with_capacity(tables.len());
    // Per table: the column indices with any key, and their expressions.
    // Empty or refused columns (stats (0, 0)) stay out of the match
    // statements, where one refused column would fail them all.
    let mut live: Vec<(Vec<usize>, Vec<String>)> = Vec::with_capacity(tables.len());
    for ((_, cols), from) in tables.iter().zip(&froms) {
        let exprs: Vec<String> = cols
            .iter()
            .map(|col| dialect::key_text(engine, &engine.quote_ident(&col.name)))
            .collect();
        let s = expr_stats(c, from, &exprs, cancel)?;
        live.push(
            exprs
                .into_iter()
                .enumerate()
                .filter(|&(i, _)| s[i].0 > 0)
                .unzip(),
        );
        stats.push(s);
    }
    let mut out = Vec::new();
    // Same loop order as `suggest_keys`, so the stable sort ties alike.
    for ti in 0..tables.len() {
        for tj in (ti + 1)..tables.len() {
            let ((li, le), (ri, re)) = (&live[ti], &live[tj]);
            if le.is_empty() || re.is_empty() {
                continue;
            }
            let shared: HashMap<(usize, usize), usize> = shared_counts(
                c,
                engine,
                KeySide {
                    from: &froms[ti],
                    exprs: le,
                },
                KeySide {
                    from: &froms[tj],
                    exprs: re,
                },
                false,
                cancel,
            )?
            .into_iter()
            .map(|((i, j), n)| ((li[i], ri[j]), n))
            .collect();
            for (ci, &(lv, ls)) in stats[ti].iter().enumerate() {
                if lv == 0 {
                    continue;
                }
                for (cj, &(rv, rs)) in stats[tj].iter().enumerate() {
                    let n = shared.get(&(ci, cj)).copied().unwrap_or(0);
                    let Some(p) = score_counts(lv, ls, rv, rs, n) else {
                        continue;
                    };
                    if p.score >= MIN_SCORE {
                        out.push(candidate((ti, ci), (tj, cj), p));
                    }
                }
            }
        }
    }
    sort_candidates(&mut out);
    Ok(out)
}

/// Exact counts for chosen pairings: one stats statement per table for the
/// columns the pairs use, one match statement per pair with keys on both
/// sides. `froms[t]` and `names[t]` describe table `t`.
pub(crate) fn pair_counts(
    c: &mut dyn DbConnector,
    engine: DbEngine,
    froms: &[String],
    names: &[Vec<String>],
    pairs: &[ColPair],
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<Counts>> {
    let key =
        |(t, col): (usize, usize)| dialect::key_text(engine, &engine.quote_ident(&names[t][col]));
    let mut stats: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    for (t, from) in froms.iter().enumerate() {
        let mut cols: Vec<usize> = pairs
            .iter()
            .flat_map(|&(l, r)| [l, r])
            .filter(|&(n, _)| n == t)
            .map(|(_, col)| col)
            .collect();
        cols.sort_unstable();
        cols.dedup();
        if cols.is_empty() {
            continue;
        }
        let exprs: Vec<String> = cols.iter().map(|&col| key((t, col))).collect();
        for (&col, s) in cols.iter().zip(expr_stats(c, from, &exprs, cancel)?) {
            stats.insert((t, col), s);
        }
    }
    let mut out = Vec::with_capacity(pairs.len());
    for &(l, r) in pairs {
        let (lv, ls) = stats.get(&l).copied().unwrap_or((0, 0));
        let (rv, rs) = stats.get(&r).copied().unwrap_or((0, 0));
        // An empty or refused side shares nothing; asking would only fail.
        let shared = if lv == 0 || rv == 0 {
            0
        } else {
            let (kl, kr) = (key(l), key(r));
            shared_counts(
                c,
                engine,
                KeySide {
                    from: &froms[l.0],
                    exprs: std::slice::from_ref(&kl),
                },
                KeySide {
                    from: &froms[r.0],
                    exprs: std::slice::from_ref(&kr),
                },
                false,
                cancel,
            )?
            .get(&(0, 0))
            .copied()
            .unwrap_or(0)
        };
        out.push(Counts {
            lv,
            ls,
            rv,
            rs,
            shared,
        });
    }
    Ok(out)
}

/// Exact numbers for just `pairs` (the ones the loaded rows suggested),
/// ranked like `run`. A pair whose exact score falls under `MIN_SCORE` is
/// dropped, as `suggest_keys` would drop it. Pass the pairs in the loaded-row
/// ranking's order: equal scores keep it.
pub fn rescore(
    c: &mut dyn DbConnector,
    tables: &[(ServerSource, Vec<ColumnInfo>)],
    pairs: &[ColPair],
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<KeyCandidate>> {
    let Some(engine) = tables.first().map(|(s, _)| s.engine()) else {
        return Ok(Vec::new());
    };
    let froms: Vec<String> = tables.iter().map(|(s, _)| s.from_sql()).collect();
    let names: Vec<Vec<String>> = tables
        .iter()
        .map(|(_, cols)| cols.iter().map(|c| c.name.clone()).collect())
        .collect();
    let counts = pair_counts(c, engine, &froms, &names, pairs, cancel)?;
    let mut out: Vec<KeyCandidate> = pairs
        .iter()
        .zip(counts)
        .filter_map(|(&(l, r), n)| {
            score_counts(n.lv, n.ls, n.rv, n.rs, n.shared)
                .filter(|p| p.score >= MIN_SCORE)
                .map(|p| candidate(l, r, p))
        })
        .collect();
    sort_candidates(&mut out);
    Ok(out)
}

#[cfg(test)]
#[path = "join_keys_tests.rs"]
pub(crate) mod tests;
