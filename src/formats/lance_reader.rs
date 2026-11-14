//! Lance datasets, read and written through DuckDB's `lance` extension.
//!
//! A dataset is a **folder** (`_versions/`, `data/`, `_transactions/`), so the
//! path this reader takes is a directory, and the `.lance` files inside `data/`
//! are not tables of their own. Reading is [`lakehouse_reader`]'s scan, shared
//! with Delta and Iceberg. The extension is about 100 MB, installed on first use
//! and cached by DuckDB after that, which is why the GUI reads and saves these on
//! a worker whatever their size.
//!
//! A write replaces the dataset's contents with a new **version**: Lance commits
//! it atomically and the version before stays in the folder, so a failed write
//! changes nothing and no temp-and-rename or backup is wanted. The cost is that
//! every save stores the whole table again, which is why auto-save leaves these
//! tabs alone, and that Octa rebuilds no index built on the dataset.
//!
//! Types are kept exactly in both directions ([`exact_type`], [`stage_type`]):
//! vector, list and struct columns are what these datasets are for, and a save
//! that turned them into text would break whatever reads them next.

use std::path::Path;

use anyhow::{Context, Result, bail};
use duckdb::types::Value;

use super::FormatReader;
use super::duckdb_reader::is_nested;
use super::lakehouse_reader::{self, LakehouseKind};
use crate::data::times::TimeKind;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::sql::quote_ident;

/// The folder every Lance dataset has, and nothing else does.
const VERSIONS_DIR: &str = "_versions";

pub struct LanceReader;

impl FormatReader for LanceReader {
    fn name(&self) -> &str {
        "Lance"
    }

    fn extensions(&self) -> &[&str] {
        &["lance"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        refuse_a_data_file(path)?;
        lakehouse_reader::read_dir(path, LakehouseKind::Lance)
    }

    fn read_schema(&self, path: &Path) -> Result<Vec<ColumnInfo>> {
        refuse_a_data_file(path)?;
        lakehouse_reader::read_columns(path, LakehouseKind::Lance)
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_dataset(path, table)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn writes_a_folder(&self) -> bool {
        true
    }
}

/// Whether `dir` is a Lance dataset.
pub fn is_dataset(dir: &Path) -> bool {
    dir.join(VERSIONS_DIR).is_dir()
}

/// The dataset a data file belongs to: `<dataset>/data/<file>.lance`. The GUI
/// opens that instead of refusing the file. `None` for anything else, so a
/// stray `.lance` file still gets [`refuse_a_data_file`]'s message.
pub fn dataset_of(file: &Path) -> Option<&Path> {
    let is_lance = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lance"));
    let data = file.parent()?;
    let dataset = data.parent()?;
    (is_lance && file.is_file() && data.file_name()? == "data" && is_dataset(dataset))
        .then_some(dataset)
}

/// The table function reading the dataset at `dir_lit`, the body of a string
/// literal. A path ending in `.lance` is the documented way in; `__lance_scan`,
/// which that resolves to, is what reads a folder named anything else.
pub(super) fn scan(dir_lit: &str) -> String {
    if dir_lit.ends_with(".lance") {
        format!("'{dir_lit}'")
    } else {
        format!("__lance_scan('{dir_lit}')")
    }
}

/// A `.lance` file on its own is one fragment of a dataset, not a table.
fn refuse_a_data_file(path: &Path) -> Result<()> {
    if path.is_file() {
        bail!(
            "{} is one data file of a Lance dataset. Open the folder that holds the {VERSIONS_DIR} \
             folder instead.",
            path.display()
        );
    }
    Ok(())
}

/// Octa's name for the DuckDB type `raw`, one a [`stage_type`] turns back into the
/// same type. `None` for a type Octa has no cell for (vectors, lists, structs): the
/// column then keeps `raw` as its type and reads as text in DuckDB's notation, which
/// DuckDB can cast back.
pub(super) fn exact_type(raw: &str, session_tz: Option<&str>) -> Option<String> {
    let upper = raw.to_uppercase();
    let name = match upper.as_str() {
        "BOOLEAN" => "Boolean",
        "TINYINT" => "Int8",
        "SMALLINT" => "Int16",
        "INTEGER" => "Int32",
        "BIGINT" => "Int64",
        "UTINYINT" => "UInt8",
        "USMALLINT" => "UInt16",
        "UINTEGER" => "UInt32",
        "UBIGINT" => "UInt64",
        "FLOAT" => "Float32",
        "DOUBLE" => "Float64",
        "VARCHAR" => "Utf8",
        "BLOB" => "Binary",
        "DATE" => "Date32",
        "TIME" => "Time64(µs)",
        "TIMESTAMP" => "Timestamp(Microsecond, None)",
        "TIMESTAMP_S" => "Timestamp(Second, None)",
        "TIMESTAMP_MS" => "Timestamp(Millisecond, None)",
        "TIMESTAMP_NS" => "Timestamp(Nanosecond, None)",
        "TIMESTAMP WITH TIME ZONE" => {
            return Some(match session_tz {
                Some(tz) => format!("Timestamp(Microsecond, Some({tz:?}))"),
                None => "Timestamp(Microsecond, None)".to_string(),
            });
        }
        other => return crate::data::decimal::from_duckdb_type(other),
    };
    Some(name.to_string())
}

/// `data_type` when it is a DuckDB nested type that cannot leave the column
/// definition it is pasted into. A column type can arrive from outside Octa (an
/// agent's `write_table`), and this one goes into SQL unquoted, so anything but a
/// plain balanced type name is refused and the column is written as text.
///
/// ponytail: a struct field name holding other characters writes as text too;
/// a real parser of DuckDB's type grammar if anyone has such a dataset.
fn nested_type_for_sql(data_type: &str) -> Option<&str> {
    // Upper case is DuckDB's spelling: another format's `Map(Utf8, Int32)` is
    // nested too, but no DuckDB type.
    let duckdb_spelling = data_type.chars().take(2).all(|c| c.is_ascii_uppercase());
    if !duckdb_spelling || !is_nested(data_type) {
        return None;
    }
    let mut depth = 0i32;
    for ch in data_type.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | ' ' | ',' | '[' | ']' | '"' => {}
            _ => return None,
        }
    }
    (depth == 0).then_some(data_type)
}

/// The DuckDB type a column is staged as, the way back from [`exact_type`].
///
/// Durations go as text, in the display style: Lance refuses DuckDB's `INTERVAL`
/// outright. A zoned timestamp is staged on its wall clock, and [`copy_item`] says
/// which zone that clock was on. Anything unknown is text, which keeps every digit.
pub(super) fn stage_type(data_type: &str) -> String {
    if let Some(nested) = nested_type_for_sql(data_type) {
        return nested.to_string();
    }
    match TimeKind::of_type(data_type) {
        Some(TimeKind::Time) => return "TIME".to_string(),
        Some(TimeKind::Duration) => return "VARCHAR".to_string(),
        None => {}
    }
    if let Some(decimal) = crate::data::decimal::duckdb_type(data_type) {
        return decimal;
    }
    if let Some(args) = data_type.strip_prefix("Timestamp(") {
        let unit = args.split([',', ')']).next().unwrap_or_default().trim();
        return match unit {
            "Second" | "s" => "TIMESTAMP_S",
            "Millisecond" | "ms" => "TIMESTAMP_MS",
            "Nanosecond" | "ns" => "TIMESTAMP_NS",
            _ => "TIMESTAMP",
        }
        .to_string();
    }
    match data_type {
        // Fetched as text (the driver panics on it), still this type on disk.
        "TIME_NS" => "TIME_NS",
        "Boolean" => "BOOLEAN",
        "Int8" => "TINYINT",
        "Int16" => "SMALLINT",
        "Int32" => "INTEGER",
        "Int64" => "BIGINT",
        "UInt8" => "UTINYINT",
        "UInt16" => "USMALLINT",
        "UInt32" => "UINTEGER",
        "UInt64" => "UBIGINT",
        "Float16" | "Float32" => "FLOAT",
        "Float64" => "DOUBLE",
        "Binary" | "LargeBinary" => "BLOB",
        "Date32" | "Date64" => "DATE",
        _ => "VARCHAR",
    }
    .to_string()
}

/// A column in the `SELECT` handed to Lance: as staged, except a zoned timestamp,
/// which turns from its wall clock back into the instant it stands for.
fn copy_item(column: &ColumnInfo) -> String {
    let name = quote_ident(&column.name);
    match super::parquet_reader::timestamp_tz_from_type_name(&column.data_type) {
        Some(zone) => format!("timezone('{}', {name}) AS {name}", zone.replace('\'', "''")),
        None => name,
    }
}

/// What may be written over: nothing yet, an empty folder or a dataset. A plain
/// file makes DuckDB leave a `tmp_` file behind, and the extension would happily
/// write into a folder holding somebody's other files.
fn check_target(path: &Path) -> Result<()> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    if meta.is_file() {
        bail!(
            "{} is a file, and a Lance dataset is a folder. Nothing was written: pick another name.",
            path.display()
        );
    }
    let empty = std::fs::read_dir(path)
        .with_context(|| format!("reading {}", path.display()))?
        .next()
        .is_none();
    if !empty && !is_dataset(path) {
        bail!(
            "{} is a folder holding other files, not a Lance dataset. Nothing was written: pick \
             another name.",
            path.display()
        );
    }
    Ok(())
}

/// One cell as the staging table takes it. Text reaches any typed column through
/// DuckDB's own cast, so a vector cell someone edited into nonsense fails the write
/// rather than becoming something else.
fn stage_value(cell: &CellValue) -> Value {
    match cell {
        CellValue::Duration(_) => Value::Text(cell.to_string()),
        other => crate::sql::cell_to_value(other),
    }
}

/// Replace the dataset at `path` with `table`, as a new version of it.
///
/// ponytail: one full copy of the table in an in-memory DuckDB while it writes,
/// the cost of the GUI's whole-table saves too; feed slices if that ever hurts.
fn write_dataset(path: &Path, table: &DataTable) -> Result<()> {
    check_target(path)?;
    if table.columns.is_empty() {
        bail!("a Lance dataset needs at least one column");
    }
    let target = lakehouse_reader::dir_literal(path)?;
    let conn = lakehouse_reader::open_connection(LakehouseKind::Lance)?;
    let definitions: Vec<String> = table
        .columns
        .iter()
        .map(|c| format!("{} {}", quote_ident(&c.name), stage_type(&c.data_type)))
        .collect();
    conn.execute_batch(&format!("CREATE TABLE stage ({})", definitions.join(", ")))
        .context("preparing the columns")?;
    {
        let mut appender = conn.appender("stage").context("preparing the rows")?;
        for row in 0..table.row_count() {
            let cells = (0..table.col_count())
                .map(|col| stage_value(table.get(row, col).unwrap_or(&CellValue::Null)));
            appender
                .append_row(duckdb::appender_params_from_iter(cells))
                .with_context(|| format!("preparing row {}", row + 1))?;
        }
        appender.flush().context("preparing the rows")?;
    }
    let select: Vec<String> = table.columns.iter().map(copy_item).collect();
    conn.execute_batch(&format!(
        "COPY (SELECT {} FROM stage) TO '{target}' (FORMAT lance, MODE 'overwrite')",
        select.join(", ")
    ))
    .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[path = "lance_reader_tests.rs"]
mod tests;
