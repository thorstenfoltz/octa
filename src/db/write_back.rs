//! Live-DB write-back: diff a loaded table against its `DbRowMeta` baseline
//! into a plan (deletes / updates / inserts / added columns), then apply the
//! plan in one transaction. Same diff rules as the SQLite file writer
//! (`src/formats/sqlite_reader.rs`), but addressed by primary-key values
//! instead of rowids, since a server table has no client-visible rowid.
//!
//! Accepted ceilings (by construction, documented in the manual):
//! - Rows beyond the `initial_load_rows` window were never loaded, so they
//!   are never touched: deletes/updates only address loaded tags, inserts
//!   append.
//! - Concurrent server edits between load and save lose to the full-row
//!   UPDATE (last-writer-wins; optimistic locking is out of scope).

use anyhow::{Context, bail};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbConnector, DbEngine, sql_literal};

/// How a changed row is addressed on the server.
///
/// A table with a server-enforced unique key uses it. A table with none can
/// still be edited by matching **every original column value**, which is only
/// acceptable because each such statement's affected-row count is checked to
/// be exactly 1: if the values match two rows, or none, the whole transaction
/// rolls back rather than guessing. That check is the entire safety argument
/// for [`RowIdentity::FullRow`], so it is not optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowIdentity {
    /// A primary key, or a UNIQUE constraint over NOT NULL columns. Addresses
    /// exactly one row by the server's own guarantee, so no count check is
    /// needed - and none is made, because MySQL reports *changed* rather than
    /// *matched* rows and would make a correct update look wrong.
    Key(Vec<String>),
    /// Every column of the baseline row. No server guarantee, so every
    /// statement built from it is verified to have touched exactly one row.
    FullRow(Vec<String>),
}

impl RowIdentity {
    /// The columns forming the WHERE clause.
    pub fn columns(&self) -> &[String] {
        match self {
            Self::Key(c) | Self::FullRow(c) => c,
        }
    }

    /// Whether each statement must be verified to have touched exactly one row.
    pub fn needs_affected_row_check(&self) -> bool {
        matches!(self, Self::FullRow(_))
    }

    /// Whether a NULL among the addressing values is acceptable. A key column
    /// is NOT NULL by construction, so a NULL there means the baseline is
    /// wrong; a full row may legitimately hold NULLs, matched with `IS NULL`.
    fn allows_null(&self) -> bool {
        matches!(self, Self::FullRow(_))
    }
}

/// What a save would do to the server table, computed from the table's rows
/// vs its `DbRowMeta` baseline.
#[derive(Debug, Default, PartialEq)]
pub struct DbWriteBackPlan {
    /// PK values per row to delete (from `original`, so an edited PK cell
    /// still locates its row).
    pub deletes: Vec<Vec<CellValue>>,
    /// (original PK values, full current row) per changed row.
    pub updates: Vec<(Vec<CellValue>, Vec<CellValue>)>,
    /// Full rows for `None`-tagged (user-inserted) rows.
    pub inserts: Vec<Vec<CellValue>>,
    /// Columns present now but not in `original_columns` (`ALTER TABLE ADD`).
    pub added_columns: Vec<ColumnInfo>,
}

impl DbWriteBackPlan {
    pub fn is_empty(&self) -> bool {
        self.deletes.is_empty()
            && self.updates.is_empty()
            && self.inserts.is_empty()
            && self.added_columns.is_empty()
    }

    /// Total number of row-level changes (for the status line).
    pub fn change_count(&self) -> usize {
        self.deletes.len() + self.updates.len() + self.inserts.len()
    }
}

/// Outcome counts of an applied write-back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbWriteBackReport {
    pub deleted: usize,
    pub updated: usize,
    pub inserted: usize,
    pub added_columns: usize,
}

/// Build the plan from a table whose edits are applied (`apply_edits()` done
/// by the caller). Errors when `db_meta` is missing (row identity lost), a
/// baseline column was removed or renamed, a PK column is not among the
/// current columns, or a PK value is NULL (the row cannot be addressed).
pub fn build_write_back_plan(
    table: &DataTable,
    identity: &RowIdentity,
) -> anyhow::Result<DbWriteBackPlan> {
    let pk_cols = identity.columns();
    let Some(meta) = table.db_meta.as_ref() else {
        bail!(
            "row identity lost: the table was rewritten locally (e.g. by a SQL mutation); \
             reload the table or use Run on server"
        );
    };
    if pk_cols.is_empty() {
        bail!("the table has no key and no baseline columns, so edits cannot be addressed");
    }

    // Baseline columns must all still exist under their original names, in
    // any position; column drops/renames are rejected for now.
    let current_names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    for orig in &meta.original_columns {
        if !current_names.contains(&orig.as_str()) {
            bail!("column '{orig}' was removed or renamed; write-back supports only added columns");
        }
    }
    let added_columns: Vec<ColumnInfo> = table
        .columns
        .iter()
        .filter(|c| !meta.original_columns.contains(&c.name))
        .cloned()
        .collect();

    // PK column indices in the ORIGINAL column order (the baseline snapshot
    // rows are in load order, which is the original order).
    let pk_orig_idx: Vec<usize> = pk_cols
        .iter()
        .map(|pk| {
            meta.original_columns
                .iter()
                .position(|c| c == pk)
                .with_context(|| format!("primary-key column '{pk}' not found in the table"))
        })
        .collect::<anyhow::Result<_>>()?;

    let pk_values = |original_row: &[CellValue]| -> anyhow::Result<Vec<CellValue>> {
        pk_orig_idx
            .iter()
            .map(|&i| {
                let v = original_row.get(i).cloned().unwrap_or(CellValue::Null);
                // A key column is NOT NULL by construction, so a NULL here
                // means the baseline disagrees with the schema. A full-row
                // match may hold NULLs; `pk_where_sql` turns them into
                // `IS NULL`.
                if matches!(v, CellValue::Null) && !identity.allows_null() {
                    bail!("a key value is NULL; the row cannot be addressed");
                }
                Ok(v)
            })
            .collect()
    };

    let mut plan = DbWriteBackPlan {
        added_columns,
        ..Default::default()
    };

    // DELETE rows whose tag is no longer present.
    let live_tags: std::collections::HashSet<i64> =
        meta.row_tags.iter().filter_map(|t| *t).collect();
    let mut delete_tags: Vec<&i64> = meta
        .original
        .keys()
        .filter(|t| !live_tags.contains(t))
        .collect();
    delete_tags.sort();
    for tag in delete_tags {
        plan.deletes.push(pk_values(&meta.original[tag])?);
    }

    // INSERT / UPDATE per current row. Added columns force full-row updates
    // even for rows whose baseline cells match (same gate as the SQLite
    // writer), so the new columns get their values.
    let force_update = !plan.added_columns.is_empty();
    for (row_idx, tag) in meta.row_tags.iter().enumerate() {
        let row_vals: Vec<CellValue> = (0..table.columns.len())
            .map(|c| table.get(row_idx, c).cloned().unwrap_or(CellValue::Null))
            .collect();
        match tag {
            None => plan.inserts.push(row_vals),
            Some(tag) => {
                let Some(original) = meta.original.get(tag) else {
                    continue;
                };
                if !force_update && original == &row_vals {
                    continue;
                }
                plan.updates.push((pk_values(original)?, row_vals));
            }
        }
    }
    Ok(plan)
}

/// The fully-quoted `schema.table` target.
fn target_sql(engine: DbEngine, schema: &str, table: &str) -> String {
    if schema.is_empty() {
        engine.quote_ident(table)
    } else {
        format!(
            "{}.{}",
            engine.quote_ident(schema),
            engine.quote_ident(table)
        )
    }
}

/// `col = literal AND col2 IS NULL` for a WHERE clause.
///
/// A NULL is matched with `IS NULL`, never `= NULL`, which is never true in
/// SQL. Key columns cannot be NULL, so this only ever fires for a full-row
/// match - where getting it wrong would silently address no rows at all.
fn pk_where_sql(engine: DbEngine, pk_cols: &[String], pk_vals: &[CellValue]) -> String {
    pk_cols
        .iter()
        .zip(pk_vals)
        .map(|(col, val)| {
            let ident = engine.quote_ident(col);
            match val {
                CellValue::Null => format!("{ident} IS NULL"),
                _ => format!("{ident} = {}", sql_literal(engine, val)),
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn delete_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    pk_cols: &[String],
    pk_vals: &[CellValue],
) -> String {
    format!(
        "DELETE FROM {} WHERE {}",
        target_sql(engine, schema, table),
        pk_where_sql(engine, pk_cols, pk_vals)
    )
}

fn update_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    pk_cols: &[String],
    pk_vals: &[CellValue],
    row: &[CellValue],
) -> String {
    let assignments: Vec<String> = columns
        .iter()
        .zip(row)
        .map(|(c, v)| {
            format!(
                "{} = {}",
                engine.quote_ident(&c.name),
                sql_literal(engine, v)
            )
        })
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {}",
        target_sql(engine, schema, table),
        assignments.join(", "),
        pk_where_sql(engine, pk_cols, pk_vals)
    )
}

fn insert_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    row: &[CellValue],
) -> String {
    let col_list = columns
        .iter()
        .map(|c| engine.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let values = row
        .iter()
        .map(|v| sql_literal(engine, v))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({col_list}) VALUES ({values})",
        target_sql(engine, schema, table)
    )
}

fn alter_add_sql(engine: DbEngine, schema: &str, table: &str, col: &ColumnInfo) -> String {
    use crate::data::schema_export::sql::column_type_sql;
    let dialect = super::live_dialect_for(engine);
    // SQL Server takes `ADD <col>`, the others `ADD COLUMN <col>` (MySQL and
    // Postgres both also accept the bare ADD, but COLUMN reads clearer).
    let add = match engine {
        DbEngine::Mssql => "ADD",
        _ => "ADD COLUMN",
    };
    format!(
        "ALTER TABLE {} {add} {} {}",
        target_sql(engine, schema, table),
        engine.quote_ident(&col.name),
        column_type_sql(dialect, &col.data_type)
    )
}

/// Outcome of [`plan_from_comparison`].
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub plan: DbWriteBackPlan,
    /// Columns in the desired table that the server table does not have. They
    /// are reported, never added: a script that silently ALTERs a production
    /// table is not what a reviewer asked for.
    pub ignored_columns: Vec<String>,
}

/// Whether two cells differ in a way worth writing.
///
/// [`crate::data::compare`] compares cells as text, which is right for a diff
/// report and wrong for generating SQL: a CSV `120.50` parses to `Float(120.5)`
/// while Postgres returns the exact `120.50`, so an unchanged row would be
/// reported as changed and the script would carry an UPDATE that changes
/// nothing. A reviewer who sees phantom statements stops trusting the script,
/// so numbers are compared as numbers and everything else as text.
fn cells_differ(a: &CellValue, b: &CellValue) -> bool {
    let (sa, sb) = (a.to_string(), b.to_string());
    if sa == sb {
        return false;
    }
    match (sa.trim().parse::<f64>(), sb.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x != y,
        _ => true,
    }
}

/// Turn a key-matched comparison of a server table against a desired table
/// into a write-back plan: rows only on the server become deletes, rows only
/// in the desired table become inserts, and matched rows that really differ
/// become full-row updates addressed by the key columns.
///
/// Shared by the CLI's `--sync-sql` and the MCP `sync_sql` tool, so the two
/// cannot disagree. `added_columns` is always empty by design (see
/// [`SyncPlan::ignored_columns`]).
pub fn plan_from_comparison(
    server: &DataTable,
    desired: &DataTable,
    result: &crate::data::compare::CompareResult,
    keys: &[String],
) -> anyhow::Result<SyncPlan> {
    let key_idx: Vec<usize> = keys
        .iter()
        .map(|k| {
            server
                .columns
                .iter()
                .position(|c| &c.name == k)
                .ok_or_else(|| anyhow::anyhow!("key column '{k}' is not in the server table"))
        })
        .collect::<anyhow::Result<_>>()?;

    let key_values = |t: &DataTable, row: usize| -> Vec<CellValue> {
        key_idx
            .iter()
            .map(|&c| t.get(row, c).cloned().unwrap_or(CellValue::Null))
            .collect()
    };
    let full_row = |t: &DataTable, row: usize| -> Vec<CellValue> {
        (0..t.col_count())
            .map(|c| t.get(row, c).cloned().unwrap_or(CellValue::Null))
            .collect()
    };

    let ignored_columns: Vec<String> = desired
        .columns
        .iter()
        .filter(|c| !server.columns.iter().any(|sc| sc.name == c.name))
        .map(|c| c.name.clone())
        .collect();

    let updates = result
        .changed
        .iter()
        .filter(|ch| {
            // `changed_columns` is text-based; re-check each named column
            // numerically so formatting alone does not produce an UPDATE.
            ch.changed_columns.iter().any(|name| {
                let si = server.columns.iter().position(|c| &c.name == name);
                let di = desired.columns.iter().position(|c| &c.name == name);
                match (si, di) {
                    (Some(si), Some(di)) => {
                        let sv = server.get(ch.row_a, si).cloned().unwrap_or(CellValue::Null);
                        let dv = desired
                            .get(ch.row_b, di)
                            .cloned()
                            .unwrap_or(CellValue::Null);
                        cells_differ(&sv, &dv)
                    }
                    // A column only one side has is a real difference.
                    _ => true,
                }
            })
        })
        .map(|ch| (key_values(server, ch.row_a), full_row(desired, ch.row_b)))
        .collect();

    Ok(SyncPlan {
        plan: DbWriteBackPlan {
            deletes: result
                .only_in_a
                .iter()
                .map(|&row| key_values(server, row))
                .collect(),
            updates,
            inserts: result
                .only_in_b
                .iter()
                .map(|&row| full_row(desired, row))
                .collect(),
            added_columns: Vec::new(),
        },
        ignored_columns,
    })
}

/// Render the plan as a reviewable SQL script instead of executing it.
///
/// The statements, their order and their escaping are the same ones
/// [`apply_write_back`] runs, because both go through the private builders in
/// this file. A reviewer approving this text is approving exactly what Confirm
/// would have done.
pub fn render_plan_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    pk_cols: &[String],
    plan: &DbWriteBackPlan,
) -> String {
    if plan.is_empty() {
        return format!(
            "-- no changes to write back to {}\n",
            target_sql(engine, schema, table)
        );
    }

    let begin = match engine {
        DbEngine::Mssql => "BEGIN TRANSACTION",
        _ => "BEGIN",
    };
    let mut out = String::new();
    out.push_str(&format!("{begin};\n"));

    for col in &plan.added_columns {
        out.push_str(&alter_add_sql(engine, schema, table, col));
        out.push_str(";\n");
    }
    for pk_vals in &plan.deletes {
        out.push_str(&delete_sql(engine, schema, table, pk_cols, pk_vals));
        out.push_str(";\n");
    }
    for (pk_vals, row) in &plan.updates {
        out.push_str(&update_sql(
            engine, schema, table, columns, pk_cols, pk_vals, row,
        ));
        out.push_str(";\n");
    }
    for row in &plan.inserts {
        out.push_str(&insert_sql(engine, schema, table, columns, row));
        out.push_str(";\n");
    }
    out.push_str("COMMIT;\n");
    out
}

/// Apply the plan in ONE transaction: `ALTER TABLE ADD` per new column,
/// DELETE by PK, full-row UPDATE by PK, INSERT new rows. Rolls back on any
/// error (mirrors `write_table_generic`'s transaction skeleton). The caller
/// has already checked `ensure_write_allowed`.
/// Refuse a statement that did not touch exactly one row.
///
/// Only enforced for [`RowIdentity::FullRow`], where nothing but this check
/// stands between an ambiguous WHERE clause and silently rewriting several
/// rows. `0` means the row is gone or was changed by someone else since the
/// tab loaded; `2` or more means the table holds duplicate rows that the
/// baseline values cannot tell apart. Both roll the transaction back.
fn verify_one(check: bool, affected: u64, what: &str) -> anyhow::Result<()> {
    if !check || affected == 1 {
        return Ok(());
    }
    if affected == 0 {
        bail!(
            "the {what} matched no row: this table has no key, so rows are matched on all their \
             values, and it has changed on the server since the tab was loaded"
        );
    }
    bail!(
        "the {what} matched {affected} rows: this table has no key, so rows are matched on all \
         their values, and these rows are identical. Nothing was changed."
    )
}

pub fn apply_write_back(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    identity: &RowIdentity,
    plan: &DbWriteBackPlan,
) -> anyhow::Result<DbWriteBackReport> {
    let pk_cols = identity.columns();
    let check = identity.needs_affected_row_check();
    let begin = match engine {
        DbEngine::Mssql => "BEGIN TRANSACTION",
        _ => "BEGIN",
    };
    connector.execute(begin).context("starting transaction")?;
    let result = (|| -> anyhow::Result<()> {
        for col in &plan.added_columns {
            connector
                .execute(&alter_add_sql(engine, schema, table, col))
                .with_context(|| format!("adding column '{}'", col.name))?;
        }
        for pk_vals in &plan.deletes {
            let n = connector
                .execute(&delete_sql(engine, schema, table, pk_cols, pk_vals))
                .context("deleting a row")?;
            verify_one(check, n, "delete")?;
        }
        for (pk_vals, row) in &plan.updates {
            let n = connector
                .execute(&update_sql(
                    engine, schema, table, columns, pk_cols, pk_vals, row,
                ))
                .context("updating a row")?;
            verify_one(check, n, "update")?;
        }
        for row in &plan.inserts {
            connector
                .execute(&insert_sql(engine, schema, table, columns, row))
                .context("inserting a row")?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            connector.execute("COMMIT").context("committing")?;
            Ok(DbWriteBackReport {
                deleted: plan.deletes.len(),
                updated: plan.updates.len(),
                inserted: plan.inserts.len(),
                added_columns: plan.added_columns.len(),
            })
        }
        Err(e) => {
            let _ = connector.execute("ROLLBACK");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DbRowMeta;
    use std::collections::HashMap;

    fn col(name: &str, ty: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: ty.into(),
        }
    }

    /// A two-column PK'd table: id (PK) + name, three baseline rows.
    fn base_table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![col("id", "Int64"), col("name", "Utf8")];
        t.rows = vec![
            vec![CellValue::Int(1), CellValue::String("a".into())],
            vec![CellValue::Int(2), CellValue::String("b".into())],
            vec![CellValue::Int(3), CellValue::String("c".into())],
        ];
        let mut original = HashMap::new();
        for (i, row) in t.rows.iter().enumerate() {
            original.insert(i as i64, row.clone());
        }
        t.db_meta = Some(DbRowMeta {
            table_name: "t".into(),
            schema: Some("public".into()),
            row_tags: vec![Some(0), Some(1), Some(2)],
            original,
            original_columns: vec!["id".into(), "name".into()],
        });
        t
    }

    fn pk() -> RowIdentity {
        RowIdentity::Key(vec!["id".into()])
    }

    /// A connector that records the SQL it was given and reports a fixed
    /// affected-row count, so the guard can be exercised without a server.
    struct CountingConnector {
        affected: u64,
        statements: Vec<String>,
    }

    impl DbConnector for CountingConnector {
        fn engine(&self) -> DbEngine {
            DbEngine::Postgres
        }
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, _: &str) -> anyhow::Result<DataTable> {
            Ok(DataTable::empty())
        }
        fn execute(&mut self, sql: &str) -> anyhow::Result<u64> {
            self.statements.push(sql.to_string());
            // Transaction control is not a row-affecting statement.
            if matches!(sql, "BEGIN" | "BEGIN TRANSACTION" | "COMMIT" | "ROLLBACK") {
                return Ok(0);
            }
            Ok(self.affected)
        }
        fn write_table(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: super::super::DbWriteMode,
            _: &DataTable,
        ) -> anyhow::Result<super::super::DbWriteReport> {
            unimplemented!("not used by these tests")
        }
    }

    fn full_row() -> RowIdentity {
        RowIdentity::FullRow(vec!["id".into(), "name".into()])
    }

    fn one_update_plan() -> DbWriteBackPlan {
        let mut t = base_table();
        t.rows[1][1] = CellValue::String("B".into());
        build_write_back_plan(&t, &full_row()).unwrap()
    }

    /// Without a key the WHERE clause carries every baseline value, which is
    /// what lets a keyless table be edited at all.
    #[test]
    fn a_full_row_match_addresses_the_row_by_all_its_values() {
        let plan = one_update_plan();
        assert_eq!(plan.updates.len(), 1);
        let (where_vals, _) = &plan.updates[0];
        assert_eq!(
            where_vals,
            &vec![CellValue::Int(2), CellValue::String("b".into())],
            "the WHERE uses the ORIGINAL values, not the edited ones"
        );
        let sql = update_sql(
            DbEngine::Postgres,
            "public",
            "t",
            &base_table().columns,
            full_row().columns(),
            where_vals,
            &plan.updates[0].1,
        );
        assert!(sql.contains(r#"WHERE "id" = 2 AND "name" = 'b'"#), "{sql}");
    }

    /// `= NULL` is never true, so a baseline NULL has to become `IS NULL` or
    /// the statement would match nothing and the save would silently no-op.
    #[test]
    fn a_null_in_the_baseline_is_matched_with_is_null() {
        let sql = pk_where_sql(
            DbEngine::Postgres,
            &["id".to_string(), "note".to_string()],
            &[CellValue::Int(1), CellValue::Null],
        );
        assert_eq!(sql, r#""id" = 1 AND "note" IS NULL"#);
    }

    /// A key column cannot be NULL, so a NULL there means the baseline is
    /// wrong and the row must not be addressed by it.
    #[test]
    fn a_null_key_value_is_still_refused() {
        let mut t = base_table();
        t.db_meta.as_mut().unwrap().original.get_mut(&1).unwrap()[0] = CellValue::Null;
        t.rows[1][1] = CellValue::String("B".into());
        let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
        assert!(err.contains("NULL"), "{err}");
        // The same baseline is fine for a full-row match.
        assert!(build_write_back_plan(&t, &full_row()).is_ok());
    }

    #[test]
    fn a_full_row_update_that_touches_exactly_one_row_commits() {
        let mut c = CountingConnector {
            affected: 1,
            statements: vec![],
        };
        let plan = one_update_plan();
        let report = apply_write_back(
            &mut c,
            DbEngine::Postgres,
            "public",
            "t",
            &base_table().columns,
            &full_row(),
            &plan,
        )
        .expect("one matched row commits");
        assert_eq!(report.updated, 1);
        assert!(c.statements.contains(&"COMMIT".to_string()));
    }

    /// Duplicate rows are the case this whole mode has to survive: the WHERE
    /// matches both, and rewriting both is exactly the data loss the guard
    /// exists to prevent.
    #[test]
    fn a_full_row_update_matching_two_rows_rolls_back() {
        let mut c = CountingConnector {
            affected: 2,
            statements: vec![],
        };
        let plan = one_update_plan();
        let err = apply_write_back(
            &mut c,
            DbEngine::Postgres,
            "public",
            "t",
            &base_table().columns,
            &full_row(),
            &plan,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("2 rows"), "{err}");
        assert!(c.statements.contains(&"ROLLBACK".to_string()));
        assert!(!c.statements.contains(&"COMMIT".to_string()));
    }

    /// Zero means someone else changed or removed the row since the tab
    /// loaded. Silently writing nothing and reporting success would be worse.
    #[test]
    fn a_full_row_update_matching_no_row_rolls_back() {
        let mut c = CountingConnector {
            affected: 0,
            statements: vec![],
        };
        let plan = one_update_plan();
        let err = apply_write_back(
            &mut c,
            DbEngine::Postgres,
            "public",
            "t",
            &base_table().columns,
            &full_row(),
            &plan,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("no row"), "{err}");
        assert!(c.statements.contains(&"ROLLBACK".to_string()));
    }

    /// The count is NOT checked for a keyed table: MySQL reports *changed*
    /// rather than *matched* rows, so a correct keyed update can legitimately
    /// report 0 and must not be rolled back.
    #[test]
    fn a_keyed_update_is_not_subject_to_the_count_check() {
        let mut c = CountingConnector {
            affected: 0,
            statements: vec![],
        };
        let mut t = base_table();
        t.rows[1][1] = CellValue::String("B".into());
        let plan = build_write_back_plan(&t, &pk()).unwrap();
        let report = apply_write_back(
            &mut c,
            DbEngine::Postgres,
            "public",
            "t",
            &base_table().columns,
            &pk(),
            &plan,
        )
        .expect("a keyed write is unaffected by the affected-row count");
        assert_eq!(report.updated, 1);
        assert!(c.statements.contains(&"COMMIT".to_string()));
    }

    #[test]
    fn unchanged_table_yields_an_empty_plan() {
        let plan = build_write_back_plan(&base_table(), &pk()).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn detects_update_insert_delete() {
        let mut t = base_table();
        // Edit row 1's name, delete row 2, insert a new row.
        t.rows[1][1] = CellValue::String("B".into());
        t.rows.remove(2);
        t.rows
            .push(vec![CellValue::Int(9), CellValue::String("z".into())]);
        let meta = t.db_meta.as_mut().unwrap();
        meta.row_tags = vec![Some(0), Some(1), None];

        let plan = build_write_back_plan(&t, &pk()).unwrap();
        assert_eq!(plan.deletes, vec![vec![CellValue::Int(3)]]);
        assert_eq!(plan.updates.len(), 1);
        assert_eq!(plan.updates[0].0, vec![CellValue::Int(2)]);
        assert_eq!(plan.updates[0].1[1], CellValue::String("B".into()));
        assert_eq!(plan.inserts.len(), 1);
        assert_eq!(plan.inserts[0][0], CellValue::Int(9));
        assert_eq!(plan.change_count(), 3);
    }

    #[test]
    fn edited_pk_cell_is_located_by_its_original_value() {
        let mut t = base_table();
        t.rows[0][0] = CellValue::Int(100);
        let plan = build_write_back_plan(&t, &pk()).unwrap();
        // WHERE uses the ORIGINAL pk (1); SET writes the new one (100).
        assert_eq!(plan.updates[0].0, vec![CellValue::Int(1)]);
        assert_eq!(plan.updates[0].1[0], CellValue::Int(100));
    }

    #[test]
    fn added_column_forces_full_row_updates() {
        let mut t = base_table();
        t.columns.push(col("extra", "Utf8"));
        for row in &mut t.rows {
            row.push(CellValue::String("x".into()));
        }
        let plan = build_write_back_plan(&t, &pk()).unwrap();
        assert_eq!(plan.added_columns.len(), 1);
        assert_eq!(plan.updates.len(), 3, "every row rewritten");
    }

    #[test]
    fn removed_column_is_rejected() {
        let mut t = base_table();
        t.columns.remove(1);
        for row in &mut t.rows {
            row.remove(1);
        }
        let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
        assert!(err.contains("removed or renamed"), "{err}");
    }

    #[test]
    fn null_pk_value_is_rejected() {
        let mut t = base_table();
        t.db_meta
            .as_mut()
            .unwrap()
            .original
            .insert(0, vec![CellValue::Null, CellValue::String("a".into())]);
        t.rows[0][1] = CellValue::String("edited".into());
        let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
        assert!(err.contains("NULL"), "{err}");
    }

    #[test]
    fn missing_db_meta_is_identity_lost() {
        let mut t = base_table();
        t.db_meta = None;
        let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
        assert!(err.contains("row identity lost"), "{err}");
    }

    #[test]
    fn sql_rendering_per_engine() {
        let cols = [col("id", "Int64"), col("ok", "Boolean")];
        let identity = pk();
        let pk_cols = identity.columns();
        let pk_vals = [CellValue::Int(7)];
        let row = [CellValue::Int(7), CellValue::Bool(true)];

        assert_eq!(
            delete_sql(DbEngine::Postgres, "public", "t", pk_cols, &pk_vals),
            "DELETE FROM \"public\".\"t\" WHERE \"id\" = 7"
        );
        assert_eq!(
            update_sql(DbEngine::MySql, "app", "t", &cols, pk_cols, &pk_vals, &row),
            "UPDATE `app`.`t` SET `id` = 7, `ok` = TRUE WHERE `id` = 7"
        );
        // MSSQL renders booleans as BIT literals.
        assert_eq!(
            insert_sql(DbEngine::Mssql, "dbo", "t", &cols, &row),
            "INSERT INTO [dbo].[t] ([id], [ok]) VALUES (7, 1)"
        );
        assert_eq!(
            alter_add_sql(DbEngine::Mssql, "dbo", "t", &col("extra", "Utf8")),
            "ALTER TABLE [dbo].[t] ADD [extra] NVARCHAR(MAX)"
        );
        assert_eq!(
            alter_add_sql(DbEngine::Postgres, "public", "t", &col("extra", "Int64")),
            "ALTER TABLE \"public\".\"t\" ADD COLUMN \"extra\" BIGINT"
        );
    }

    #[test]
    fn alter_add_uses_the_same_dialect_as_create() {
        let col = ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        };
        // Snowflake spells it VARCHAR; the Postgres fallback would say TEXT.
        let alter = alter_add_sql(DbEngine::Snowflake, "s", "t", &col);
        assert!(alter.contains("VARCHAR"), "got: {alter}");
        let create = crate::db::create_table_sql(DbEngine::Snowflake, "s", "t", &[col]);
        assert!(create.contains("VARCHAR"), "got: {create}");
    }

    #[test]
    fn renders_plan_as_transactional_script() {
        let cols = vec![col("id", "Int64"), col("name", "Utf8")];
        let plan = DbWriteBackPlan {
            deletes: vec![vec![CellValue::Int(7)]],
            updates: vec![(
                vec![CellValue::Int(3)],
                vec![CellValue::Int(3), CellValue::String("O'Brien".into())],
            )],
            inserts: vec![vec![CellValue::Int(9), CellValue::String("new".into())]],
            added_columns: Vec::new(),
        };
        let sql = render_plan_sql(
            DbEngine::Postgres,
            "public",
            "people",
            &cols,
            &["id".to_string()],
            &plan,
        );

        assert!(sql.starts_with("BEGIN;\n"), "{sql}");
        assert!(sql.trim_end().ends_with("COMMIT;"), "{sql}");
        let del = sql.find("DELETE FROM").unwrap();
        let upd = sql.find("UPDATE").unwrap();
        let ins = sql.find("INSERT INTO").unwrap();
        assert!(
            del < upd && upd < ins,
            "statements out of apply order:\n{sql}"
        );

        assert!(sql.contains("'O''Brien'"), "quote not doubled:\n{sql}");
    }

    #[test]
    fn renders_added_columns_first_and_mssql_begins_transaction() {
        let cols = vec![col("id", "Int64")];
        let plan = DbWriteBackPlan {
            deletes: Vec::new(),
            updates: Vec::new(),
            inserts: Vec::new(),
            added_columns: vec![col("note", "Utf8")],
        };
        let sql = render_plan_sql(
            DbEngine::Mssql,
            "dbo",
            "people",
            &cols,
            &["id".to_string()],
            &plan,
        );
        assert!(sql.starts_with("BEGIN TRANSACTION;\n"), "{sql}");
        assert!(sql.contains("ALTER TABLE"), "{sql}");
    }

    #[test]
    fn empty_plan_renders_a_comment_not_an_empty_transaction() {
        let sql = render_plan_sql(
            DbEngine::Postgres,
            "public",
            "people",
            &[],
            &["id".to_string()],
            &DbWriteBackPlan::default(),
        );
        assert!(sql.contains("no changes"), "{sql}");
        assert!(!sql.contains("BEGIN"), "{sql}");
    }

    #[test]
    fn numeric_formatting_alone_is_not_a_change() {
        // A CSV reads 120.50 as a float; Postgres returns the exact text.
        assert!(!cells_differ(
            &CellValue::String("120.50".into()),
            &CellValue::Float(120.5)
        ));
        assert!(!cells_differ(
            &CellValue::Int(88),
            &CellValue::String("88.00".into())
        ));
        assert!(cells_differ(
            &CellValue::Float(120.5),
            &CellValue::Float(120.6)
        ));
        // NULL against a value is a change, not a formatting quirk.
        assert!(cells_differ(&CellValue::Null, &CellValue::Float(1.0)));
        // Text that merely looks numeric stays a text comparison.
        assert!(cells_differ(
            &CellValue::String("007".into()),
            &CellValue::String("7A".into())
        ));
    }

    #[test]
    fn comparison_becomes_deletes_updates_and_inserts() {
        use crate::data::compare::compare_join;

        let mut server = DataTable::empty();
        server.columns = vec![col("id", "Int64"), col("city", "Utf8")];
        server.rows = vec![
            vec![CellValue::Int(1), CellValue::String("London".into())],
            vec![CellValue::Int(2), CellValue::String("Baltimore".into())],
            vec![CellValue::Int(3), CellValue::String("Cambridge".into())],
        ];

        let mut desired = DataTable::empty();
        desired.columns = vec![col("id", "Int64"), col("city", "Utf8")];
        desired.rows = vec![
            vec![CellValue::Int(1), CellValue::String("London".into())],
            vec![CellValue::Int(2), CellValue::String("NEWCITY".into())],
            vec![CellValue::Int(5), CellValue::String("Helsinki".into())],
        ];

        let result = compare_join(&server, &desired, &["id".to_string()]).unwrap();
        let synced = plan_from_comparison(&server, &desired, &result, &["id".to_string()]).unwrap();

        assert_eq!(synced.plan.deletes, vec![vec![CellValue::Int(3)]]);
        assert_eq!(synced.plan.inserts.len(), 1);
        // Row 1 is untouched; only row 2 really changed.
        assert_eq!(synced.plan.updates.len(), 1);
        assert_eq!(synced.plan.updates[0].0, vec![CellValue::Int(2)]);
        // Never invents a schema change.
        assert!(synced.plan.added_columns.is_empty());
        assert!(synced.ignored_columns.is_empty());
    }

    #[test]
    fn extra_source_columns_are_reported_not_added() {
        use crate::data::compare::compare_join;

        let mut server = DataTable::empty();
        server.columns = vec![col("id", "Int64")];
        server.rows = vec![vec![CellValue::Int(1)]];

        let mut desired = DataTable::empty();
        desired.columns = vec![col("id", "Int64"), col("note", "Utf8")];
        desired.rows = vec![vec![CellValue::Int(1), CellValue::String("hi".into())]];

        let result = compare_join(&server, &desired, &["id".to_string()]).unwrap();
        let synced = plan_from_comparison(&server, &desired, &result, &["id".to_string()]).unwrap();
        assert_eq!(synced.ignored_columns, vec!["note".to_string()]);
        assert!(synced.plan.added_columns.is_empty());
    }

    #[test]
    fn a_key_missing_from_the_server_table_is_an_error() {
        use crate::data::compare::CompareResult;
        let mut server = DataTable::empty();
        server.columns = vec![col("id", "Int64")];
        let desired = server.clone();
        let err = plan_from_comparison(
            &server,
            &desired,
            &CompareResult::default(),
            &["nope".to_string()],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("nope"), "{err}");
    }
}
