//! In-memory SQL execution against a `DataTable` via DuckDB.
//!
//! The current table is registered as a temporary table named `data` in an
//! in-memory DuckDB connection, the user's query is executed, and the result
//! is materialized back into a `DataTable`. Each call uses a fresh connection
//! — there is no persistent SQL state between runs.
//!
//! SELECT queries return rows as a new `DataTable` for display.
//! UPDATE / INSERT / DELETE (and other mutations) re-export the full contents
//! of `data` after the statement runs so the caller can replace the base table
//! — making SQL feel like a real database even for file-backed formats.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use duckdb::{Connection, types::ValueRef};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Classification of a SQL statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// A read-only query (SELECT etc.). `table` holds the result rows.
    Select,
    /// A mutation (INSERT / UPDATE / DELETE / …). `table` holds the full
    /// contents of `data` after the statement ran, suitable for replacing the
    /// base table in the caller's UI.
    Mutation,
}

/// Result of executing a user query.
#[derive(Debug, Clone)]
pub struct QueryOutcome {
    pub kind: QueryKind,
    /// Number of rows reported affected by a mutation (None for SELECT).
    pub affected: Option<usize>,
    /// For SELECT: the query result. For mutations: the post-mutation contents
    /// of `data`, rebuilt with the original table's column schema preserved.
    pub table: DataTable,
}

/// Execute `query` against `table`, returning a classified outcome.
/// The table is exposed in SQL as `data`. Identifiers in the schema are quoted,
/// so column names with spaces or punctuation are preserved.
pub fn run_query(table: &DataTable, query: &str) -> Result<QueryOutcome> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Query is empty"));
    }
    if let Some(egg) = octopuses_easter_egg(trimmed) {
        return Ok(QueryOutcome {
            kind: QueryKind::Select,
            affected: None,
            table: egg,
        });
    }
    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    register_table(&conn, "data", table)?;
    if is_mutation(trimmed) {
        let affected = conn.execute(trimmed, [])?;
        let mut mutated = execute_query(&conn, "SELECT * FROM data")?;
        // Preserve the original column schema (names + Arrow types) so the
        // base table keeps its typing. Column counts match as long as the
        // mutation didn't add/drop columns (ALTER TABLE); fall back to the
        // query-derived schema if they don't.
        if mutated.columns.len() == table.columns.len() {
            mutated.columns = table.columns.clone();
        }
        mutated.source_path = table.source_path.clone();
        mutated.format_name = table.format_name.clone();
        mutated.structural_changes = true;
        if let Some(meta) = table.db_meta.as_ref() {
            // For DB-backed tables, keep the original identity snapshot so
            // save-time diffing deletes originals and inserts current rows.
            // We can't map DuckDB's post-mutation rows back to rowids, so
            // every current row is flagged as "new" (None tag).
            let row_count = mutated.row_count();
            mutated.db_meta = Some(crate::data::DbRowMeta {
                table_name: meta.table_name.clone(),
                row_tags: vec![None; row_count],
                original: meta.original.clone(),
                original_columns: meta.original_columns.clone(),
            });
        }
        return Ok(QueryOutcome {
            kind: QueryKind::Mutation,
            affected: Some(affected),
            table: mutated,
        });
    }
    let result = execute_query(&conn, trimmed)?;
    Ok(QueryOutcome {
        kind: QueryKind::Select,
        affected: None,
        table: result,
    })
}

/// Classify `query` by its leading keyword. Mutating statements do not return
/// rows via `query()` in DuckDB's Rust bindings, so they must be run through
/// `execute()` instead. After a mutation we re-select `data` so the user sees
/// the effect of their change.
fn is_mutation(query: &str) -> bool {
    let first = query
        .split(|c: char| c.is_whitespace() || c == '(')
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        first.as_str(),
        "INSERT"
            | "UPDATE"
            | "DELETE"
            | "REPLACE"
            | "MERGE"
            | "CREATE"
            | "DROP"
            | "ALTER"
            | "TRUNCATE"
            | "ATTACH"
            | "DETACH"
            | "COPY"
            | "SET"
            | "PRAGMA"
    )
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

/// Easter egg: `SELECT * FROM octopuses` (case-insensitive, optional trailing
/// semicolon) returns a hand-crafted little aquarium. Anything more elaborate —
/// extra clauses, joins, projections — falls through to DuckDB unchanged.
fn octopuses_easter_egg(query: &str) -> Option<DataTable> {
    let normalized = query
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized != "select * from octopuses" {
        return None;
    }
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "species".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "tentacles".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "favorite_snack".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "iq".into(),
            data_type: "Int64".into(),
        },
    ];
    let rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("Inky".into()),
            CellValue::String("Common octopus".into()),
            CellValue::Int(8),
            CellValue::String("Crab".into()),
            CellValue::Int(140),
        ],
        vec![
            CellValue::Int(2),
            CellValue::String("Otto".into()),
            CellValue::String("Giant Pacific octopus".into()),
            CellValue::Int(8),
            CellValue::String("Lego brick".into()),
            CellValue::Int(155),
        ],
        vec![
            CellValue::Int(3),
            CellValue::String("Paul".into()),
            CellValue::String("Common octopus".into()),
            CellValue::Int(8),
            CellValue::String("Mussel (predicted)".into()),
            CellValue::Int(200),
        ],
        vec![
            CellValue::Int(4),
            CellValue::String("Mimi".into()),
            CellValue::String("Mimic octopus".into()),
            CellValue::Int(8),
            CellValue::String("Whatever the neighbors brought".into()),
            CellValue::Int(160),
        ],
        vec![
            CellValue::Int(5),
            CellValue::String("Blue".into()),
            CellValue::String("Blue-ringed octopus".into()),
            CellValue::Int(8),
            CellValue::String("Tiny shrimp (do not pet)".into()),
            CellValue::Int(120),
        ],
    ];
    Some(DataTable {
        columns,
        rows,
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("\u{1f419} Octopuses".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_table() -> DataTable {
        DataTable {
            columns: vec![ColumnInfo {
                name: "x".into(),
                data_type: "Int64".into(),
            }],
            rows: vec![],
            edits: HashMap::new(),
            source_path: None,
            format_name: None,
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    #[test]
    fn octopuses_egg_triggers_case_insensitively() {
        let t = empty_table();
        for q in [
            "SELECT * FROM octopuses",
            "select * from octopuses",
            "  SELECT   *   FROM   octopuses  ",
            "SELECT * FROM octopuses;",
        ] {
            let out = run_query(&t, q).expect(q);
            assert_eq!(out.kind, QueryKind::Select);
            assert_eq!(out.table.col_count(), 6);
            assert_eq!(out.table.row_count(), 5);
            assert_eq!(out.table.columns[0].name, "id");
            assert_eq!(out.table.columns[1].name, "name");
        }
    }

    #[test]
    fn octopuses_egg_does_not_swallow_real_queries() {
        let t = empty_table();
        let err = run_query(&t, "SELECT * FROM octopuses WHERE iq > 100").unwrap_err();
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("octopuses"),
            "expected DuckDB to complain about missing table `octopuses`, got: {msg}"
        );
    }
}
