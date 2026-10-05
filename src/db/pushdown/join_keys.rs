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
pub(crate) mod tests {
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
}
