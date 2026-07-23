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
    pk_cols: &[String],
) -> anyhow::Result<DbWriteBackPlan> {
    let Some(meta) = table.db_meta.as_ref() else {
        bail!(
            "row identity lost: the table was rewritten locally (e.g. by a SQL mutation); \
             reload the table or use Run on server"
        );
    };
    if pk_cols.is_empty() {
        bail!("the table has no primary key, so edits cannot be written back");
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
                if matches!(v, CellValue::Null) {
                    bail!("a primary-key value is NULL; the row cannot be addressed");
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

/// `pk = literal AND pk2 = literal` for a WHERE clause.
fn pk_where_sql(engine: DbEngine, pk_cols: &[String], pk_vals: &[CellValue]) -> String {
    pk_cols
        .iter()
        .zip(pk_vals)
        .map(|(col, val)| format!("{} = {}", engine.quote_ident(col), sql_literal(engine, val)))
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

/// Apply the plan in ONE transaction: `ALTER TABLE ADD` per new column,
/// DELETE by PK, full-row UPDATE by PK, INSERT new rows. Rolls back on any
/// error (mirrors `write_table_generic`'s transaction skeleton). The caller
/// has already checked `ensure_write_allowed`.
pub fn apply_write_back(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    pk_cols: &[String],
    plan: &DbWriteBackPlan,
) -> anyhow::Result<DbWriteBackReport> {
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
            connector
                .execute(&delete_sql(engine, schema, table, pk_cols, pk_vals))
                .context("deleting a row")?;
        }
        for (pk_vals, row) in &plan.updates {
            connector
                .execute(&update_sql(
                    engine, schema, table, columns, pk_cols, pk_vals, row,
                ))
                .context("updating a row")?;
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

    fn pk() -> Vec<String> {
        vec!["id".into()]
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
        let pk_cols = pk();
        let pk_vals = [CellValue::Int(7)];
        let row = [CellValue::Int(7), CellValue::Bool(true)];

        assert_eq!(
            delete_sql(DbEngine::Postgres, "public", "t", &pk_cols, &pk_vals),
            "DELETE FROM \"public\".\"t\" WHERE \"id\" = 7"
        );
        assert_eq!(
            update_sql(DbEngine::MySql, "app", "t", &cols, &pk_cols, &pk_vals, &row),
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
}
