//! Shared helpers for MCP tool handlers. Each tool lives in its own
//! submodule so adding one is a drop-in (write the file, add it to the
//! `mod` list here, add a wrapper method to `OctaMcpServer`).

pub mod convert;
pub mod count_rows;
pub mod export_schema;
pub mod find_duplicates;
pub mod list_tables;
pub mod profile;
pub mod read_table;
pub mod run_sql;
pub mod schema;
pub mod search;
pub mod value_frequency;

use std::path::Path;

use serde_json::{Map, Value};

use octa::data::{CellValue, DataTable};
use octa::formats::FormatRegistry;

/// Read a file with the registry. Honours `table` when the source supports
/// multi-table dispatch (SQLite, DuckDB, GeoPackage), otherwise falls back
/// to `read_file`. Returns a friendly error when no reader claims the path.
pub fn read_with_registry(path: &Path, table: Option<&str>) -> anyhow::Result<DataTable> {
    let registry = FormatRegistry::new();
    let reader = registry
        .reader_for_path(path)
        .ok_or_else(|| anyhow::anyhow!("no reader available for {}", path.display()))?;
    match table {
        Some(name) => reader.read_table(path, name),
        None => reader.read_file(path),
    }
}

/// Serialise a `DataTable` into MCP-friendly JSON, capping the number of
/// rows at `row_cap` (None = unlimited) and the on-wire size of each cell
/// at `cell_byte_cap` (0 = unlimited). The shape is:
/// ```json
/// {
///   "schema": [{"name": "...", "type": "..."}, ...],
///   "rows":   [[v, v, ...], ...],
///   "row_count": N,
///   "truncated": false,
///   "total_rows_available": null,
///   "cell_truncated": false
/// }
/// ```
/// `truncated` is true when the table held more rows than `row_cap` and the
/// returned `rows` were shortened. `cell_truncated` is true when at least
/// one cell was replaced with a marker because it exceeded `cell_byte_cap`.
/// `total_rows_available` is only populated when we know it cheaply (i.e.
/// the table is already fully materialised in memory).
pub fn table_to_json(table: &DataTable, row_cap: Option<usize>, cell_byte_cap: usize) -> Value {
    let total = table.row_count();
    let emit = match row_cap {
        None => total,
        Some(0) => total,
        Some(n) => n.min(total),
    };
    let truncated = emit < total;

    let schema: Vec<Value> = table
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();

    let mut cell_truncated = false;
    let mut rows: Vec<Value> = Vec::with_capacity(emit);
    for r in 0..emit {
        let mut row: Vec<Value> = Vec::with_capacity(table.col_count());
        for c in 0..table.col_count() {
            let (v, was_truncated) =
                cell_to_json(table.get(r, c).unwrap_or(&CellValue::Null), cell_byte_cap);
            if was_truncated {
                cell_truncated = true;
            }
            row.push(v);
        }
        rows.push(Value::Array(row));
    }

    let mut out = Map::new();
    out.insert("schema".to_string(), Value::Array(schema));
    out.insert("rows".to_string(), Value::Array(rows));
    out.insert("row_count".to_string(), Value::from(emit));
    out.insert("truncated".to_string(), Value::Bool(truncated));
    out.insert("total_rows_available".to_string(), Value::from(total));
    out.insert("cell_truncated".to_string(), Value::Bool(cell_truncated));
    Value::Object(out)
}

/// Convert a single cell to JSON, honouring `cell_byte_cap` (0 = unlimited).
/// Returns `(value, was_truncated)`.
fn cell_to_json(cell: &CellValue, cell_byte_cap: usize) -> (Value, bool) {
    let v = match cell {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Int(i) => Value::from(*i),
        CellValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => Value::String(s.clone()),
        CellValue::Binary(b) => {
            // Hex-encoded; ASCII so byte length == char length.
            let mut s = String::with_capacity(b.len() * 2);
            for byte in b {
                use std::fmt::Write;
                let _ = write!(&mut s, "{byte:02x}");
            }
            Value::String(s)
        }
    };
    if cell_byte_cap == 0 {
        return (v, false);
    }
    let Value::String(s) = &v else {
        return (v, false);
    };
    if s.len() <= cell_byte_cap {
        return (v, false);
    }
    let marker = format!(
        "[truncated: {} bytes; cap {} bytes. Slice the value with --sql / run_sql to fetch the rest.]",
        s.len(),
        cell_byte_cap
    );
    (Value::String(marker), true)
}

/// Serialise a `DataTable`'s schema only (no rows).
pub fn schema_to_json(table: &DataTable) -> Value {
    let schema: Vec<Value> = table
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();
    let mut out = Map::new();
    out.insert("columns".to_string(), Value::Array(schema));
    out.insert("column_count".to_string(), Value::from(table.col_count()));
    Value::Object(out)
}
