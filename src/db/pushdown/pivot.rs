//! Pivot on the server: one column per distinct value of `on`, each the
//! aggregate of `value` over the rows holding it, in the shape of DuckDB's
//! `PIVOT` (the file path, `crate::data::pivot::pivot_sql`): NULL values of
//! `on` are left out, columns are ordered by the value as text, Count is 0
//! for an empty cell and the others NULL.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use crate::data::pivot::PivotAgg;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{as_float, exact, str_lit, text};
use super::lookups::{cap_clause, cut};
use super::{MAX_SELECT_ITEMS, ServerSource, check_cancel};

/// Value columns per statement: the select list carries the grouping
/// columns too and stays under [`MAX_SELECT_ITEMS`].
pub fn chunk_size(groups: usize) -> usize {
    MAX_SELECT_ITEMS.saturating_sub(groups).max(1)
}

/// The distinct values of `on` as the database's text, sorted as DuckDB
/// orders its pivot columns: byte by byte (`10` before `9`, `B` before `a`).
/// Grouped by the exact text: MySQL's and SQL Server's usual collations
/// would merge `B` and `b`.
fn labels(c: &mut dyn DbConnector, src: &ServerSource, on: &str) -> anyhow::Result<Vec<String>> {
    let e = src.engine();
    let col = e.quote_ident(on);
    let as_text = text(e, &col);
    let t = c.query(&format!(
        "SELECT MIN({as_text}) FROM {} WHERE {col} IS NOT NULL GROUP BY {}",
        src.from_sql(),
        exact(e, &as_text)
    ))?;
    let mut out: Vec<String> = (0..t.row_count())
        .filter_map(|r| t.get(r, 0))
        .filter(|v| !matches!(v, CellValue::Null))
        .map(|v| v.to_string())
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// `agg(value)` over the rows whose `on` reads `label`.
fn cell_expr(e: DbEngine, agg: PivotAgg, on_text: &str, label: &str, value: &str) -> String {
    let when = format!("{} = {}", exact(e, on_text), exact(e, &str_lit(e, label)));
    // SQL Server's AVG(int) truncates and its SUM(int) overflows.
    let v = match agg {
        PivotAgg::Sum | PivotAgg::Avg => as_float(e, value),
        PivotAgg::Count | PivotAgg::Min | PivotAgg::Max => value.to_string(),
    };
    format!(
        "{}(CASE WHEN {when} THEN {v} END)",
        agg.sql_fn().to_uppercase()
    )
}

/// DuckDB's names for the pivot columns: the value itself; a name already
/// taken, ignoring case as DuckDB's identifiers do, gets `_1`, `_2`, ...
/// An empty value is named "(empty)" rather than DuckDB's generated
/// expression.
fn column_names(groups: &[String], labels: &[String]) -> Vec<String> {
    let mut used: HashSet<String> = groups.iter().map(|g| g.to_lowercase()).collect();
    labels
        .iter()
        .map(|l| {
            let base = if l.is_empty() {
                crate::i18n::t("dialog.cf_empty")
            } else {
                l.clone()
            };
            let (mut name, mut k) = (base.clone(), 1);
            while used.contains(&name.to_lowercase()) {
                name = format!("{base}_{k}");
                k += 1;
            }
            used.insert(name.to_lowercase());
            name
        })
        .collect()
}

/// What to pivot, by column name.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotSpec {
    /// Every column of the table, for the inferred grouping.
    pub columns: Vec<String>,
    pub on: String,
    pub agg: PivotAgg,
    pub value: String,
    /// Empty: every column but `on` and `value`, as DuckDB infers.
    pub group: Vec<String>,
}

/// The pivot of `src`: `spec.on`'s values as columns, `agg(value)` in each,
/// one row per combination of the grouping columns. At most `cap` rows; the
/// flag says the database has more. `chunk` value columns per statement
/// (callers pass [`chunk_size`]); every statement groups and orders the same
/// way, and the pieces are joined side by side, checked group by group.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    spec: &PivotSpec,
    chunk: usize,
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, bool)> {
    let e = src.engine();
    check_cancel(cancel)?;
    let group: Vec<String> = if spec.group.is_empty() {
        spec.columns
            .iter()
            .filter(|c| **c != spec.on && **c != spec.value)
            .cloned()
            .collect()
    } else {
        spec.group.clone()
    };
    let labels = labels(c, src, &spec.on)?;
    if labels.is_empty() {
        anyhow::bail!("\"{}\" has no values to make columns from", spec.on);
    }
    let on_text = text(e, &e.quote_ident(&spec.on));
    let v = e.quote_ident(&spec.value);
    let g: Vec<String> = group.iter().map(|x| e.quote_ident(x)).collect();
    let tail = if g.is_empty() {
        String::new()
    } else {
        format!(
            " GROUP BY {} ORDER BY {}{}",
            g.join(", "),
            g.join(", "),
            cap_clause(e, cap)
        )
    };
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut group_types: Vec<String> = Vec::new();
    let mut value_types: Vec<String> = Vec::new();
    // The inferred grouping takes select items too: size from what is used.
    let chunk = chunk.min(chunk_size(g.len())).max(1);
    for (i, piece) in labels.chunks(chunk).enumerate() {
        check_cancel(cancel)?;
        let mut select = g.clone();
        select.extend(
            piece
                .iter()
                .map(|l| cell_expr(e, spec.agg, &on_text, l, &v)),
        );
        let t = c.query(&format!(
            "SELECT {} FROM {}{tail}",
            select.join(", "),
            src.from_sql()
        ))?;
        let k = g.len();
        if i == 0 {
            group_types = t.columns[..k].iter().map(|c| c.data_type.clone()).collect();
            rows = t.rows.iter().map(|r| r[..k].to_vec()).collect();
        } else if t.row_count() != rows.len()
            || t.rows.iter().zip(&rows).any(|(a, b)| a[..k] != b[..k])
        {
            anyhow::bail!(
                "the database returned the groups in a different order for one part of the pivot"
            );
        }
        value_types.extend(t.columns[k..].iter().map(|c| c.data_type.clone()));
        for (row, r) in rows.iter_mut().zip(&t.rows) {
            row.extend(r[k..].iter().cloned());
        }
    }
    let mut out = DataTable::empty();
    out.columns = group
        .iter()
        .zip(&group_types)
        .chain(column_names(&group, &labels).iter().zip(&value_types))
        .map(|(name, ty)| ColumnInfo {
            name: name.clone(),
            data_type: ty.clone(),
        })
        .collect();
    out.rows = rows;
    Ok(cut(out, cap))
}

#[cfg(test)]
mod tests {
    use super::super::join_keys::tests::text_table;
    use super::super::test_support::{DuckConn, source};
    use super::*;
    use crate::data::pivot::pivot_sql;

    /// DuckDB's own answers for this table (checked while planning):
    /// columns `g, 10, 9, B, a, b_1`; Count 0 / Sum NULL where empty; the
    /// NULL `k` row (x, NULL, 4) is in no column; (y, a, NULL) counts 0.
    fn table() -> DataTable {
        let mut t = text_table(
            &["g", "k", "v"],
            &[
                &["x", "b", "1"],
                &["x", "a", "2"],
                &["y", "B", "3"],
                &["x", "NULL", "4"],
                &["y", "a", "NULL"],
                &["z", "10", "5"],
                &["z", "9", "6"],
            ],
        );
        t.columns[2].data_type = "Int64".into();
        for r in &mut t.rows {
            if let CellValue::String(s) = &r[2] {
                r[2] = CellValue::Int(s.parse().unwrap());
            }
        }
        t
    }

    fn spec(agg: PivotAgg) -> PivotSpec {
        PivotSpec {
            columns: vec!["g".into(), "k".into(), "v".into()],
            on: "k".into(),
            agg,
            value: "v".into(),
            group: vec!["g".into()],
        }
    }

    /// Cells as numbers where they are numbers, so DuckDB's int128 and the
    /// server's double compare equal; NULL stays NULL.
    fn norm(t: &DataTable) -> Vec<Vec<String>> {
        let mut rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| match super::super::cell_f64(c) {
                        Some(x) => format!("{x}"),
                        None => c.to_string(),
                    })
                    .collect()
            })
            .collect();
        rows.sort();
        rows
    }

    #[test]
    fn the_server_pivot_is_duckdbs_pivot() {
        for agg in [PivotAgg::Count, PivotAgg::Sum, PivotAgg::Max] {
            let t = table();
            let want = crate::sql::run_query(&t, &pivot_sql("k", agg, "v", &["g".into()]))
                .unwrap()
                .table;
            for chunk in [chunk_size(1), 2] {
                let mut c = DuckConn::new(table());
                let (got, more) = run(
                    &mut c,
                    &source(),
                    &spec(agg),
                    chunk,
                    1_000,
                    &AtomicBool::new(false),
                )
                .unwrap();
                assert!(!more);
                let names =
                    |t: &DataTable| t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
                assert_eq!(names(&got), names(&want), "{agg:?} chunk {chunk}");
                assert_eq!(norm(&got), norm(&want), "{agg:?} chunk {chunk}");
            }
        }
    }

    #[test]
    fn no_group_chosen_groups_by_every_other_column() {
        let mut s = spec(PivotAgg::Sum);
        s.group.clear();
        let mut c = DuckConn::new(table());
        let (got, _) = run(&mut c, &source(), &s, 100, 1_000, &AtomicBool::new(false)).unwrap();
        assert_eq!(got.columns[0].name, "g");
        assert_eq!(got.row_count(), 3);
    }

    #[test]
    fn more_groups_than_the_cap_says_so() {
        let mut c = DuckConn::new(table());
        let (got, more) = run(
            &mut c,
            &source(),
            &spec(PivotAgg::Count),
            100,
            2,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(more);
        assert_eq!(got.row_count(), 2);
    }

    #[test]
    fn values_are_compared_exactly_on_mysql() {
        let sql = cell_expr(
            DbEngine::MySql,
            PivotAgg::Sum,
            "CAST(`k` AS CHAR)",
            "b",
            "`v`",
        );
        assert_eq!(
            sql,
            "SUM(CASE WHEN CAST(CAST(`k` AS CHAR) AS BINARY) = CAST('b' AS BINARY) THEN CAST(`v` AS DOUBLE) END)"
        );
    }

    #[test]
    fn duckdb_names_case_twins_apart() {
        let labels: Vec<String> = ["10", "9", "B", "a", "b", ""]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let names = column_names(&["g".into()], &labels);
        assert_eq!(&names[..5], &["10", "9", "B", "a", "b_1"]);
        assert_eq!(names[5], crate::i18n::t("dialog.cf_empty"));
    }
}
