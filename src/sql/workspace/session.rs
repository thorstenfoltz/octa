//! Running statements on either of a workspace's two connections: its own,
//! on the window thread, or the runner a worker holds while a query runs or
//! `data` is copied ([`SqlWorkspace::run_job`]). Paged results live here too,
//! since both connections page through the same tables.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use anyhow::{Context, Result, anyhow, bail};
use duckdb::Connection;

use super::SqlWorkspace;
use crate::data::DataTable;
use crate::sql::engine::{
    QueryKind, QueryOutcome, append_rows, create_table_for, execute_query, h2o_easter_egg,
    is_create, is_mutation, octopuses_easter_egg, quote_ident, stars_easter_egg,
};

/// Table a paged SELECT is materialised into. The `__octa_` prefix keeps
/// it out of the way of user tables; it is replaced on every paged run and
/// never registered in `tables`, so it stays out of the workspace listing and
/// out of autocomplete.
/// Where a paged result is materialised, one table per result slot: the GUI
/// passes its editor pane's id, so side-by-side editors each keep their own
/// result to page through and export. A single shared table meant the second
/// editor's run replaced the first editor's rows behind its back.
fn result_table(slot: u64) -> String {
    format!("__octa_result_{slot}")
}

/// Ordering key added to that table. Without it a page is a LIMIT/OFFSET over
/// a parallel scan with no promised order, which can repeat or skip rows
/// between pages.
const RESULT_ORDER_COL: &str = "__octa_rn";

/// A statement run for paged display: the first page, plus the exact total
/// when the statement could be paged.
pub struct PagedResult {
    pub outcome: QueryOutcome,
    /// Rows in the whole result. `None` when the statement was not paged (a
    /// mutation, an easter egg, or several statements at once), in which case
    /// `outcome.table` already holds everything there is.
    pub total_rows: Option<usize>,
}

/// The inner query of a statement that can safely be wrapped in
/// `SELECT * FROM (...)`, or `None` when it cannot be.
///
/// Refusing is always safe: the caller falls back to the unpaged path. Only
/// single SELECT-shaped statements qualify - a mutation changes state, an
/// easter egg is not SQL at all, and several statements separated by `;`
/// cannot be a subquery.
fn pageable_select(trimmed: &str) -> Option<&str> {
    if trimmed.is_empty() || is_mutation(trimmed) {
        return None;
    }
    if octopuses_easter_egg(trimmed).is_some()
        || stars_easter_egg(trimmed).is_some()
        || h2o_easter_egg(trimmed).is_some()
    {
        return None;
    }
    let inner = trimmed.trim_end().trim_end_matches(';').trim_end();
    (!inner.is_empty() && !has_bare_semicolon(inner)).then_some(inner)
}

/// Is there a `;` outside a string or quoted identifier? Comments are not
/// tracked, so a `;` inside one reads as bare and the query falls back to the
/// unpaged path - the harmless direction to be wrong in.
fn has_bare_semicolon(s: &str) -> bool {
    let (mut in_single, mut in_double) = (false, false);
    for ch in s.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }
    false
}

/// Rows handed to [`SqlWorkspace::run_job`] for `data`, a slice at a time;
/// `None` is the end. A dropped sender stops the copy, `data` untouched.
pub struct DataFeed {
    pub columns: Vec<crate::data::ColumnInfo>,
    pub rows: Receiver<Option<DataTable>>,
}

/// A query for [`SqlWorkspace::run_job`], as [`SqlWorkspace::execute_paged`]
/// takes it.
pub struct JobQuery {
    pub sql: String,
    pub page_rows: usize,
    pub slot: u64,
}

/// What a [`SqlWorkspace::run_job`] did.
pub struct RunDone {
    /// Rows now in `data`, or why the copy failed. `None`: no copy asked.
    pub copied: Option<std::result::Result<usize, String>>,
    /// The query's outcome. `None`: no query asked, or the copy failed and
    /// the query was not run.
    pub query: Option<std::result::Result<PagedResult, String>>,
}

impl SqlWorkspace {
    /// Copy each of `feeds` into the workspace table it names, as a job for
    /// a worker on the runner connection: the other open tabs, registered so
    /// a query can join them. One failing (its tab changed, or closed) does
    /// not stop the rest. Each copied table is listed afterwards with
    /// [`Self::note_table`].
    pub fn copy_tables_job(
        &self,
        feeds: Vec<(String, DataFeed)>,
    ) -> impl FnOnce() -> Vec<(String, std::result::Result<usize, String>)> + Send + 'static {
        let runner = self.runner.clone();
        move || {
            feeds
                .into_iter()
                .map(|(name, feed)| {
                    let copied = match runner.lock() {
                        Ok(conn) => Session(&conn)
                            .copy_into(&name, feed)
                            .map_err(|e| format!("{e:#}")),
                        Err(_) => Err("the SQL worker stopped".to_string()),
                    };
                    (name, copied)
                })
                .collect()
        }
    }

    /// Copy `data` from `feed` and then run `query`, both optional, as a job
    /// for a worker on the runner connection, so neither holds up the window.
    /// The copy builds the new `data` beside the old one and swaps it in
    /// whole, so a query on the window's connection meanwhile sees the old
    /// rows, never half of the new. The handle interrupts the query. Once
    /// a copy landed, the caller calls [`Self::note_active_table`].
    pub fn run_job(
        &self,
        feed: Option<DataFeed>,
        query: Option<JobQuery>,
    ) -> Result<(
        impl FnOnce() -> RunDone + Send + 'static,
        Arc<duckdb::InterruptHandle>,
    )> {
        let runner = self.runner.clone();
        let interrupt = runner
            .lock()
            .map_err(|_| anyhow!("the SQL worker stopped"))?
            .interrupt_handle();
        Ok((
            move || {
                let Ok(conn) = runner.lock() else {
                    return RunDone {
                        copied: feed.map(|_| Err("the SQL worker stopped".to_string())),
                        query: None,
                    };
                };
                let session = Session(&conn);
                let copied = feed.map(|f| session.copy_data(f).map_err(|e| format!("{e:#}")));
                let query = match (&copied, query) {
                    (Some(Err(_)), _) | (_, None) => None,
                    (_, Some(q)) => Some(
                        session
                            .execute_paged(&q.sql, q.page_rows, q.slot)
                            .map_err(|e| format!("{e:#}")),
                    ),
                };
                RunDone { copied, query }
            },
            interrupt,
        ))
    }
}

/// The statements a workspace runs, on whichever of its two connections runs
/// them: its own, on the window thread, or the runner a worker holds
/// ([`SqlWorkspace::run_job`]). `data`, the registered tables and the paged
/// results live in the main catalog, which both connections share.
pub(super) struct Session<'a>(pub(super) &'a Connection);

impl Session<'_> {
    /// Build `data` from `feed` beside the old one, then swap it in. Returns
    /// the rows copied.
    pub(super) fn copy_data(&self, feed: DataFeed) -> Result<usize> {
        self.copy_into("data", feed)
    }

    /// Build table `target` from `feed` beside any old one, then swap it in,
    /// so a query meanwhile sees the old rows, never half of the new. A
    /// dropped feed leaves the old table as it was. Returns the rows copied.
    pub(super) fn copy_into(&self, target: &str, feed: DataFeed) -> Result<usize> {
        let next = format!("__octa_next_{target}");
        let next_q = quote_ident(&next);
        self.0
            .execute(&format!("DROP TABLE IF EXISTS {next_q}"), [])?;
        create_table_for(self.0, &next, &feed.columns)?;
        let mut rows = 0;
        let filled = (|| -> Result<()> {
            loop {
                match feed.rows.recv() {
                    Ok(Some(slice)) => {
                        rows += slice.row_count();
                        append_rows(self.0, &next, &slice)?;
                    }
                    Ok(None) => return Ok(()),
                    Err(_) => bail!("copying the table into SQL was stopped"),
                }
            }
        })();
        if let Err(e) = filled {
            let _ = self
                .0
                .execute(&format!("DROP TABLE IF EXISTS {next_q}"), []);
            return Err(e);
        }
        self.0.execute_batch("BEGIN TRANSACTION")?;
        let swapped = self.drop_relation(target).and_then(|()| {
            self.0
                .execute(
                    &format!("ALTER TABLE {next_q} RENAME TO {}", quote_ident(target)),
                    [],
                )
                .map(|_| ())
                .context("putting the copied table in place")
        });
        match swapped {
            Ok(()) => self.0.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = self.0.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        Ok(rows)
    }

    /// Run `query` for paged display: materialise the whole result inside
    /// DuckDB once, hand back the first `page_rows` rows and the **exact**
    /// total.
    ///
    /// The unpaged path pulls every row into a Rust `Vec` and stops at the
    /// file-open cap, so a bare `SELECT * FROM data` cost millions of rows of
    /// memory and reported "2,000,000 (capped)" rather than a real count.
    /// Here the rows stay in DuckDB and only a page crosses into Rust.
    ///
    /// `page_rows` of 0 means "no paging". Anything this cannot safely wrap in
    /// a subquery (a mutation, an easter egg, several statements at once)
    /// falls through to [`Self::execute`] with `total_rows: None`.
    pub fn execute_paged(&self, query: &str, page_rows: usize, slot: u64) -> Result<PagedResult> {
        let inner = match pageable_select(query.trim()) {
            Some(inner) if page_rows > 0 => inner.to_string(),
            _ => {
                return Ok(PagedResult {
                    outcome: self.execute(query)?,
                    total_rows: None,
                });
            }
        };
        let result_table = result_table(slot);
        self.0
            .execute(&format!("DROP TABLE IF EXISTS {result_table}"), [])?;
        // `row_number() OVER ()` over the already-ordered subquery, so a page
        // boundary cannot repeat or skip a row: a plain LIMIT/OFFSET over a
        // parallel table scan has no promised order to page through.
        self.0.execute(
            &format!(
                "CREATE TABLE {result_table} AS \
                 SELECT row_number() OVER () AS {RESULT_ORDER_COL}, * FROM ({inner})"
            ),
            [],
        )?;
        let total: i64 =
            self.0
                .query_row(&format!("SELECT count(*) FROM {result_table}"), [], |r| {
                    r.get(0)
                })?;
        let table = self.result_page(slot, 0, page_rows)?;
        Ok(PagedResult {
            outcome: QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table,
                created: None,
            },
            total_rows: Some(total.max(0) as usize),
        })
    }

    /// One page of the result [`Self::execute_paged`] last materialised.
    /// Errors when the last statement was not paged.
    pub fn result_page(&self, slot: u64, offset: usize, len: usize) -> Result<DataTable> {
        let result_table = result_table(slot);
        execute_query(
            self.0,
            &format!(
                "SELECT * EXCLUDE ({RESULT_ORDER_COL}) FROM {result_table} \
                 ORDER BY {RESULT_ORDER_COL} LIMIT {len} OFFSET {offset}"
            ),
        )
    }

    /// The whole materialised result of `slot`.
    pub fn result_all(&self, slot: u64) -> Result<DataTable> {
        let result_table = result_table(slot);
        execute_query(
            self.0,
            &format!(
                "SELECT * EXCLUDE ({RESULT_ORDER_COL}) FROM {result_table} \
                 ORDER BY {RESULT_ORDER_COL}"
            ),
        )
    }

    /// Free `slot`'s materialised result.
    pub fn drop_result(&self, slot: u64) {
        let _ = self
            .0
            .execute(&format!("DROP TABLE IF EXISTS {}", result_table(slot)), []);
    }

    /// Execute a statement against the workspace's persistent connection.
    /// Same shape as `run_query`. Mutations re-select `data` (preserves the
    /// existing single-table behaviour for the GUI's mutation flow).
    pub fn execute(&self, query: &str) -> Result<QueryOutcome> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            bail!("Query is empty");
        }
        if let Some(egg) = octopuses_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
                created: None,
            });
        }
        if let Some(egg) = stars_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
                created: None,
            });
        }
        if let Some(egg) = h2o_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
                created: None,
            });
        }
        if is_mutation(trimmed) {
            // A CREATE that adds one table or view to the workspace's own
            // catalog hands that table to the caller instead of re-selecting
            // `data`: the catalog diff, not a parser, says what appeared, so
            // CTAS, views, quoted and TEMP names all count. A CREATE inside an
            // attached database is not in this catalog and stays put there.
            let creating = is_create(trimmed);
            let before = if creating {
                self.own_relations()?
            } else {
                BTreeMap::new()
            };
            let affected = self.0.execute(trimmed, [])?;
            if creating {
                let after = self.own_relations()?;
                let mut fresh = after.iter().filter(|(k, _)| !before.contains_key(*k));
                if let (Some(((schema, name), kind)), None) = (fresh.next(), fresh.next()) {
                    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));
                    let table = self.read_typed(&qualified)?;
                    self.0.execute(&format!("DROP {kind} {qualified}"), [])?;
                    return Ok(QueryOutcome {
                        kind: QueryKind::Mutation,
                        affected: Some(affected),
                        table,
                        created: Some(name.clone()),
                    });
                }
            }
            // Re-select `data` so the GUI mutation path sees the post-state.
            // If `data` no longer exists (the user dropped it), return an
            // empty result rather than erroring.
            let mutated =
                execute_query(self.0, "SELECT * FROM data").unwrap_or_else(|_| DataTable {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    format_name: Some("SQL Result".to_string()),
                    ..DataTable::empty()
                });
            return Ok(QueryOutcome {
                kind: QueryKind::Mutation,
                affected: Some(affected),
                table: mutated,
                created: None,
            });
        }
        let result = execute_query(self.0, trimmed)?;
        Ok(QueryOutcome {
            kind: QueryKind::Select,
            affected: None,
            table: result,
            created: None,
        })
    }

    /// Tables and views in the workspace's own catalog (the in-memory database,
    /// where `data` and the registered tables live, plus `temp`, where a
    /// user's `CREATE TEMP` lands), keyed by
    /// (schema, name) with the DROP keyword (`TABLE` / `VIEW`) as the value.
    fn own_relations(&self) -> Result<BTreeMap<(String, String), &'static str>> {
        let mut stmt = self.0.prepare(
            "SELECT table_schema, table_name, table_type FROM information_schema.tables \
             WHERE table_catalog IN (current_database(), 'temp')",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (schema, name, ty) = row?;
            let kind = if ty.eq_ignore_ascii_case("VIEW") {
                "VIEW"
            } else {
                "TABLE"
            };
            out.insert((schema, name), kind);
        }
        Ok(out)
    }

    /// `SELECT *` of `qualified` with the declared column types kept, unlike
    /// `execute_query`, whose result columns are all `Utf8`. A created table
    /// becomes a tab, and its INTEGER column should stay a number on Save.
    fn read_typed(&self, qualified: &str) -> Result<DataTable> {
        let mut table = execute_query(self.0, &format!("SELECT * FROM {qualified}"))?;
        let mut stmt = self.0.prepare(&format!("DESCRIBE {qualified}"))?;
        let declared: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (name, ty) in declared {
            if let Some(col) = table.columns.iter_mut().find(|c| c.name == name) {
                col.data_type = crate::formats::duckdb_reader::duckdb_type_to_arrow(&ty, None);
            }
        }
        table.format_name = None;
        Ok(table)
    }

    /// Drop `name` from the workspace's own catalogs, whether it is a table or
    /// a view there: `data` is a table for an ordinary tab and a view over
    /// the file for a large-file one, and DuckDB refuses `DROP TABLE` on a view
    /// (and the other way round). Attached databases are never touched.
    pub(super) fn drop_relation(&self, name: &str) -> Result<()> {
        let mut stmt = self.0.prepare(
            "SELECT table_catalog, table_schema, table_type FROM information_schema.tables \
             WHERE table_name = ? AND table_catalog IN (current_database(), 'temp')",
        )?;
        let found: Vec<(String, String, String)> = stmt
            .query_map([name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (catalog, schema, kind) in found {
            let what = if kind == "VIEW" { "VIEW" } else { "TABLE" };
            let qualified = format!(
                "{}.{}.{}",
                quote_ident(&catalog),
                quote_ident(&schema),
                quote_ident(name)
            );
            self.0
                .execute(&format!("DROP {what} IF EXISTS {qualified}"), [])
                .with_context(|| format!("dropping {name}"))?;
        }
        Ok(())
    }
}
