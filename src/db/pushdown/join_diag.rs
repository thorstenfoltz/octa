//! Join diagnostics on the server: why two key columns do not match, over
//! every row of both tables.
//!
//! Counts are over distinct untrimmed values, blanks skipped, as in
//! `crate::data::join_diag`. Each normalisation is the same distinct match
//! recomputed with it applied, all in one statement. A normalisation the
//! engine cannot spell is returned as "not checked" for the caller to
//! compute on the loaded rows.
//!
//! SQL Server's `=` ignores trailing spaces under every collation: there
//! `'b2 '` already matches `'b2'`, so the baseline is higher than in memory
//! and "Trim spaces" may not be offered. MySQL keys go through
//! `dialect::exact`'s binary cast, which compares every byte.

use std::sync::atomic::AtomicBool;

use super::join_keys::{KeySide, expr_stats, shared_counts};
use super::{ServerSource, check_cancel, count_rows, dialect};
use crate::data::join_diag::{ALL_FIXES, FixKind, JoinDiagnosis, MAX_SAMPLES, fixes_from_counts};
use crate::db::{DbConnector, DbEngine};

/// `t` with `kind` applied, for non-blank values only (blank stays NULL, so
/// it is skipped as in memory). A normalisation that turns a value into ''
/// keeps it as '', which still matches '' on the other side, as in memory
/// (Oracle and Exasol read that '' as NULL: documented). `None` = this engine cannot
/// spell it.
pub(crate) fn fix_expr(engine: DbEngine, kind: FixKind, t: &str) -> Option<String> {
    let tr = dialect::trim(engine, t);
    let non_blank =
        |e: String| format!("CASE WHEN NULLIF({tr}, '') IS NULL THEN NULL ELSE {e} END");
    match kind {
        FixKind::TrimWhitespace => Some(format!("NULLIF({tr}, '')")),
        FixKind::IgnoreCase => Some(dialect::blank_null(engine, &dialect::lower(engine, t))),
        FixKind::CollapseWhitespace => dialect::collapse_ws(engine, t).map(non_blank),
        FixKind::StripPunctuation => dialect::strip_punct(engine, t).map(non_blank),
        // "000" strips to '' (NULL on Oracle); in memory it becomes "0", so it
        // matches "0" and is never taken for a blank key.
        FixKind::StripLeadingZeros => dialect::strip_leading_zeros(engine, &tr)
            .map(|s| non_blank(format!("COALESCE(NULLIF({s}, ''), '0')"))),
    }
}

/// The fixes `engine` cannot spell, which `run` reports as not checked.
/// Known before any query, so a caller can prepare for them up front.
pub fn unchecked_fixes(engine: DbEngine) -> Vec<FixKind> {
    // Whether a fix is spellable depends on the engine only, never the column.
    ALL_FIXES
        .into_iter()
        .filter(|&k| fix_expr(engine, k, "x").is_none())
        .collect()
}

/// Up to `MAX_SAMPLES` distinct values of `a` with no partner in `b`, sorted.
/// `exact` keeps MySQL's and SQL Server's case-blind collations from merging
/// or matching values that differ in case. `LEFT JOIN`, not `NOT EXISTS`:
/// ClickHouse has no correlated subqueries.
fn only_in(engine: DbEngine, a: KeySide, b: KeySide) -> String {
    let side = |s: &KeySide, x: &str| {
        format!(
            "(SELECT DISTINCT kv FROM (SELECT {} AS kv FROM {}){} WHERE kv IS NOT NULL)",
            dialect::exact(engine, &s.exprs[0]),
            s.from,
            dialect::subquery_alias(engine, x)
        )
    };
    // Byte order, as in memory. Postgres sorts by the database locale
    // otherwise; Redshift has no `COLLATE` clause and sorts case-sensitively
    // by default, so it needs none.
    let collate = if engine == DbEngine::Postgres {
        " COLLATE \"C\""
    } else {
        ""
    };
    format!(
        "SELECT a.kv FROM {}{} LEFT JOIN {}{} ON a.kv = b.kv WHERE b.kv IS NULL \
         ORDER BY a.kv{collate}{}",
        side(&a, "xa"),
        dialect::subquery_alias(engine, "a"),
        side(&b, "xb"),
        dialect::subquery_alias(engine, "b"),
        dialect::limit_clause(engine, MAX_SAMPLES)
    )
}

/// Diagnose `left.left_col` against `right.right_col` on the server. The
/// second value lists the fixes this engine could not check.
pub fn run(
    c: &mut dyn DbConnector,
    left: &ServerSource,
    left_col: &str,
    right: &ServerSource,
    right_col: &str,
    cancel: &AtomicBool,
) -> anyhow::Result<(JoinDiagnosis, Vec<FixKind>)> {
    let engine = left.engine();
    let (lf, rf) = (left.from_sql(), right.from_sql());
    let lt = dialect::text(engine, &engine.quote_ident(left_col));
    let rt = dialect::text(engine, &engine.quote_ident(right_col));
    let left_rows = count_rows(c, left, cancel)?;
    let right_rows = count_rows(c, right, cancel)?;

    let not_checked = unchecked_fixes(engine);
    let mut kinds = Vec::new();
    let mut lx = vec![dialect::blank_null(engine, &lt)];
    let mut rx = vec![dialect::blank_null(engine, &rt)];
    for kind in ALL_FIXES {
        if let (Some(l), Some(r)) = (fix_expr(engine, kind, &lt), fix_expr(engine, kind, &rt)) {
            kinds.push(kind);
            lx.push(l);
            rx.push(r);
        }
    }
    let (dl, _) = expr_stats(c, &lf, &lx[..1], cancel)?[0];
    let (dr, _) = expr_stats(c, &rf, &rx[..1], cancel)?[0];
    let shared = shared_counts(
        c,
        engine,
        KeySide {
            from: &lf,
            exprs: &lx,
        },
        KeySide {
            from: &rf,
            exprs: &rx,
        },
        true,
        cancel,
    )?;
    let at = |i: usize| shared.get(&(i, i)).copied().unwrap_or(0);
    let matched = at(0);
    let fixes = fixes_from_counts(
        matched,
        kinds.iter().enumerate().map(|(i, &k)| (k, at(i + 1))),
    );

    let lb = KeySide {
        from: &lf,
        exprs: &lx[..1],
    };
    let rb = KeySide {
        from: &rf,
        exprs: &rx[..1],
    };
    let mut sample = |a: KeySide, b: KeySide| -> anyhow::Result<Vec<String>> {
        check_cancel(cancel)?;
        let t = c.query(&only_in(engine, a, b))?;
        Ok((0..t.row_count())
            .filter_map(|r| t.get(r, 0).map(|v| v.to_string()))
            .collect())
    };
    let unmatched_left = sample(lb, rb)?;
    let unmatched_right = sample(rb, lb)?;

    Ok((
        JoinDiagnosis {
            left_rows,
            right_rows,
            distinct_left: dl,
            distinct_right: dr,
            matched_left: matched,
            matched_right: matched,
            unmatched_left,
            unmatched_right,
            fixes,
            capped: false,
        },
        not_checked,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::join_keys::tests::text_table;
    use super::super::test_support::{DuckConn, source_named};
    use super::*;
    use crate::data::join_diag::diagnose;

    fn both(
        left: crate::data::DataTable,
        right: crate::data::DataTable,
    ) -> (JoinDiagnosis, JoinDiagnosis) {
        let want = diagnose(&left, 0, &right, 0, usize::MAX);
        let mut c = DuckConn::with_tables(left, vec![("other", right)]);
        let (got, not_checked) = run(
            &mut c,
            &source_named("data"),
            "k",
            &source_named("other"),
            "id",
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(not_checked.is_empty());
        (got, want)
    }

    /// Every fix, regex ones included, against the in-memory engine.
    #[test]
    fn the_server_diagnoses_like_the_in_memory_engine() {
        let left = text_table(
            &["k"],
            &[
                &["007"],
                &["A1"],
                &["b2 "],
                &["c3"],
                &["c3"],
                &["x9"],
                &[""],
                &["NULL"],
                &["a  b"],
                &["x-1"],
                &["a  b  c"],
                &["x-1-2"],
            ],
        );
        let right = text_table(
            &["id"],
            &[
                &["7"],
                &["a1"],
                &["b2"],
                &["c3"],
                &["zz"],
                &["a b"],
                &["x 1"],
                &["a b c"],
                &["x 1 2"],
            ],
        );
        let (got, want) = both(left, right);
        assert_eq!(got, want);
        for kind in [
            FixKind::CollapseWhitespace,
            FixKind::StripPunctuation,
            FixKind::StripLeadingZeros,
        ] {
            assert!(got.fixes.iter().any(|f| f.kind == kind), "{kind:?}");
        }
    }

    /// `LTRIM('000', '0')` is `''` on most engines (NULL on Oracle); in memory
    /// an all-zero key becomes "0", so it matches "0" and never a blank.
    #[test]
    fn an_all_zero_key_strips_to_zero_not_blank() {
        let left = text_table(&["k"], &[&["000"], &["5"]]);
        let right = text_table(&["id"], &[&["0"], &["05"], &[" "]]);
        let (got, want) = both(left, right);
        assert_eq!(got, want);
        let zeros = got
            .fixes
            .iter()
            .find(|f| f.kind == FixKind::StripLeadingZeros);
        assert_eq!(zeros.map(|f| f.would_match), Some(2));
    }

    #[test]
    fn unchecked_fixes_name_what_the_engine_cannot_spell() {
        assert_eq!(
            unchecked_fixes(DbEngine::Mssql),
            [
                FixKind::CollapseWhitespace,
                FixKind::StripPunctuation,
                FixKind::StripLeadingZeros,
            ]
        );
        assert!(unchecked_fixes(DbEngine::Postgres).is_empty());
    }

    #[test]
    fn sql_server_leaves_the_regex_and_zero_fixes_to_the_loaded_rows() {
        let missing: Vec<FixKind> = ALL_FIXES
            .into_iter()
            .filter(|&k| fix_expr(DbEngine::Mssql, k, "x").is_none())
            .collect();
        assert_eq!(
            missing,
            [
                FixKind::CollapseWhitespace,
                FixKind::StripPunctuation,
                FixKind::StripLeadingZeros
            ]
        );
        assert!(
            ALL_FIXES
                .into_iter()
                .all(|k| fix_expr(DbEngine::Postgres, k, "x").is_some())
        );
    }
}
