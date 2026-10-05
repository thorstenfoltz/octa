//! Analyses run as SQL on the database server ("pushdown").
//!
//! A live-database tab holds the pages it has loaded. These modules answer
//! about the whole table instead, without downloading it: each takes a
//! connector and a [`ServerSource`] and returns the same result struct the
//! in-memory engine in `crate::data` returns, so the result tabs and dialogs
//! do not know which path produced it. One file per analysis, modelled on
//! `crate::formats`; per-engine spellings live in [`dialect`].

pub mod chart;
pub mod correlation;
pub mod dialect;
pub mod facets;
pub mod hash;
pub mod join_diag;
pub mod join_keys;
pub mod lookups;
pub mod pivot;
pub mod quality;
pub mod rel_measure;
pub mod row_estimate;
pub mod sample;
pub mod summary;
#[cfg(test)]
pub(crate) mod test_support;
pub mod timeseries;
pub mod value_frequency;
pub mod view;

use std::sync::atomic::{AtomicBool, Ordering};

use super::{DbConnection, DbConnector, DbEngine};
use crate::data::{CellValue, DataTable};

/// The whole table a database tab came from, or the filtered result the tab
/// shows when it is sorted and filtered on the server.
#[derive(Debug, Clone)]
pub struct ServerSource {
    pub conn: DbConnection,
    /// Catalog for three-level engines (Snowflake/Databricks/BigQuery).
    pub catalog: Option<String>,
    pub schema: String,
    pub table: String,
    /// The tab's server filter (`view::ServerView::where_sql`). Set, every
    /// analysis reads that result, which is what the tab holds.
    pub filter: Option<String>,
    /// The tab's hash columns (`view::ServerView::derived`): every analysis
    /// reads the table with them added.
    pub derived: Vec<hash::ServerHash>,
}

impl ServerSource {
    pub fn engine(&self) -> DbEngine {
        self.conn.engine
    }

    /// The table itself, quoted; never filtered. For paging and catalog
    /// lookups.
    pub fn table_sql(&self) -> String {
        super::qualified_name(
            self.engine(),
            self.catalog.as_deref(),
            &self.schema,
            &self.table,
        )
    }

    /// The table as it is: no filter, no hash columns.
    pub fn is_plain(&self) -> bool {
        self.filter.is_none() && self.derived.is_empty()
    }

    /// A `FROM` item under `alias`: the table, or the filtered result, with
    /// the hash columns added.
    pub fn from_as(&self, alias: &str) -> String {
        let e = self.engine();
        let table = || {
            let view = view::ServerView {
                derived: self.derived.clone(),
                ..Default::default()
            };
            view.from_item(e, &self.table_sql())
        };
        let inner = match &self.filter {
            None if self.derived.is_empty() => self.table_sql(),
            None => format!(
                "({})",
                hash::select_with(e, &self.table_sql(), &self.derived)
            ),
            Some(w) => format!("(SELECT * FROM {} WHERE {w})", table()),
        };
        format!("{inner}{}", dialect::subquery_alias(e, alias))
    }

    /// The `FROM` target: the quoted table, or the filtered or hashed result
    /// under an alias. A derived table needs one on Postgres, MySQL and SQL
    /// Server.
    pub fn from_sql(&self) -> String {
        if self.is_plain() {
            self.table_sql()
        } else {
            self.from_as("octa_view")
        }
    }
}

/// The parts of a result computed on the loaded rows instead of the server,
/// named by their column header in the result, for the result note. Three
/// lists because the reasons differ. Empty when everything ran on the server.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalParts {
    /// This engine's SQL cannot express them (MySQL has no percentile).
    pub engine: Vec<String>,
    /// They look at individual values, so they are local on every engine.
    pub by_design: Vec<String>,
    /// The server refused the query, so the loaded rows stand in.
    pub failed: Vec<String>,
}

impl LocalParts {
    pub fn is_empty(&self) -> bool {
        self.engine.is_empty() && self.by_design.is_empty() && self.failed.is_empty()
    }
}

/// Stop before the next round trip once the user pressed Cancel. Also what
/// keeps `with_conn`'s one retry from re-running a cancelled analysis.
pub fn check_cancel(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("{}", crate::i18n::t("db.load_cancelled"));
    }
    Ok(())
}

/// A server cell as a finite f64. Oracle and some REST engines hand numbers
/// back as text, so text that parses counts.
pub fn cell_f64(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Null => None,
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => f.is_finite().then_some(*f),
        other => other
            .to_string()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite()),
    }
}

/// A server cell as an integer (a count). Accepts `"12"` and `12.0`.
pub fn cell_i64(v: &CellValue) -> Option<i64> {
    match v {
        CellValue::Int(i) => Some(*i),
        CellValue::Null => None,
        other => {
            let s = other.to_string();
            let s = s.trim();
            s.parse::<i64>().ok().or_else(|| {
                s.parse::<f64>()
                    .ok()
                    .filter(|f| f.fract() == 0.0)
                    .map(|f| f as i64)
            })
        }
    }
}

/// `COUNT(*)` of the whole source.
pub fn count_rows(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    cancel: &AtomicBool,
) -> anyhow::Result<usize> {
    check_cancel(cancel)?;
    let t = c.query(&format!("SELECT COUNT(*) AS n FROM {}", src.from_sql()))?;
    t.get(0, 0)
        .and_then(cell_i64)
        .map(|n| n.max(0) as usize)
        .ok_or_else(|| anyhow::anyhow!("the row count came back empty"))
}

/// Engines cap the select list (Oracle 1000, Postgres 1664, Redshift 1600,
/// SQL Server and MySQL 4096), so one statement never carries more than
/// this many select items.
pub(crate) const MAX_SELECT_ITEMS: usize = 900;

/// Split items of these select widths into consecutive runs whose total fits
/// [`MAX_SELECT_ITEMS`]. An item wider than the cap gets a run of its own.
pub(crate) fn width_chunks(widths: &[usize]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < widths.len() {
        let (mut end, mut width) = (start, 0);
        while end < widths.len() && (end == start || width + widths[end] <= MAX_SELECT_ITEMS) {
            width += widths[end];
            end += 1;
        }
        out.push(start..end);
        start = end;
    }
    out
}

/// One `SELECT <every column's expressions> FROM from`, sliced back into
/// one cell row per column, split into statements under
/// [`MAX_SELECT_ITEMS`] so a wide table does not overflow the select list.
/// If the server refuses a statement, one query per column of it instead,
/// so a single column of a type it cannot `MIN` or `COUNT DISTINCT` (a
/// CLOB, a map) blanks itself (`None`) and not the whole result. Errors
/// only when every column failed, with the first error.
pub fn per_column_aggregates(
    c: &mut dyn DbConnector,
    from: &str,
    per_col: &[Vec<String>],
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<Option<Vec<CellValue>>>> {
    let row = |t: &DataTable, start: usize, n: usize| -> Vec<CellValue> {
        (start..start + n)
            .map(|i| t.get(0, i).cloned().unwrap_or(CellValue::Null))
            .collect()
    };
    let widths: Vec<usize> = per_col.iter().map(Vec::len).collect();
    let mut out = Vec::with_capacity(per_col.len());
    let mut first_err = None;
    for range in width_chunks(&widths) {
        let chunk = &per_col[range];
        let all: Vec<&str> = chunk.iter().flatten().map(String::as_str).collect();
        if all.is_empty() {
            out.extend(chunk.iter().map(|_| None));
            continue;
        }
        check_cancel(cancel)?;
        if let Ok(t) = c.query(&format!("SELECT {} FROM {from}", all.join(", "))) {
            let mut at = 0;
            for exprs in chunk {
                out.push((!exprs.is_empty()).then(|| row(&t, at, exprs.len())));
                at += exprs.len();
            }
            continue;
        }
        for exprs in chunk {
            check_cancel(cancel)?;
            if exprs.is_empty() {
                out.push(None);
                continue;
            }
            match c.query(&format!("SELECT {} FROM {from}", exprs.join(", "))) {
                Ok(t) => out.push(Some(row(&t, 0, exprs.len()))),
                Err(e) => {
                    first_err.get_or_insert(e);
                    out.push(None);
                }
            }
        }
    }
    match first_err {
        Some(e) if out.iter().all(Option::is_none) => Err(e),
        _ => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{DuckConn, source};
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "a".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "b".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::Int(1), CellValue::String("x".into())],
            vec![CellValue::Int(2), CellValue::Null],
            vec![CellValue::Null, CellValue::String("y".into())],
        ];
        t
    }

    #[test]
    fn a_filtered_source_reads_only_its_rows() {
        let mut src = source();
        assert_eq!(src.from_sql(), "\"data\"");
        src.filter = Some("\"k\" = 'a'".into());
        assert_eq!(
            src.from_sql(),
            "(SELECT * FROM \"data\" WHERE \"k\" = 'a') AS octa_view"
        );
        assert_eq!(src.table_sql(), "\"data\"");
        let mut c = DuckConn::new(super::join_keys::tests::text_table(
            &["k"],
            &[&["a"], &["a"], &["b"]],
        ));
        assert_eq!(
            count_rows(&mut c, &src, &AtomicBool::new(false)).unwrap(),
            2
        );
    }

    #[test]
    fn a_filtered_source_has_no_catalog_estimate() {
        let mut src = source();
        assert!(super::row_estimate::estimate_sql(&src).is_some());
        src.filter = Some("1 = 1".into());
        assert!(super::row_estimate::estimate_sql(&src).is_none());
    }

    #[test]
    fn counts_the_whole_source() {
        let mut c = DuckConn::new(table());
        assert_eq!(
            count_rows(&mut c, &source(), &AtomicBool::new(false)).unwrap(),
            3
        );
    }

    #[test]
    fn per_column_aggregates_slices_one_row_per_column() {
        let mut c = DuckConn::new(table());
        let per_col = vec![
            vec!["COUNT(\"a\")".to_string(), "MAX(\"a\")".to_string()],
            vec!["COUNT(\"b\")".to_string()],
        ];
        let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
        assert_eq!(cell_i64(&got[0].as_ref().unwrap()[0]), Some(2));
        assert_eq!(cell_i64(&got[0].as_ref().unwrap()[1]), Some(2));
        assert_eq!(cell_i64(&got[1].as_ref().unwrap()[0]), Some(2));
    }

    /// One column the server refuses must not blank the others.
    #[test]
    fn a_refused_column_only_blanks_itself() {
        let mut c = DuckConn::new(table());
        let per_col = vec![
            vec!["COUNT(\"a\")".to_string()],
            vec!["no_such_function(\"b\")".to_string()],
        ];
        let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
        assert_eq!(cell_i64(&got[0].as_ref().unwrap()[0]), Some(2));
        assert!(got[1].is_none());
    }

    /// Wider than one statement: the chunks must slice back to the same
    /// per-column cells a single statement would give.
    #[test]
    fn a_table_wider_than_one_statement_is_chunked() {
        let n = MAX_SELECT_ITEMS / 2 + 10;
        let mut t = DataTable::empty();
        t.columns = (0..n)
            .map(|i| ColumnInfo {
                name: format!("c{i}"),
                data_type: "Int64".into(),
            })
            .collect();
        t.rows = (0..3)
            .map(|r| {
                (0..n)
                    .map(|i| {
                        if (r + i) % 3 == 0 {
                            CellValue::Null
                        } else {
                            CellValue::Int((i * 10 + r) as i64)
                        }
                    })
                    .collect()
            })
            .collect();
        let per_col: Vec<Vec<String>> = (0..n)
            .map(|i| vec![format!("COUNT(\"c{i}\")"), format!("MAX(\"c{i}\")")])
            .collect();
        let mut c = DuckConn::new(t);
        let got = per_column_aggregates(&mut c, "data", &per_col, &AtomicBool::new(false)).unwrap();
        assert_eq!(c.log.len(), 2, "two statements, not one per column");
        assert!(
            c.log
                .iter()
                .all(|q| q.matches("COUNT(").count() <= MAX_SELECT_ITEMS)
        );
        for (i, cells) in got.iter().enumerate() {
            let cells = cells.as_ref().unwrap();
            let present: Vec<i64> = (0..3)
                .filter(|r| (r + i) % 3 != 0)
                .map(|r| (i * 10 + r) as i64)
                .collect();
            assert_eq!(cell_i64(&cells[0]), Some(present.len() as i64), "c{i}");
            assert_eq!(cell_i64(&cells[1]), present.iter().max().copied(), "c{i}");
        }
    }

    #[test]
    fn width_chunks_keep_each_run_under_the_cap() {
        assert!(width_chunks(&[]).is_empty());
        assert_eq!(width_chunks(&[1, 2]), vec![0..2]);
        assert_eq!(
            width_chunks(&[MAX_SELECT_ITEMS - 1, 2, MAX_SELECT_ITEMS + 5]),
            vec![0..1, 1..2, 2..3]
        );
    }

    #[test]
    fn a_cancel_stops_before_the_next_query() {
        let mut c = DuckConn::new(table());
        assert!(count_rows(&mut c, &source(), &AtomicBool::new(true)).is_err());
    }

    /// The docs page's engine table is generated from the dialect, so it
    /// cannot drift from the code. On failure, paste the printed table
    /// between the markers in docs/usage/analyses-on-live-databases.md.
    #[test]
    fn docs_support_table_matches_the_dialect() {
        let mut table = String::from(
            "| Engine | Median, quartiles, IQR, outliers | Correlation | Join diagnostics: spaces and punctuation | Join diagnostics: leading zeros | Fast random sample |\n|---|---|---|---|---|---|\n",
        );
        for &e in DbEngine::ALL {
            let quart = match (dialect::quartiles_sql(e, "t", "x"), e) {
                (None, _) => "loaded rows",
                (Some(_), DbEngine::Trino | DbEngine::Athena) => "server (approximate)",
                (Some(_), _) => "server",
            };
            let corr = if dialect::pearson_select(e, "a", "b").1 == 1 {
                "server (CORR)"
            } else {
                "server (sums)"
            };
            let on = |b: bool| if b { "server" } else { "loaded rows" };
            let regex = on(dialect::collapse_ws(e, "x").is_some());
            let zeros = on(dialect::strip_leading_zeros(e, "x").is_some());
            let fast = if sample::fast_available(e) {
                "block sampling"
            } else {
                "exact only"
            };
            table.push_str(&format!(
                "| {} | {quart} | {corr} | {regex} | {zeros} | {fast} |\n",
                e.label()
            ));
        }
        let doc = include_str!("../../../docs/usage/analyses-on-live-databases.md");
        let start = "<!-- support-table:start -->\n";
        let inner = doc
            .split(start)
            .nth(1)
            .and_then(|s| s.split("<!-- support-table:end -->").next());
        // MegaLinter's table formatter pads the columns, so compare the cells
        // and not the spacing (a separator cell reduces to "").
        let cells = |t: &str| -> Vec<Vec<String>> {
            t.lines()
                .map(|l| {
                    l.split('|')
                        .map(|c| c.trim().trim_matches('-').to_string())
                        .collect()
                })
                .collect()
        };
        assert_eq!(inner.map(cells), Some(cells(&table)), "\n{table}");
    }
}
