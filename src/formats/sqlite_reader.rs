use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OpenFlags, types::ValueRef};

use crate::data::{CellValue, ColumnInfo, DataTable, DbRowMeta};

use super::{FormatReader, TableInfo};

pub struct SqliteReader;

impl FormatReader for SqliteReader {
    fn name(&self) -> &str {
        "SQLite"
    }

    fn extensions(&self) -> &[&str] {
        &["sqlite", "sqlite3", "db"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let tables = list_user_tables(path)?;
        let first = tables
            .first()
            .ok_or_else(|| anyhow!("No tables found in SQLite database"))?;
        self.read_table(path, &first.name)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let meta = table
            .db_meta
            .as_ref()
            .ok_or_else(|| anyhow!("SQLite write requires a table loaded from a database"))?;

        let current_cols: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        let original_cols: Vec<&str> = meta.original_columns.iter().map(|s| s.as_str()).collect();
        if current_cols != original_cols {
            bail!(
                "Schema changes are not supported on SQLite tables (column rename/add/remove). \
                 Save aborted."
            );
        }

        let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .with_context(|| format!("opening SQLite at {}", path.display()))?;
        let tx = conn.transaction()?;

        let table_name = quote_ident(&meta.table_name);
        let col_idents: Vec<String> = table.columns.iter().map(|c| quote_ident(&c.name)).collect();

        // DELETE rows whose tag is no longer present.
        let live_tags: std::collections::HashSet<i64> = meta
            .row_tags
            .iter()
            .filter_map(|t| t.as_ref().copied())
            .collect();
        for tag in meta.original.keys() {
            if !live_tags.contains(tag) {
                tx.execute(&format!("DELETE FROM {table_name} WHERE rowid = ?1"), [tag])?;
            }
        }

        // INSERT / UPDATE per current row.
        for (row_idx, tag) in meta.row_tags.iter().enumerate() {
            let row_vals: Vec<CellValue> = (0..table.columns.len())
                .map(|c| table.get(row_idx, c).cloned().unwrap_or(CellValue::Null))
                .collect();
            match tag {
                None => {
                    let placeholders: Vec<String> =
                        (1..=col_idents.len()).map(|i| format!("?{i}")).collect();
                    let sql = format!(
                        "INSERT INTO {table_name} ({}) VALUES ({})",
                        col_idents.join(", "),
                        placeholders.join(", ")
                    );
                    let params = rusqlite::params_from_iter(
                        row_vals
                            .iter()
                            .map(cell_to_sqlite_value)
                            .collect::<Vec<_>>(),
                    );
                    tx.execute(&sql, params)?;
                }
                Some(tag) => {
                    let original = meta.original.get(tag);
                    let unchanged = original.map(|orig| orig == &row_vals).unwrap_or(false);
                    if unchanged {
                        continue;
                    }
                    let assignments: Vec<String> = col_idents
                        .iter()
                        .enumerate()
                        .map(|(i, ident)| format!("{ident} = ?{}", i + 1))
                        .collect();
                    let sql = format!(
                        "UPDATE {table_name} SET {} WHERE rowid = ?{}",
                        assignments.join(", "),
                        col_idents.len() + 1
                    );
                    let mut params: Vec<rusqlite::types::Value> =
                        row_vals.iter().map(cell_to_sqlite_value).collect();
                    params.push(rusqlite::types::Value::Integer(*tag));
                    tx.execute(&sql, rusqlite::params_from_iter(params))?;
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
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening SQLite at {}", path.display()))?;

        let columns = read_table_columns(&conn, table)?;
        if columns.is_empty() {
            bail!("Table '{table}' has no columns");
        }

        let select_cols = columns
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT rowid, {select_cols} FROM {} ORDER BY rowid",
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
                let v = sqlite_value_to_cell(r.get_ref(i + 1)?);
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
            format_name: Some("SQLite".to_string()),
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
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening SQLite at {}", path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let columns = read_table_columns(&conn, &name).unwrap_or_default();
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
    let sql = format!("PRAGMA table_info({})", quote_ident(table));
    let mut stmt = conn.prepare(&sql)?;
    let cols = stmt
        .query_map([], |r| {
            let name: String = r.get(1)?;
            let ty: String = r.get(2)?;
            Ok(ColumnInfo {
                name,
                data_type: sqlite_type_to_arrow(&ty),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

fn sqlite_type_to_arrow(ty: &str) -> String {
    let upper = ty.to_uppercase();
    if upper.contains("INT") {
        "Int64".into()
    } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
        "Float64".into()
    } else if upper.contains("BLOB") {
        "Binary".into()
    } else if upper.contains("BOOL") {
        "Boolean".into()
    } else {
        "Utf8".into()
    }
}

fn sqlite_value_to_cell(v: ValueRef<'_>) -> CellValue {
    match v {
        ValueRef::Null => CellValue::Null,
        ValueRef::Integer(i) => CellValue::Int(i),
        ValueRef::Real(f) => CellValue::Float(f),
        ValueRef::Text(t) => match std::str::from_utf8(t) {
            Ok(s) => CellValue::String(s.to_string()),
            Err(_) => CellValue::Binary(t.to_vec()),
        },
        ValueRef::Blob(b) => CellValue::Binary(b.to_vec()),
    }
}

fn cell_to_sqlite_value(v: &CellValue) -> rusqlite::types::Value {
    use rusqlite::types::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Integer(if *b { 1 } else { 0 }),
        CellValue::Int(n) => Value::Integer(*n),
        CellValue::Float(f) => Value::Real(*f),
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
