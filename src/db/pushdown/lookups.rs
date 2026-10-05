//! Find lookup tables on the server: which columns follow a key, counted
//! over every row.
//!
//! Per key, one statement with one `UNION ALL` branch per candidate column
//! groups by (key, value), then by key, and sums the rows of repeated keys,
//! the rows off their key's most common value, and the keys with more than
//! one value. The budget and sort order are `crate::data::lookups`'s own.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use super::join_keys::MAX_UNION_BRANCHES;
use super::{ServerSource, cell_i64, check_cancel, dialect, per_column_aggregates};
use crate::data::lookups::{LookupFinding, Split, dependent_from_counts, finish};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnector, DbEngine};

/// `dep, covered, breaking, conflicting` for key `k` and column `d`.
/// Aliases avoid `TOP` (reserved on SQL Server). The inner ones carry an
/// `octa_` prefix: ClickHouse resolves an alias before a same-named column,
/// so a user column called `kv` or `n` would be shadowed inside `{k}`/`{d}`.
fn branch(engine: DbEngine, from: &str, k: &str, d: &str, dep: usize) -> String {
    format!(
        "SELECT {dep} AS dep, SUM(octa_sz) AS covered, SUM(octa_sz - octa_mx) AS breaking, \
         SUM(CASE WHEN octa_nd > 1 THEN 1 ELSE 0 END) AS conflicting \
         FROM (SELECT SUM(octa_n) AS octa_sz, MAX(octa_n) AS octa_mx, COUNT(*) AS octa_nd \
         FROM (SELECT {k} AS octa_kv, {d} AS octa_dv, COUNT(*) AS octa_n \
         FROM {from} GROUP BY {k}, {d}){} \
         GROUP BY octa_kv HAVING SUM(octa_n) > 1){}",
        dialect::subquery_alias(engine, &format!("g{dep}")),
        dialect::subquery_alias(engine, &format!("kk{dep}")),
    )
}

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    columns: &[ColumnInfo],
    total: usize,
    min_consistency: f64,
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<LookupFinding>> {
    let engine = src.engine();
    let from = src.from_sql();
    let g: Vec<String> = columns
        .iter()
        .map(|col| grouped(engine, &engine.quote_ident(&col.name)))
        .collect();
    // Distinct values with NULL/'' counted as one value, as in memory.
    let per_col: Vec<Vec<String>> = g
        .iter()
        .map(|e| {
            vec![
                format!("COUNT(DISTINCT {e})"),
                format!("MAX(CASE WHEN {e} IS NULL THEN 1 ELSE 0 END)"),
            ]
        })
        .collect();
    let distinct: Vec<usize> = per_column_aggregates(c, &from, &per_col, cancel)?
        .into_iter()
        .map(|cells| {
            cells
                .map(|v| v.iter().filter_map(cell_i64).sum::<i64>().max(0) as usize)
                .unwrap_or(0)
        })
        .collect();
    let mut findings = Vec::new();
    for (key, &d) in distinct.iter().enumerate() {
        // A key must repeat; a unique column "determines" everything.
        if d < 2 || d * 2 > total {
            continue;
        }
        let deps: Vec<usize> = (0..columns.len())
            .filter(|&x| x != key && distinct[x] >= 2)
            .collect();
        let mut dependents = Vec::new();
        for chunk in deps.chunks(MAX_UNION_BRANCHES) {
            check_cancel(cancel)?;
            let sql = chunk
                .iter()
                .map(|&dep| branch(engine, &from, &g[key], &g[dep], dep))
                .collect::<Vec<_>>()
                .join(" UNION ALL ");
            let t = c.query(&sql)?;
            for r in 0..t.row_count() {
                let n = |i: usize| t.get(r, i).and_then(cell_i64).unwrap_or(0).max(0) as usize;
                if let Some(x) = dependent_from_counts(n(0), n(1), n(2), n(3), min_consistency) {
                    dependents.push(x);
                }
            }
        }
        if !dependents.is_empty() {
            findings.push(LookupFinding {
                key,
                keys: d,
                dependents,
            });
        }
    }
    finish(&mut findings);
    Ok(findings)
}

/// A cell as `run` groups it: untrimmed text, NULL and `''` one value,
/// compared byte for byte. Every query here uses it, so the rows fetched
/// agree with the counts `run` reported.
fn grouped(engine: DbEngine, col: &str) -> String {
    dialect::exact(engine, &dialect::group_text(engine, col))
}

/// The `cap` to pass to [`breaking_rows`] and [`split_out`] when every
/// connector stops reading at `row_cap` rows (the process-wide
/// `initial_load_rows`): one below it, so the extra row [`cap_clause`] asks
/// for still arrives and a cut is detected. Passing `row_cap` itself would
/// hide every cut. `usize::MAX` (Unlimited) stays unlimited.
pub fn fetch_cap(row_cap: usize) -> usize {
    row_cap.saturating_sub(1).max(1)
}

/// `limit_clause` for `cap + 1` rows, so a cut can be detected; nothing
/// for an unlimited cap (no engine spells a limit past i64).
pub(crate) fn cap_clause(engine: DbEngine, cap: usize) -> String {
    if cap >= i64::MAX as usize {
        String::new()
    } else {
        dialect::limit_clause(engine, cap + 1)
    }
}

pub(crate) fn cut(mut t: DataTable, cap: usize) -> (DataTable, bool) {
    let capped = t.rows.len() > cap;
    t.rows.truncate(cap);
    (t, capped)
}

/// Every row of every key whose `deps` disagree, ordered by key. The keys
/// come from one grouped subquery (a dependent with more than one value,
/// NULL and '' counted as one); the rows from joining them back on two
/// plain equalities, because a NULL key is a key here and `a = b OR (a IS
/// NULL AND b IS NULL)` cannot be hash-joined. The key's text is compared
/// through `COALESCE(.., 'x')` and its NULL-ness (`octa_kn`) separately, so
/// a real key `'x'` never meets the NULL key. `'x'` is not empty because
/// Oracle reads `''` as NULL. Aliases carry an `octa_` prefix against
/// ClickHouse resolving an alias before a same-named column.
pub fn breaking_rows(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    key: &str,
    deps: &[String],
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, bool)> {
    check_cancel(cancel)?;
    let engine = src.engine();
    let from = src.from_sql();
    let g =
        |prefix: &str, col: &str| grouped(engine, &format!("{prefix}{}", engine.quote_ident(col)));
    let dep_cols: Vec<String> = deps
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{} AS octa_d{i}", g("", d)))
        .collect();
    let disagree: Vec<String> = (0..deps.len())
        .map(|i| {
            format!(
                "COUNT(DISTINCT octa_d{i}) + MAX(CASE WHEN octa_d{i} IS NULL THEN 1 ELSE 0 END) > 1"
            )
        })
        .collect();
    let keys = format!(
        "SELECT octa_kv, CASE WHEN octa_kv IS NULL THEN 1 ELSE 0 END AS octa_kn \
         FROM (SELECT {} AS octa_kv, {} FROM {from}){} GROUP BY octa_kv HAVING {}",
        g("", key),
        dep_cols.join(", "),
        dialect::subquery_alias(engine, "s"),
        disagree.join(" OR "),
    );
    let xk = g("x.", key);
    // SQL Server's `=` ignores trailing spaces even under `_BIN2`, so there
    // `'b2 '` joins key `'b2'`. MySQL's `CAST(.. AS BINARY)` compares every
    // byte, trailing spaces included.
    let fx = src.from_as("x");
    let sql = format!(
        "SELECT x.* FROM {fx} INNER JOIN ({keys}){} \
         ON COALESCE({xk}, 'x') = COALESCE(k.octa_kv, 'x') \
         AND CASE WHEN {xk} IS NULL THEN 1 ELSE 0 END = k.octa_kn ORDER BY {xk}{}",
        dialect::subquery_alias(engine, "k"),
        cap_clause(engine, cap),
    );
    Ok(cut(c.query(&sql)?, cap))
}

/// The map key for a `kg` cell. `kg` is never '' (NULLIF), so NULL's "" names
/// only the NULL key. SQL Server's `_BIN2` GROUP BY ignores trailing spaces,
/// so `'b2'` and `'b2 '` are one group, but each dependent's query may return
/// either spelling: drop the spaces there, or one key becomes two half-NULL
/// lookup rows.
fn group_key(engine: DbEngine, kg: &str) -> String {
    if matches!(engine, DbEngine::Mssql) {
        kg.trim_end_matches(' ').to_string()
    } else {
        kg.to_string()
    }
}

/// The lookup table and the table without `deps`, from the server.
///
/// Per dependent, one statement groups by the grouped text of key and
/// value (as `run` does) and by the raw columns, so the lookup keeps the
/// server's types: within one (key, value) group the raw form with the
/// most rows stands for it (NULL vs '', or trailing spaces on SQL
/// Server). Then `ROW_NUMBER()` picks each key's most common value; ties
/// go to the smaller value as text (in memory they go to the value seen
/// first, which a server has no notion of).
pub fn split_out(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    key: &str,
    deps: &[String],
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(Split, bool)> {
    let engine = src.engine();
    let from = src.from_sql();
    let k = engine.quote_ident(key);
    let kg = grouped(engine, &k);
    let mut order: Vec<String> = Vec::new();
    let mut by_key: HashMap<String, (CellValue, Vec<CellValue>, bool)> = HashMap::new();
    let mut capped = false;
    let fallback = |name: &str| ColumnInfo {
        name: name.to_string(),
        data_type: "Utf8".into(),
    };
    let mut columns = Vec::new();
    for (i, dep) in deps.iter().enumerate() {
        check_cancel(cancel)?;
        let d = engine.quote_ident(dep);
        let dg = grouped(engine, &d);
        // Columns read by position: kg, kv, dv, nd. The `octa_` prefix keeps
        // ClickHouse from resolving a user column named `kg` to an alias.
        let sql = format!(
            "SELECT octa_kg, octa_kv, octa_dv, octa_nd FROM (\
             SELECT octa_kg, octa_kv, octa_dv, \
             ROW_NUMBER() OVER (PARTITION BY octa_kg ORDER BY octa_n DESC, octa_dg) AS octa_rn, \
             COUNT(*) OVER (PARTITION BY octa_kg) AS octa_nd FROM (\
             SELECT octa_kg, octa_dg, octa_kv, octa_dv, \
             SUM(octa_c) OVER (PARTITION BY octa_kg, octa_dg) AS octa_n, \
             ROW_NUMBER() OVER (PARTITION BY octa_kg, octa_dg ORDER BY octa_c DESC) AS octa_pick \
             FROM (SELECT {kg} AS octa_kg, {dg} AS octa_dg, {k} AS octa_kv, {d} AS octa_dv, \
             COUNT(*) AS octa_c FROM {from} GROUP BY {kg}, {dg}, {k}, {d}){}){} \
             WHERE octa_pick = 1){} WHERE octa_rn = 1 ORDER BY octa_kg{}",
            dialect::subquery_alias(engine, "g"),
            dialect::subquery_alias(engine, "p"),
            dialect::subquery_alias(engine, "r"),
            cap_clause(engine, cap),
        );
        let (t, cut_here) = cut(c.query(&sql)?, cap);
        capped |= cut_here;
        if i == 0 {
            let mut col = t.columns.get(1).cloned().unwrap_or_else(|| fallback(key));
            col.name = key.to_string();
            columns.push(col);
        }
        let mut col = t.columns.get(2).cloned().unwrap_or_else(|| fallback(dep));
        col.name = dep.clone();
        columns.push(col);
        for r in 0..t.row_count() {
            let cell = |i: usize| t.get(r, i).cloned().unwrap_or(CellValue::Null);
            let conflict = t.get(r, 3).and_then(cell_i64).unwrap_or(1) > 1;
            let group = group_key(engine, &cell(0).to_string());
            let entry = by_key.entry(group.clone()).or_insert_with(|| {
                order.push(group);
                (cell(1), vec![CellValue::Null; deps.len()], false)
            });
            entry.1[i] = cell(2);
            entry.2 |= conflict;
        }
    }
    let mut lookup = DataTable::empty();
    lookup.columns = columns;
    let mut resolved_keys = 0;
    lookup.rows = order
        .iter()
        .filter_map(|g| by_key.remove(g))
        .map(|(kv, vals, conflict)| {
            resolved_keys += usize::from(conflict);
            std::iter::once(kv).chain(vals).collect()
        })
        .collect();

    check_cancel(cancel)?;
    let sql = crate::db::select_sample_sql(
        engine,
        src.catalog.as_deref(),
        &src.schema,
        &src.table,
        if cap >= i64::MAX as usize {
            usize::MAX
        } else {
            cap + 1
        },
    );
    let (all, cut_main) = cut(c.query(&sql)?, cap);
    capped |= cut_main;
    let keep: Vec<usize> = (0..all.col_count())
        .filter(|&i| !deps.contains(&all.columns[i].name))
        .collect();
    let mut main = DataTable::empty();
    main.columns = keep.iter().map(|&i| all.columns[i].clone()).collect();
    main.rows = all
        .rows
        .iter()
        .map(|r| {
            keep.iter()
                .map(|&i| r.get(i).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    Ok((
        Split {
            lookup,
            main,
            resolved_keys,
        },
        capped,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::join_keys::tests::text_table;
    use super::super::test_support::{DuckConn, source};
    use super::*;

    #[test]
    fn the_server_finds_the_same_lookups() {
        // cust -> name holds except one row; cust -> city holds; NULL and ''
        // are one key value, as in memory.
        let t = text_table(
            &["cust", "name", "city", "amount"],
            &[
                &["1", "ada", "x", "10"],
                &["1", "ada", "x", "11"],
                &["1", "adx", "x", "12"],
                &["2", "bob", "y", "13"],
                &["2", "bob", "y", "14"],
                &["3", "cy", "z", "15"],
                &["3", "cy", "z", "16"],
                &["NULL", "eve", "w", "17"],
                &["", "eve", "w", "18"],
                &["4", "dee", "v", "19"],
                &["4", "dee", "v", "20"],
                &["4", "dee", "v", "21"],
            ],
        );
        let stop = AtomicBool::new(false);
        let want = crate::data::lookups::find_lookups(&t, 0.8, &stop);
        assert!(!want.is_empty());
        let cols = t.columns.clone();
        let n = t.row_count();
        let mut c = DuckConn::new(t);
        let got = run(&mut c, &source(), &cols, n, 0.8, &stop).unwrap();
        assert_eq!(got, want);
    }

    fn customers_flat() -> DataTable {
        // cust 1 has two names (breaks), cust 2 is clean, cust 3 has two
        // cities (breaks), NULL and '' keys are one key with two names
        // (breaks only because they are one; grouped raw, each is clean).
        text_table(
            &["cust", "name", "city"],
            &[
                &["1", "ada", "x"],
                &["1", "ada", "x"],
                &["1", "adx", "x"],
                &["2", "bob", "y"],
                &["2", "bob", "y"],
                &["3", "cy", "z"],
                &["3", "cy", "q"],
                &["NULL", "eve", "w"],
                &["", "eva", "w"],
            ],
        )
    }

    fn sorted_texts(t: &DataTable) -> Vec<Vec<String>> {
        let mut rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|r| r.iter().map(|v| v.to_string()).collect())
            .collect();
        rows.sort();
        rows
    }

    #[test]
    fn breaking_rows_match_the_in_memory_rows() {
        let t = customers_flat();
        let want = t.clone_with_rows(&crate::data::lookups::breaking_rows(&t, 0, &[1, 2]));
        let mut c = DuckConn::new(t);
        let deps = ["name".to_string(), "city".to_string()];
        let (got, capped) = breaking_rows(
            &mut c,
            &source(),
            "cust",
            &deps,
            usize::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!capped);
        assert_eq!(sorted_texts(&got), sorted_texts(&want));
        // Cust 1 (3 rows), cust 3 (2) and the NULL/'' key (2): the NULL key
        // only breaks because NULL and '' are one key, and both its rows must
        // come back through the NULL-safe join.
        assert_eq!(got.row_count(), 7);
        let blank_keys = got
            .rows
            .iter()
            .filter(|r| r[0].to_string().is_empty())
            .count();
        assert_eq!(blank_keys, 2);
        let (two, capped) =
            breaking_rows(&mut c, &source(), "cust", &deps, 2, &AtomicBool::new(false)).unwrap();
        assert!(capped);
        assert_eq!(two.row_count(), 2);
    }

    /// Every connector stops reading at the row cap, so a fetch asking for
    /// `row_cap + 1` rows got `row_cap` back and never saw its cut.
    #[test]
    fn the_fetch_cap_leaves_room_to_see_a_cut() {
        for row_cap in [2, 1_000, 2_000_000] {
            let cap = fetch_cap(row_cap);
            assert!(cap < row_cap, "LIMIT {} exceeds the cap {row_cap}", cap + 1);
        }
        assert_eq!(fetch_cap(1), 1);
        assert!(
            cap_clause(DbEngine::Postgres, fetch_cap(usize::MAX)).is_empty(),
            "Unlimited fetches every row"
        );
    }

    #[test]
    fn a_real_x_key_does_not_join_the_null_key() {
        // The join compares `COALESCE(key, 'x')` and the NULL-ness apart; the
        // NULL key breaks (NULL and '' are one key, two names), the key 'x'
        // does not, and its rows must stay out.
        let t = text_table(
            &["cust", "name"],
            &[&["NULL", "a"], &["", "b"], &["x", "c"], &["x", "c"]],
        );
        let mut c = DuckConn::new(t);
        let (got, _) = breaking_rows(
            &mut c,
            &source(),
            "cust",
            &["name".to_string()],
            usize::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            sorted_texts(&got),
            [["", "a"], ["", "b"]].map(|r| r.map(String::from).to_vec())
        );
    }

    #[test]
    fn sql_server_keys_ignore_trailing_spaces() {
        // `_BIN2` GROUP BY pads, so each dependent's query may return either
        // spelling of one key; the lookup must merge them.
        assert_eq!(group_key(DbEngine::Mssql, "b2 "), "b2");
        assert_eq!(group_key(DbEngine::Mssql, "b2"), "b2");
        assert_eq!(group_key(DbEngine::MySql, "b2 "), "b2 ");
        assert_eq!(group_key(DbEngine::Postgres, "b2 "), "b2 ");
    }

    #[test]
    fn the_split_matches_the_in_memory_split() {
        // No ties: the server breaks ties by value, which the docs state.
        let t = text_table(
            &["cust", "name", "amount"],
            &[
                &["1", "ada", "10"],
                &["1", "ada", "11"],
                &["1", "adx", "12"],
                &["2", "bob", "13"],
                &["2", "bob", "14"],
                &["3", "cy", "15"],
            ],
        );
        let want = crate::data::lookups::split_out(&t, 0, &[1]);
        let mut c = DuckConn::new(t);
        let (got, capped) = split_out(
            &mut c,
            &source(),
            "cust",
            &["name".to_string()],
            usize::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(!capped);
        assert_eq!(got.resolved_keys, want.resolved_keys);
        assert_eq!(sorted_texts(&got.lookup), sorted_texts(&want.lookup));
        assert_eq!(sorted_texts(&got.main), sorted_texts(&want.main));
        assert_eq!(
            got.main.columns.iter().map(|c| &c.name).collect::<Vec<_>>(),
            ["cust", "amount"]
        );
        assert_eq!(
            got.lookup
                .columns
                .iter()
                .map(|c| &c.name)
                .collect::<Vec<_>>(),
            ["cust", "name"]
        );
    }

    #[test]
    fn the_split_counts_null_and_empty_keys_as_one() {
        // As Task 6 and the in-memory engine do: one lookup row for both.
        let t = customers_flat();
        let want = crate::data::lookups::split_out(&t, 0, &[1, 2]);
        let mut c = DuckConn::new(t);
        let deps = ["name".to_string(), "city".to_string()];
        let (got, _) = split_out(
            &mut c,
            &source(),
            "cust",
            &deps,
            usize::MAX,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(got.lookup.row_count(), want.lookup.row_count());
        assert_eq!(got.resolved_keys, want.resolved_keys);
        // Cust 1 (names), cust 3 (cities) and the NULL/'' key (names). Grouped
        // raw, NULL and '' would be two clean keys and this would be 2 of 5.
        assert_eq!(got.resolved_keys, 3);
        assert_eq!(got.lookup.row_count(), 4);
        let (one, capped) =
            split_out(&mut c, &source(), "cust", &deps, 1, &AtomicBool::new(false)).unwrap();
        assert!(capped);
        assert_eq!(one.lookup.row_count(), 1);
        assert_eq!(one.main.row_count(), 1);
    }
}
