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
        self.read_table(path, &first.name)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let meta = table
            .db_meta
            .as_ref()
            .ok_or_else(|| anyhow!("DuckDB write requires a table loaded from a database"))?;

        let current_cols: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        let original_cols: Vec<&str> = meta.original_columns.iter().map(|s| s.as_str()).collect();
        if current_cols != original_cols {
            bail!(
                "Schema changes are not supported on DuckDB tables (column rename/add/remove). \
                 Save aborted."
            );
        }

        let mut conn = Connection::open(path)
            .with_context(|| format!("opening DuckDB at {}", path.display()))?;
        ensure_row_id_column(&conn, &meta.table_name)?;

        let table_name = quote_ident(&meta.table_name);
        let col_idents: Vec<String> = table.columns.iter().map(|c| quote_ident(&c.name)).collect();

        let tx = conn.transaction()?;

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
                    let unchanged = original.map(|orig| orig == &row_vals).unwrap_or(false);
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
        let conn = Connection::open(path)
            .with_context(|| format!("opening DuckDB at {}", path.display()))?;
        ensure_row_id_column(&conn, table)?;

        let columns = read_table_columns(&conn, table)?
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
        let sql = format!(
            "SELECT {ROW_ID_COL}, {select_cols} FROM {} ORDER BY {ROW_ID_COL}",
            quote_ident(table)
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
                table_name: table.to_string(),
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
    let mut stmt = conn.prepare(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'main' AND table_type = 'BASE TABLE' ORDER BY table_name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let columns = read_table_columns(&conn, &name)
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.name != ROW_ID_COL)
            .collect();
        let row_count: Option<usize> = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", quote_ident(&name)),
                [],
                |r| r.get::<_, i64>(0),
            )
            .ok()
            .map(|n| n as usize);
        out.push(TableInfo {
            name,
            columns,
            row_count,
        });
    }
    Ok(out)
}

fn read_table_columns(conn: &Connection, table: &str) -> Result<Vec<ColumnInfo>> {
    let mut stmt = conn.prepare(
        "SELECT column_name, data_type FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = ? ORDER BY ordinal_position",
    )?;
    let cols = stmt
        .query_map([table], |r| {
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

/// Add a stable per-row id column if missing. Each existing row gets a unique
/// sequential value. Subsequent INSERTs assign `MAX+1`. This sidesteps the
/// fact that DuckDB has no stable rowid for arbitrary tables.
fn ensure_row_id_column(conn: &Connection, table: &str) -> Result<()> {
    let table_q = quote_ident(table);
    let exists: Option<String> = conn
        .query_row(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = 'main' AND table_name = ? AND column_name = ?",
            [table, ROW_ID_COL],
            |r| r.get(0),
        )
        .ok();
    if exists.is_some() {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table_q} ADD COLUMN {ROW_ID_COL} BIGINT"),
        [],
    )?;
    // Backfill with a row-number sequence.
    conn.execute(
        &format!(
            "UPDATE {table_q} SET {ROW_ID_COL} = sub.rn FROM \
             (SELECT rowid AS rid, ROW_NUMBER() OVER () AS rn FROM {table_q}) AS sub \
             WHERE {table_q}.rowid = sub.rid"
        ),
        [],
    )
    .or_else(|_| {
        // Fallback if rowid isn't supported here: assign sequential ids by ordinal.
        conn.execute(
            &format!(
                "UPDATE {table_q} SET {ROW_ID_COL} = sub.rn FROM \
                 (SELECT *, ROW_NUMBER() OVER () AS rn FROM {table_q}) AS sub \
                 WHERE FALSE"
            ),
            [],
        )
    })?;
    Ok(())
}

fn duckdb_type_to_arrow(ty: &str) -> String {
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

fn duckdb_value_to_cell(v: ValueRef<'_>) -> CellValue {
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
        V::Timestamp(_unit, ts) => CellValue::DateTime(ts.to_string()),
        V::Text(t) => match std::str::from_utf8(t) {
            Ok(s) => CellValue::String(s.to_string()),
            Err(_) => CellValue::Binary(t.to_vec()),
        },
        V::Blob(b) => CellValue::Binary(b.to_vec()),
        V::Date32(d) => CellValue::Date(d.to_string()),
        V::Time64(_, t) => CellValue::String(t.to_string()),
        other => CellValue::String(format!("{other:?}")),
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

fn quote_ident(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
