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
        self.write_file_schema_aware(path, table, false, &Default::default())
    }

    fn write_file_schema_aware(
        &self,
        path: &Path,
        table: &DataTable,
        allow_schema_changes: bool,
        opts: &crate::formats::write_options::WriteOptions,
    ) -> Result<()> {
        self.write_file_retagged(path, table, allow_schema_changes, opts)
            .map(|_| ())
    }

    fn write_file_retagged(
        &self,
        path: &Path,
        table: &DataTable,
        allow_schema_changes: bool,
        _opts: &crate::formats::write_options::WriteOptions,
    ) -> Result<Option<Vec<Option<i64>>>> {
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

        // INSERT / UPDATE per current row. The cells of a zoned column hold
        // the session's wall clock (that is what the reader wrote), so the
        // writer has to hand the instant back rather than the clock reading.
        let write_zones = column_zones(&table.columns);
        let next_id: i64 = tx
            .query_row(
                &format!("SELECT COALESCE(MAX({ROW_ID_COL}), 0) + 1 FROM {table_name}"),
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);
        let mut next_id = next_id;

        // `new_tags` collects the identity each row has in the file once this
        // transaction commits, so the caller can re-tag `db_meta` and a second
        // save does not INSERT this session's new rows all over again.
        let mut new_tags: Vec<Option<i64>> = Vec::with_capacity(meta.row_tags.len());
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
                    let mut params: Vec<duckdb::types::Value> = row_vals
                        .iter()
                        .zip(&write_zones)
                        .map(|(v, tz)| cell_to_duckdb_value_tz(v, tz.as_deref()))
                        .collect();
                    params.push(duckdb::types::Value::BigInt(next_id));
                    new_tags.push(Some(next_id));
                    next_id += 1;
                    tx.execute(&sql, duckdb::params_from_iter(params))?;
                }
                Some(tag) => {
                    new_tags.push(Some(*tag));
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
                    let mut params: Vec<duckdb::types::Value> = row_vals
                        .iter()
                        .zip(&write_zones)
                        .map(|(v, tz)| cell_to_duckdb_value_tz(v, tz.as_deref()))
                        .collect();
                    params.push(duckdb::types::Value::BigInt(*tag));
                    tx.execute(&sql, duckdb::params_from_iter(params))?;
                }
            }
        }

        tx.commit()?;
        Ok(Some(new_tags))
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

        let zones = column_zones(&columns);
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let tag: i64 = r.get(0)?;
            let mut row: Vec<CellValue> = Vec::with_capacity(col_count);
            for (i, tz) in zones.iter().enumerate() {
                let v = duckdb_value_to_cell_tz(r.get_ref(i + 1)?, tz.as_deref());
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
            formulas: std::collections::HashMap::new(),
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
    let session_tz = duckdb_session_timezone(conn);
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
                data_type: duckdb_type_to_arrow(&ty, session_tz.as_deref()),
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

/// The session timezone a DuckDB connection renders `TIMESTAMPTZ` in.
///
/// DuckDB stores a `TIMESTAMPTZ` as a UTC instant and picks the wall clock to
/// show it on from this setting, which defaults to the machine's zone. Octa
/// reads the raw instant, so it has to ask for the setting explicitly to show
/// the same clock the DuckDB CLI would. Asking is also the only reliable way:
/// the ICU extension applies the zone lazily, so DuckDB's *own* rendering of
/// the very first statement on a fresh connection can still come out in UTC.
///
/// `None` when the setting cannot be read, which leaves the column naive
/// rather than inventing a zone for it.
pub(crate) fn duckdb_session_timezone(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT current_setting('TimeZone')", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
    .filter(|tz| !tz.trim().is_empty())
}

/// Make DuckDB apply its session timezone before anything reads a column type.
///
/// The ICU extension installs the zone lazily: on a connection nothing has
/// asked yet, the first statement reports a `TIMESTAMPTZ` column as
/// `Timestamp(us, "UTC")` and renders it in UTC, and every statement after
/// that reports the real zone. Reading the setting is what triggers it, so a
/// connection that will hand out column types asks once up front and the
/// answer stops depending on statement order.
pub(crate) fn warm_session_timezone(conn: &Connection) {
    let _ = duckdb_session_timezone(conn);
}

/// The timezone of each column, by index, read back out of the type strings.
/// `None` for every column that is not a zoned timestamp.
pub(crate) fn column_zones(columns: &[ColumnInfo]) -> Vec<Option<String>> {
    columns
        .iter()
        .map(|c| {
            crate::formats::parquet_reader::timestamp_tz_from_type_name(&c.data_type)
                .map(str::to_string)
        })
        .collect()
}

/// Map a DuckDB type name onto Octa's column-type vocabulary.
///
/// `session_tz` names the zone a `TIMESTAMP WITH TIME ZONE` column is shown
/// in; it goes into the type string so the header says which clock the values
/// are on, and so the reader and writer can both find it again.
pub(crate) fn duckdb_type_to_arrow(ty: &str, session_tz: Option<&str>) -> String {
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
        match session_tz.filter(|_| upper.contains("WITH TIME ZONE")) {
            Some(tz) => format!("Timestamp(Microsecond, Some({tz:?}))"),
            None => "Timestamp(Microsecond, None)".into(),
        }
    } else {
        "Utf8".into()
    }
}

/// As [`duckdb_value_to_cell`], reading a timestamp on `tz`'s wall clock.
pub(crate) fn duckdb_value_to_cell_tz(v: ValueRef<'_>, tz: Option<&str>) -> CellValue {
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
        V::Timestamp(unit, ts) => duckdb_timestamp_to_cell_tz(unit, ts, tz),
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
    duckdb_timestamp_to_cell_tz(unit, ts, None)
}

/// As [`duckdb_timestamp_to_cell`], read on `tz`'s wall clock.
///
/// A `TIMESTAMPTZ` is a UTC instant; showing it in UTC while the connection
/// renders it in `Europe/Berlin` puts Octa two hours away from what every
/// other DuckDB client shows for the same row. `None` keeps the value exactly
/// as stored, which is right for a plain `TIMESTAMP`.
pub(crate) fn duckdb_timestamp_to_cell_tz(
    unit: duckdb::types::TimeUnit,
    ts: i64,
    tz: Option<&str>,
) -> CellValue {
    let (secs, nanos) = duckdb_unit_to_secs_nanos(unit, ts);
    match crate::formats::parquet_reader::arrow_instant_to_local(
        secs,
        nanos,
        tz,
        "%Y-%m-%d %H:%M:%S%.f",
    ) {
        Some(s) => CellValue::DateTime(s),
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

/// Bind one cell, spelling a zoned datetime so its instant cannot move.
///
/// The reader shows a `TIMESTAMPTZ` on the session's clock, so the writer has
/// to convert back out of that zone. It then writes the offset explicitly
/// (`...+00:00`) rather than handing DuckDB a bare wall clock: a bare string
/// is parsed against the session timezone, and the ICU extension applies that
/// setting lazily, so the same edit could land on a different instant
/// depending on whether anything had warmed the connection up first.
fn cell_to_duckdb_value_tz(v: &CellValue, tz: Option<&str>) -> duckdb::types::Value {
    use duckdb::types::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Boolean(*b),
        CellValue::Int(n) => Value::BigInt(*n),
        CellValue::Float(f) => Value::Double(*f),
        CellValue::DateTime(s) => match tz.and_then(|tz| local_datetime_to_utc_text(s, tz)) {
            Some(utc) => Value::Text(utc),
            None => Value::Text(s.clone()),
        },
        CellValue::String(s) | CellValue::Date(s) | CellValue::Nested(s) => Value::Text(s.clone()),
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}

/// Read a displayed datetime on `tz`'s clock and spell it back as UTC with an
/// explicit offset. `None` when the text is not a datetime Octa wrote, which
/// leaves the original string for DuckDB to interpret as it always did.
fn local_datetime_to_utc_text(s: &str, tz: &str) -> Option<String> {
    use chrono::TimeZone;
    let zone: chrono_tz::Tz = tz.trim().parse().ok()?;
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    // Earliest reading for the hour that happens twice, same as every other
    // timezone path in Octa.
    let instant = zone.from_local_datetime(&naive).earliest()?;
    Some(
        instant
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S%.f+00:00")
            .to_string(),
    )
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
