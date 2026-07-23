use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use duckdb::{Connection, types::ValueRef};

use crate::data::{CellValue, ColumnInfo, DataTable, DbRowMeta};

use super::{FormatReader, TableInfo};

pub struct DuckDbReader;

const ROW_ID_COL: &str = "__octa_row_id";

impl FormatReader for DuckDbReader {
    fn name(&self) -> &str {
        "DuckDB"
    }

    fn extensions(&self) -> &[&str] {
        &["duckdb", "ddb"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let tables = list_user_tables(path)?;
        let first = tables
            .first()
            .ok_or_else(|| anyhow!("No tables found in DuckDB database"))?;
        self.read_table(path, &first.qualified_name())
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        self.write_file_schema_aware(path, table, false)
    }

    fn write_file_schema_aware(
        &self,
        path: &Path,
        table: &DataTable,
        allow_schema_changes: bool,
    ) -> Result<()> {
        let meta = table
            .db_meta
            .as_ref()
            .ok_or_else(|| anyhow!("DuckDB write requires a table loaded from a database"))?;

        let mut conn = Connection::open(path)
            .with_context(|| format!("opening DuckDB at {}", path.display()))?;
        let schema = meta.schema.as_deref().unwrap_or("main");
        ensure_row_id_column(&conn, schema, &meta.table_name)?;

        // Detect schema changes against the LIVE DB columns (not the in-memory
        // baseline), excluding the synthetic id column.
        let db_cols: Vec<ColumnInfo> = read_table_columns(&conn, schema, &meta.table_name)?
            .into_iter()
            .filter(|c| c.name != ROW_ID_COL)
            .collect();
        let schema_changed = db_cols.len() != table.columns.len()
            || db_cols
                .iter()
                .zip(table.columns.iter())
                .any(|(a, b)| a.name != b.name || a.data_type != b.data_type);

        let table_name = qualified_quote(schema, &meta.table_name);
        let col_idents: Vec<String> = table.columns.iter().map(|c| quote_ident(&c.name)).collect();

        let tx = conn.transaction()?;

        if schema_changed {
            if !allow_schema_changes {
                bail!(
                    "Schema changes (add / remove / rename / retype columns) are turned off. \
                     Turn off Write protection in Settings to allow them. Save aborted."
                );
            }
            reconcile_duckdb_schema(&tx, &table_name, &db_cols, &table.columns)?;
        }

        // DELETE rows whose tag is no longer present.
        let live_tags: std::collections::HashSet<i64> = meta
            .row_tags
            .iter()
            .filter_map(|t| t.as_ref().copied())
            .collect();
        for tag in meta.original.keys() {
            if !live_tags.contains(tag) {
                tx.execute(
                    &format!("DELETE FROM {table_name} WHERE {ROW_ID_COL} = ?"),
                    [tag],
                )?;
            }
        }

        // INSERT / UPDATE per current row.
        let next_id: i64 = tx
            .query_row(
                &format!("SELECT COALESCE(MAX({ROW_ID_COL}), 0) + 1 FROM {table_name}"),
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);
        let mut next_id = next_id;

        for (row_idx, tag) in meta.row_tags.iter().enumerate() {
            let row_vals: Vec<CellValue> = (0..table.columns.len())
                .map(|c| table.get(row_idx, c).cloned().unwrap_or(CellValue::Null))
                .collect();
            match tag {
                None => {
                    let placeholders: Vec<String> =
                        (0..col_idents.len() + 1).map(|_| "?".to_string()).collect();
                    let sql = format!(
                        "INSERT INTO {table_name} ({}, {ROW_ID_COL}) VALUES ({})",
                        col_idents.join(", "),
                        placeholders.join(", ")
                    );
                    let mut params: Vec<duckdb::types::Value> =
                        row_vals.iter().map(cell_to_duckdb_value).collect();
                    params.push(duckdb::types::Value::BigInt(next_id));
                    next_id += 1;
                    tx.execute(&sql, duckdb::params_from_iter(params))?;
                }
                Some(tag) => {
                    let original = meta.original.get(tag);
                    // After a schema change the added / retyped columns must be
                    // written even for rows whose cells "match" the stale
                    // baseline, so the unchanged short-circuit is gated on it.
                    let unchanged =
                        !schema_changed && original.map(|orig| orig == &row_vals).unwrap_or(false);
                    if unchanged {
                        continue;
                    }
                    let assignments: Vec<String> = col_idents
                        .iter()
                        .map(|ident| format!("{ident} = ?"))
                        .collect();
                    let sql = format!(
                        "UPDATE {table_name} SET {} WHERE {ROW_ID_COL} = ?",
                        assignments.join(", ")
                    );
                    let mut params: Vec<duckdb::types::Value> =
                        row_vals.iter().map(cell_to_duckdb_value).collect();
                    params.push(duckdb::types::Value::BigInt(*tag));
                    tx.execute(&sql, duckdb::params_from_iter(params))?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        Ok(Some(list_user_tables(path)?))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        // Accept either a bare table name (defaults to `main`) or a
        // schema-qualified `schema.table` produced by `TableInfo::qualified_name`.
        // Split only on the first `.`; that keeps DuckDB's "tables with a dot
        // in the name" edge case workable as long as the caller passes the
        // bare name directly. The picker never produces such names (its source
        // is `information_schema`, which already gives us schema + name
        // separately).
        let (schema_owned, table_name): (Option<String>, String) = match table.split_once('.') {
            Some((s, t)) => (Some(s.to_string()), t.to_string()),
            None => (None, table.to_string()),
        };
        let schema_str = schema_owned.as_deref().unwrap_or("main");

        let conn = Connection::open(path)
            .with_context(|| format!("opening DuckDB at {}", path.display()))?;
        // Reading must NEVER mutate the file: the synthetic id column is only
        // materialised on save (`ensure_row_id_column` in the write path).
        // Until then DuckDB's implicit `rowid` provides the row tags; the
        // save-time backfill assigns `__octa_row_id = rowid`, so tags
        // collected here still address the same rows (under the same
        // file-unchanged-between-load-and-save assumption diff-saves already
        // make). Files that were saved before keep using their id column.
        let has_row_id = has_row_id_column(&conn, schema_str, &table_name)?;

        let columns = read_table_columns(&conn, schema_str, &table_name)?
            .into_iter()
            .filter(|c| c.name != ROW_ID_COL)
            .collect::<Vec<_>>();
        if columns.is_empty() {
            bail!("Table '{table}' has no columns");
        }

        let select_cols = columns
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let tag_col = if has_row_id { ROW_ID_COL } else { "rowid" };
        let sql = format!(
            "SELECT {tag_col}, {select_cols} FROM {} ORDER BY {tag_col}",
            qualified_quote(schema_str, &table_name)
        );
        let mut stmt = conn.prepare(&sql)?;
        let col_count = columns.len();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        let mut row_tags: Vec<Option<i64>> = Vec::new();
        let mut original: HashMap<i64, Vec<CellValue>> = HashMap::new();

        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let tag: i64 = r.get(0)?;
            let mut row: Vec<CellValue> = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let v = duckdb_value_to_cell(r.get_ref(i + 1)?);
                row.push(v);
            }
            original.insert(tag, row.clone());
            rows.push(row);
            row_tags.push(Some(tag));
        }

        let original_columns: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

        Ok(DataTable {
            columns,
            rows,
            edits: HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("DuckDB".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: Some(DbRowMeta {
                table_name: table_name.clone(),
                schema: schema_owned,
                row_tags,
                original,
                original_columns,
            }),
        })
    }
}

fn list_user_tables(path: &Path) -> Result<Vec<TableInfo>> {
    let conn =
        Connection::open(path).with_context(|| format!("opening DuckDB at {}", path.display()))?;
    // Enumerate every user schema, not just `main`. System schemas
    // (`information_schema`, `pg_catalog`) are excluded explicitly; the
    // bundled DuckDB build also ships a `temp` schema for the connection's
    // session-scoped tables which we hide for the same reason.
    let mut stmt = conn.prepare(
        "SELECT table_schema, table_name FROM information_schema.tables \
         WHERE table_type = 'BASE TABLE' \
           AND table_schema NOT IN ('information_schema', 'pg_catalog', 'temp') \
         ORDER BY (table_schema = 'main') DESC, table_schema, table_name",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (schema, name) in rows {
        let columns = read_table_columns(&conn, &schema, &name)
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.name != ROW_ID_COL)
            .collect();
        let row_count: Option<usize> = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", qualified_quote(&schema, &name)),
                [],
                |r| r.get::<_, i64>(0),
            )
            .ok()
            .map(|n| n as usize);
        out.push(TableInfo {
            name,
            schema: Some(schema),
            columns,
            row_count,
        });
    }
    Ok(out)
}

fn read_table_columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<ColumnInfo>> {
    let mut stmt = conn.prepare(
        "SELECT column_name, data_type FROM information_schema.columns \
         WHERE table_schema = ? AND table_name = ? ORDER BY ordinal_position",
    )?;
    let cols = stmt
        .query_map([schema, table], |r| {
            let name: String = r.get(0)?;
            let ty: String = r.get(1)?;
            Ok(ColumnInfo {
                name,
                data_type: duckdb_type_to_arrow(&ty),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

/// Whether the table already carries the synthetic id column (i.e. it was
/// diff-saved by Octa before).
fn has_row_id_column(conn: &Connection, schema: &str, table: &str) -> Result<bool> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = ? AND table_name = ? AND column_name = ?",
            [schema, table, ROW_ID_COL],
            |r| r.get(0),
        )
        .ok();
    Ok(exists.is_some())
}

/// Add a stable per-row id column if missing (SAVE time only - reading never
/// mutates the file). Existing rows are backfilled with their current
/// `rowid`, which is exactly what the read path used as row tags, so the
/// in-memory diff still addresses the right rows. Subsequent INSERTs assign
/// `MAX+1`. This sidesteps the fact that DuckDB's rowid is not stable across
/// deletes.
fn ensure_row_id_column(conn: &Connection, schema: &str, table: &str) -> Result<()> {
    let table_q = qualified_quote(schema, table);
    if has_row_id_column(conn, schema, table)? {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table_q} ADD COLUMN {ROW_ID_COL} BIGINT"),
        [],
    )?;
    // Backfill with the current rowid: it must match the tags the read path
    // collected for this load, not an arbitrary row-number sequence.
    conn.execute(&format!("UPDATE {table_q} SET {ROW_ID_COL} = rowid"), [])?;
    Ok(())
}

/// Inverse of [`duckdb_type_to_arrow`] for `ADD COLUMN`. Lossy but round-trips
/// the types our reader produces, so an unchanged column never looks "retyped".
fn arrow_to_duckdb_type(arrow: &str) -> &'static str {
    if arrow.starts_with("Int") || arrow.starts_with("UInt") {
        "BIGINT"
    } else if arrow.starts_with("Float") {
        "DOUBLE"
    } else if arrow == "Boolean" {
        "BOOLEAN"
    } else if arrow == "Binary" {
        "BLOB"
    } else if arrow == "Date32" || arrow == "Date64" {
        "DATE"
    } else if arrow.starts_with("Timestamp") {
        "TIMESTAMP"
    } else {
        "VARCHAR"
    }
}

pub(crate) fn duckdb_type_to_arrow(ty: &str) -> String {
    let upper = ty.to_uppercase();
    if upper.contains("BIGINT")
        || upper.contains("INTEGER")
        || upper.contains("HUGEINT")
        || upper.starts_with("INT")
        || upper.contains("SMALLINT")
        || upper.contains("TINYINT")
    {
        "Int64".into()
    } else if upper.contains("DOUBLE")
        || upper.contains("REAL")
        || upper.contains("FLOAT")
        || upper.contains("DECIMAL")
        || upper.contains("NUMERIC")
    {
        "Float64".into()
    } else if upper.contains("BOOL") {
        "Boolean".into()
    } else if upper.contains("BLOB") || upper.contains("BYTEA") {
        "Binary".into()
    } else if upper.contains("DATE") && !upper.contains("TIME") {
        "Date32".into()
    } else if upper.contains("TIMESTAMP") || upper.contains("DATETIME") {
        "Timestamp(Microsecond, None)".into()
    } else {
        "Utf8".into()
    }
}

pub(crate) fn duckdb_value_to_cell(v: ValueRef<'_>) -> CellValue {
    use duckdb::types::ValueRef as V;
    match v {
        V::Null => CellValue::Null,
        V::Boolean(b) => CellValue::Bool(b),
        V::TinyInt(i) => CellValue::Int(i as i64),
        V::SmallInt(i) => CellValue::Int(i as i64),
        V::Int(i) => CellValue::Int(i as i64),
        V::BigInt(i) => CellValue::Int(i),
        V::HugeInt(i) => CellValue::String(i.to_string()),
        V::UTinyInt(i) => CellValue::Int(i as i64),
        V::USmallInt(i) => CellValue::Int(i as i64),
        V::UInt(i) => CellValue::Int(i as i64),
        V::UBigInt(i) => CellValue::String(i.to_string()),
        V::Float(f) => CellValue::Float(f as f64),
        V::Double(f) => CellValue::Float(f),
        V::Decimal(d) => CellValue::String(d.to_string()),
        V::Timestamp(unit, ts) => duckdb_timestamp_to_cell(unit, ts),
        V::Text(t) => match std::str::from_utf8(t) {
            Ok(s) => CellValue::String(s.to_string()),
            Err(_) => CellValue::Binary(t.to_vec()),
        },
        V::Blob(b) => CellValue::Binary(b.to_vec()),
        V::Date32(d) => duckdb_date32_to_cell(d),
        V::Time64(unit, t) => duckdb_time_to_cell(unit, t),
        other => CellValue::String(format!("{other:?}")),
    }
}

/// Split a DuckDB temporal count expressed in `unit` into whole seconds plus
/// sub-second nanoseconds. Euclidean div/rem keep pre-1970 (negative) values
/// on the correct second.
fn duckdb_unit_to_secs_nanos(unit: duckdb::types::TimeUnit, value: i64) -> (i64, u32) {
    use duckdb::types::TimeUnit;
    let (secs, nanos) = match unit {
        TimeUnit::Second => (value, 0),
        TimeUnit::Millisecond => (value.div_euclid(1_000), value.rem_euclid(1_000) * 1_000_000),
        TimeUnit::Microsecond => (
            value.div_euclid(1_000_000),
            value.rem_euclid(1_000_000) * 1_000,
        ),
        TimeUnit::Nanosecond => (
            value.div_euclid(1_000_000_000),
            value.rem_euclid(1_000_000_000),
        ),
    };
    (secs, nanos as u32)
}

/// Format a DuckDB `DATE` (days since the Unix epoch) as `YYYY-MM-DD`.
/// Shared with the SQL engine's result converter (`src/sql/engine.rs`).
pub(crate) fn duckdb_date32_to_cell(d: i32) -> CellValue {
    match chrono::DateTime::from_timestamp(d as i64 * 86_400, 0) {
        Some(dt) => CellValue::Date(dt.naive_utc().format("%Y-%m-%d").to_string()),
        None => CellValue::String(d.to_string()),
    }
}

/// Format a DuckDB `TIMESTAMP` (a count since the Unix epoch in `unit`) as a
/// canonical datetime string, matching the Parquet / Avro / ORC readers.
/// Falls back to the raw number on out-of-range values. Shared with the SQL
/// engine's result converter (`src/sql/engine.rs`).
pub(crate) fn duckdb_timestamp_to_cell(unit: duckdb::types::TimeUnit, ts: i64) -> CellValue {
    let (secs, nanos) = duckdb_unit_to_secs_nanos(unit, ts);
    match chrono::DateTime::from_timestamp(secs, nanos) {
        Some(dt) => CellValue::DateTime(dt.naive_utc().format("%Y-%m-%d %H:%M:%S%.f").to_string()),
        None => CellValue::String(ts.to_string()),
    }
}

/// Format a DuckDB `TIME` (a count since midnight in `unit`) as `HH:MM:SS[.f]`.
/// Shared with the SQL engine's result converter (`src/sql/engine.rs`).
pub(crate) fn duckdb_time_to_cell(unit: duckdb::types::TimeUnit, t: i64) -> CellValue {
    let (secs, nanos) = duckdb_unit_to_secs_nanos(unit, t);
    match u32::try_from(secs)
        .ok()
        .and_then(|s| chrono::NaiveTime::from_num_seconds_from_midnight_opt(s, nanos))
    {
        Some(tm) => CellValue::String(tm.format("%H:%M:%S%.f").to_string()),
        None => CellValue::String(t.to_string()),
    }
}

fn cell_to_duckdb_value(v: &CellValue) -> duckdb::types::Value {
    use duckdb::types::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Boolean(*b),
        CellValue::Int(n) => Value::BigInt(*n),
        CellValue::Float(f) => Value::Double(*f),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => Value::Text(s.clone()),
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}

/// Make the DB table's user columns match `target` by dropping columns that are
/// absent or whose type changed, then adding the ones that are missing. The row
/// diff-save that follows repopulates added / retyped columns from memory, so a
/// rename (drop old + add new) and a retype (drop + re-add) are both
/// data-preserving. The synthetic id column is never touched.
fn reconcile_duckdb_schema(
    tx: &duckdb::Transaction<'_>,
    table_q: &str,
    db_cols: &[ColumnInfo],
    target: &[ColumnInfo],
) -> Result<()> {
    use std::collections::HashSet;
    let target_match: HashSet<(&str, &str)> = target
        .iter()
        .map(|c| (c.name.as_str(), c.data_type.as_str()))
        .collect();
    // Drop DB columns absent from target, or present-but-retyped.
    for c in db_cols {
        if !target_match.contains(&(c.name.as_str(), c.data_type.as_str())) {
            tx.execute(
                &format!("ALTER TABLE {table_q} DROP COLUMN {}", quote_ident(&c.name)),
                [],
            )?;
        }
    }
    // What remains after the drops.
    let kept: HashSet<&str> = db_cols
        .iter()
        .filter(|c| target_match.contains(&(c.name.as_str(), c.data_type.as_str())))
        .map(|c| c.name.as_str())
        .collect();
    // Add target columns not currently present.
    for c in target {
        if !kept.contains(c.name.as_str()) {
            tx.execute(
                &format!(
                    "ALTER TABLE {table_q} ADD COLUMN {} {}",
                    quote_ident(&c.name),
                    arrow_to_duckdb_type(&c.data_type)
                ),
                [],
            )?;
        }
    }
    Ok(())
}

/// Quote a `schema.table` pair so each half is safe as an identifier. Quoting
/// both halves independently is critical: emitting `"schema.table"` as a
/// single quoted token would address a table named literally `schema.table`
/// inside the default schema, not the table `table` inside `schema`.
fn qualified_quote(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(table))
}

fn quote_ident(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
