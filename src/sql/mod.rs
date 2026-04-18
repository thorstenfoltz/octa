//! In-memory SQL execution against a `DataTable` via DuckDB.
//!
//! The current table is registered as a temporary table named `data` in an
//! in-memory DuckDB connection, the user's query is executed, and the result
//! is materialized back into a `DataTable`. Each call uses a fresh connection
//! — there is no persistent SQL state between runs.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use duckdb::{Connection, types::ValueRef};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Execute `query` against `table`, returning the results as a new `DataTable`.
/// The table is exposed in SQL as `data`. Identifiers in the schema are quoted,
/// so column names with spaces or punctuation are preserved.
pub fn run_query(table: &DataTable, query: &str) -> Result<DataTable> {
    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    register_table(&conn, "data", table)?;
    execute_query(&conn, query)
}

fn register_table(conn: &Connection, name: &str, table: &DataTable) -> Result<()> {
    if table.columns.is_empty() {
        return Ok(());
    }
    let cols_sql: Vec<String> = table
        .columns
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                quote_ident(&c.name),
                arrow_to_duckdb_type(&c.data_type)
            )
        })
        .collect();
    conn.execute(
        &format!(
            "CREATE TEMP TABLE {} ({})",
            quote_ident(name),
            cols_sql.join(", ")
        ),
        [],
    )?;

    if table.row_count() == 0 {
        return Ok(());
    }

    let mut app = conn
        .appender(name)
        .with_context(|| format!("opening DuckDB appender for `{name}`"))?;
    for row_idx in 0..table.row_count() {
        let row: Vec<duckdb::types::Value> = (0..table.col_count())
            .map(|c| cell_to_value(table.get(row_idx, c).unwrap_or(&CellValue::Null)))
            .collect();
        app.append_row(duckdb::appender_params_from_iter(row))?;
    }
    Ok(())
}

fn execute_query(conn: &Connection, query: &str) -> Result<DataTable> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Query is empty"));
    }

    let mut stmt = conn.prepare(trimmed)?;
    let mut q = stmt.query([])?;

    let stmt_ref = q
        .as_ref()
        .ok_or_else(|| anyhow!("Query produced no statement"))?;
    let col_count = stmt_ref.column_count();
    let columns: Vec<ColumnInfo> = (0..col_count)
        .map(|i| ColumnInfo {
            name: stmt_ref
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{i}")),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(col_count);
        for i in 0..col_count {
            row.push(value_ref_to_cell(r.get_ref(i)?));
        }
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("SQL Result".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

fn arrow_to_duckdb_type(arrow_ty: &str) -> &'static str {
    match arrow_ty {
        "Int64" | "Int32" | "Int16" | "Int8" => "BIGINT",
        "Float64" | "Float32" => "DOUBLE",
        "Boolean" => "BOOLEAN",
        "Date32" => "DATE",
        "Timestamp(Microsecond, None)" | "Timestamp(Millisecond, None)" => "TIMESTAMP",
        "Binary" | "LargeBinary" => "BLOB",
        _ => "VARCHAR",
    }
}

fn cell_to_value(v: &CellValue) -> duckdb::types::Value {
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

fn value_ref_to_cell(v: ValueRef<'_>) -> CellValue {
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
        V::Timestamp(_, ts) => CellValue::DateTime(ts.to_string()),
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

fn quote_ident(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
