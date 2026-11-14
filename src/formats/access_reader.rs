//! Microsoft Access database (`.mdb`, `.accdb`) reader. Read-only.
//!
//! The file is read by the `jetdb` crate (pure Rust, Access 97 to 2019); this
//! file only turns its values into cells. System and hidden tables are left
//! out, as are the columns Access hides. A Date/Time column whose every value
//! is at midnight becomes `Date`, else `DateTime`. Currency and Decimal stay
//! exact.
//!
//! A database with a password (an encrypted `.accdb`) opens with the
//! password the GUI's prompt handed to [`unlock`], else with the one in the
//! `OCTA_ACCESS_PASSWORD` environment variable, which is how the CLI and the
//! MCP server get it. A password never appears in an error. An `.mdb` with a
//! database password is not encrypted by it and opens without one.
//!
//! ponytail: attachments and multi-value columns read as empty.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use jetdb::format::ColumnType;
use jetdb::{
    FileError, PageReader, TableDef, Value, read_catalog, read_table_def, read_table_rows,
};

use crate::data::decimal::{decimal_parts, number_cell};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::{FormatReader, TableInfo};

pub struct AccessReader;

impl FormatReader for AccessReader {
    fn name(&self) -> &str {
        "Access"
    }

    fn extensions(&self) -> &[&str] {
        &["mdb", "accdb"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let (mut reader, tables) = open(path)?;
        let (name, page) = tables
            .first()
            .ok_or_else(|| anyhow!("the Access database holds no table"))?;
        table(&mut reader, name, *page, path)
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let (mut reader, tables) = open(path)?;
        let mut out = Vec::with_capacity(tables.len());
        for (name, page) in tables {
            // A table that cannot be described is still listed; opening it
            // then says why.
            let def = read_table_def(&mut reader, &name, page).ok();
            out.push(TableInfo {
                columns: def.as_ref().map(columns).unwrap_or_default(),
                row_count: def.map(|d| d.num_rows as usize),
                name,
                schema: None,
            });
        }
        Ok(Some(out))
    }

    fn read_table(&self, path: &Path, name: &str) -> Result<DataTable> {
        let (mut reader, tables) = open(path)?;
        let (name, page) = tables
            .iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| anyhow!("no table '{name}' in {}", path.display()))?;
        table(&mut reader, name, *page, path)
    }
}

/// The environment variable the CLI and the MCP server read a database
/// password from.
pub const PASSWORD_ENV: &str = "OCTA_ACCESS_PASSWORD";

/// Passwords that opened a database in this session, by path. Memory only.
static PASSWORDS: LazyLock<Mutex<HashMap<PathBuf, String>>> = LazyLock::new(Mutex::default);

fn password_for(path: &Path) -> Option<String> {
    let known = PASSWORDS.lock().ok()?.get(path).cloned();
    known.or_else(|| std::env::var(PASSWORD_ENV).ok().filter(|p| !p.is_empty()))
}

/// Whether `path` is an Access database that the password Octa has for it,
/// if any, does not open. Answered from the extension for any other file.
pub fn needs_password(path: &Path) -> bool {
    let access = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("accdb") || e.eq_ignore_ascii_case("mdb"));
    access
        && matches!(
            PageReader::open_with_password(path, password_for(path).as_deref()),
            Err(FileError::PasswordRequired | FileError::InvalidPassword)
        )
}

/// Try `password` on the database at `path` and keep it for this session
/// when it opens it. The error never repeats what was tried.
pub fn unlock(path: &Path, password: &str) -> Result<()> {
    match PageReader::open_with_password(path, Some(password)) {
        Ok(_) => {
            if let Ok(mut known) = PASSWORDS.lock() {
                known.insert(path.to_path_buf(), password.to_string());
            }
            Ok(())
        }
        Err(FileError::InvalidPassword | FileError::PasswordRequired) => {
            bail!("the password was rejected for this database")
        }
        Err(e) => Err(e).with_context(|| format!("reading Access database {}", path.display())),
    }
}

/// The open file and its user tables as `(name, definition page)`, by name.
fn open(path: &Path) -> Result<(PageReader, Vec<(String, u32)>)> {
    let context = || format!("reading Access database {}", path.display());
    let mut reader = match PageReader::open_with_password(path, password_for(path).as_deref()) {
        Err(FileError::PasswordRequired) => bail!(
            "{} is protected with a password: set the environment variable {PASSWORD_ENV} to it",
            path.display()
        ),
        Err(FileError::InvalidPassword) => {
            bail!("the password was rejected for {}", path.display())
        }
        other => other.with_context(context)?,
    };
    let mut tables: Vec<(String, u32)> = read_catalog(&mut reader)
        .with_context(context)?
        .into_iter()
        .filter(|e| e.object_type == jetdb::ObjectType::Table && !e.is_system_or_hidden())
        .map(|e| (e.name, e.table_page))
        .collect();
    tables.sort();
    Ok((reader, tables))
}

/// The columns Access shows, as indexes into a row.
fn shown(def: &TableDef) -> Vec<usize> {
    (0..def.columns.len())
        .filter(|&i| !def.columns[i].is_hidden())
        .collect()
}

fn columns(def: &TableDef) -> Vec<ColumnInfo> {
    shown(def)
        .into_iter()
        .map(|i| {
            let c = &def.columns[i];
            let decimal = format!("Decimal128({}, {})", c.precision, c.scale);
            ColumnInfo {
                name: c.name.clone(),
                data_type: match c.col_type {
                    ColumnType::Boolean => "Boolean",
                    ColumnType::Byte | ColumnType::Int | ColumnType::Long | ColumnType::BigInt => {
                        "Int64"
                    }
                    ColumnType::Float | ColumnType::Double => "Float64",
                    ColumnType::Money => "Decimal128(19, 4)",
                    ColumnType::Numeric if decimal_parts(&decimal).is_some() => &decimal,
                    ColumnType::Timestamp | ColumnType::DateTimeExtended => "DateTime",
                    ColumnType::Binary | ColumnType::Ole => "Binary",
                    _ => "Utf8",
                }
                .to_string(),
            }
        })
        .collect()
}

fn table(reader: &mut PageReader, name: &str, page: u32, path: &Path) -> Result<DataTable> {
    let context = || format!("reading table '{name}' of {}", path.display());
    let def = read_table_def(reader, name, page).with_context(context)?;
    let read = read_table_rows(reader, &def).with_context(context)?;
    read.warn_skipped(name);

    let keep = shown(&def);
    let mut columns = columns(&def);
    // Access has one date type; a column of whole days is a date.
    for (col, &i) in keep.iter().enumerate() {
        let days_only = |row: &Vec<Value>| match row[i] {
            Value::Timestamp(ts) => jetdb::timestamp::is_date_only(ts),
            _ => true,
        };
        if def.columns[i].col_type == ColumnType::Timestamp && read.rows.iter().all(days_only) {
            columns[col].data_type = "Date".to_string();
        }
    }
    let rows = read
        .rows
        .into_iter()
        .map(|mut row| {
            keep.iter()
                .zip(&columns)
                .map(|(&i, c)| cell(std::mem::replace(&mut row[i], Value::Null), &c.data_type))
                .collect()
        })
        .collect();
    Ok(DataTable {
        columns,
        rows,
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("Access".to_string()),
        ..DataTable::empty()
    })
}

fn cell(value: Value, data_type: &str) -> CellValue {
    use jetdb::timestamp::format_timestamp;
    let exact = |text: String| number_cell(&text).unwrap_or(CellValue::String(text));
    match value {
        Value::Null => CellValue::Null,
        Value::Bool(b) => CellValue::Bool(b),
        Value::Byte(n) => CellValue::Int(n.into()),
        Value::Int(n) => CellValue::Int(n.into()),
        Value::Long(n) => CellValue::Int(n.into()),
        Value::BigInt(n) => CellValue::Int(n),
        // Through text, so a Single of 0.1 is 0.1 and not 0.10000000149.
        Value::Float(f) => CellValue::Float(f.to_string().parse().unwrap_or(f64::from(f))),
        Value::Double(f) => CellValue::Float(f),
        Value::Money(s) | Value::Numeric(s) => exact(s),
        Value::Timestamp(ts) if data_type == "Date" => {
            CellValue::Date(format_timestamp(ts, "%Y-%m-%d"))
        }
        Value::Timestamp(ts) => CellValue::DateTime(format_timestamp(ts, "%Y-%m-%d %H:%M:%S")),
        Value::DateTimeExtended(s) => CellValue::DateTime(s),
        Value::Binary(b) => CellValue::Binary(b),
        Value::Text(s) | Value::Guid(s) => CellValue::String(s),
        // jetdb may grow value kinds; show one as it prints, do not drop it.
        other => CellValue::String(format!("{other:?}")),
    }
}

#[cfg(test)]
#[path = "access_reader_tests.rs"]
mod tests;
