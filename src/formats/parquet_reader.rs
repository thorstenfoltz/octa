use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Result, anyhow};
use arrow::array::*;
use arrow::datatypes::{
    ArrowPrimitiveType, DataType, Field, Int8Type, Int16Type, Int32Type, Int64Type, IntervalUnit,
    Schema, TimeUnit, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub struct ParquetReader;

/// Names that pandas writes for its on-disk row index when no explicit
/// index column was provided to `to_parquet`. Always stripped, even on
/// files with no `pandas` schema metadata, so files round-tripped through
/// older pandas releases (which didn't always write the metadata block)
/// also come through cleanly.
const PANDAS_DEFAULT_INDEX_NAMES: &[&str] = &["__index_level_0__"];

/// Inspect Arrow schema metadata for pandas' `index_columns` list and
/// return the column names that should be hidden from the user. pandas
/// writes a JSON blob under the key `pandas`; `index_columns` is the
/// relevant array - entries are either strings (column names) or
/// `{kind: "range"}` objects (auto-generated index, lives only in the
/// metadata, no Arrow column). The defaults at
/// [`PANDAS_DEFAULT_INDEX_NAMES`] are always included so missing /
/// malformed metadata doesn't leak `__index_level_0__`.
fn pandas_index_columns(metadata: &HashMap<String, String>) -> HashSet<String> {
    let mut out: HashSet<String> = PANDAS_DEFAULT_INDEX_NAMES
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let Some(blob) = metadata.get("pandas") else {
        return out;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(blob) else {
        return out;
    };
    let Some(items) = value.get("index_columns").and_then(|v| v.as_array()) else {
        return out;
    };
    for item in items {
        if let Some(name) = item.as_str() {
            out.insert(name.to_string());
        }
        // Object form (`{"kind": "range", ...}`) has no matching Arrow
        // column on disk, so nothing to filter - pandas drops the data
        // for the range index entirely.
    }
    out
}

impl FormatReader for ParquetReader {
    fn name(&self) -> &str {
        "Parquet"
    }

    fn extensions(&self) -> &[&str] {
        &["parquet", "pq", "parq"]
    }

    /// Footer only: no row group is decoded. Pandas index columns are stripped
    /// exactly as `read_via_arrow` strips them, so the reported schema is the
    /// one a full read would show.
    ///
    /// Falls back to the default body when the footer cannot be opened
    /// natively, which is the same case `read_file` hands to DuckDB (a file
    /// with more than 32,767 row groups).
    fn read_schema(&self, path: &Path) -> Result<Vec<ColumnInfo>> {
        let Ok(builder) = File::open(path)
            .map_err(anyhow::Error::from)
            .and_then(|f| ParquetRecordBatchReaderBuilder::try_new(f).map_err(anyhow::Error::from))
        else {
            return Ok(self.read_file(path)?.columns);
        };
        let schema = builder.schema();
        let drop_names = pandas_index_columns(schema.metadata());
        Ok(schema
            .fields()
            .iter()
            .filter(|f| !drop_names.contains(f.name()))
            .map(|f| ColumnInfo {
                name: f.name().clone(),
                data_type: arrow_type_name(f.data_type()),
            })
            .collect())
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_parquet(path, table)
    }

    fn write_file_with_options(
        &self,
        path: &Path,
        table: &DataTable,
        opts: &crate::formats::write_options::WriteOptions,
    ) -> Result<()> {
        write_parquet_with(path, table, &opts.parquet)
    }

    /// Try the native arrow-parquet reader first; if it errors (most commonly
    /// `Row group ordinal 32768 exceeds i16 max value` on files with > 32 767
    /// row groups), retry via DuckDB which uses its own parquet implementation
    /// without that hard cap. Wraps a second failure with an actionable
    /// recompact hint.
    fn read_file(&self, path: &Path) -> Result<DataTable> {
        match read_via_arrow(path) {
            Ok(table) => Ok(table),
            Err(arrow_err) => match read_via_duckdb(path) {
                Ok(table) => Ok(table),
                Err(duck_err) => Err(anyhow!(
                    "Parquet read failed via both arrow and DuckDB.\n  \
                     arrow error: {arrow_err:#}\n  \
                     DuckDB error: {duck_err:#}\n\
                     If the file was written with many small row groups \
                     (Spark / streaming ingest), recompact it, e.g.:\n  \
                     duckdb -c \"COPY (SELECT * FROM '{}') TO 'compact.parquet' \
                     (FORMAT PARQUET, ROW_GROUP_SIZE 1000000)\"",
                    path.display()
                )),
            },
        }
    }
}

/// Native arrow-parquet path. Honoured first because it preserves Arrow types
/// faithfully and is the fast path for well-formed files.
fn read_via_arrow(path: &Path) -> Result<DataTable> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let schema = builder.schema().clone();

    // Get total row count from Parquet metadata without reading data
    let metadata = builder.metadata();
    let total_file_rows: usize = metadata
        .row_groups()
        .iter()
        .map(|rg| rg.num_rows() as usize)
        .sum();

    // First-load cap: respects the user-configurable `initial_load_rows`
    // (default 2 M). Remaining rows stream in the background as the user
    // scrolls toward the bottom.
    let max_rows = super::initial_load_rows();
    let truncated = total_file_rows > max_rows;

    let reader = builder.with_batch_size(8192).build()?;

    // Pandas writes its row-index as a regular column (`__index_level_0__`
    // by default, or a named column listed under the schema's `pandas`
    // metadata key). Strip those columns from both the visible schema
    // and the per-row data - they're an artefact of the writer, not user
    // data, and surfacing them would force the user to manually drop the
    // column before doing anything useful with the file.
    let drop_names = pandas_index_columns(schema.metadata());
    let keep_indices: Vec<usize> = schema
        .fields()
        .iter()
        .enumerate()
        .filter_map(|(i, f)| (!drop_names.contains(f.name())).then_some(i))
        .collect();

    let columns: Vec<ColumnInfo> = keep_indices
        .iter()
        .map(|&i| {
            let f = schema.field(i);
            ColumnInfo {
                name: f.name().clone(),
                data_type: arrow_type_name(f.data_type()),
            }
        })
        .collect();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut loaded = 0usize;

    'outer: for batch_result in reader {
        let batch = batch_result?;
        let num_rows = batch.num_rows();

        for row_idx in 0..num_rows {
            if loaded >= max_rows {
                break 'outer;
            }
            let mut row = Vec::with_capacity(keep_indices.len());
            for &col_idx in &keep_indices {
                let array = batch.column(col_idx);
                let value = arrow_value_to_cell(array, row_idx);
                row.push(value);
            }
            rows.push(row);
            loaded += 1;
        }
    }

    let mut table = DataTable {
        columns,
        rows,
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("Parquet".to_string()),
        total_rows: if truncated {
            Some(total_file_rows)
        } else {
            None
        },
        ..DataTable::empty()
    };
    crate::data::times::settle_interval_columns(&mut table);
    Ok(table)
}

/// DuckDB-backed fallback. DuckDB's parquet reader has no i16 row-group
/// ordinal limit, so it can ingest files that the native arrow path rejects.
/// Rows are yielded as Arrow `RecordBatch`es so we reuse the same
/// `arrow_value_to_cell` mapping as the native path. The same `initial_load_rows`
/// cap applies via `LIMIT`.
fn read_via_duckdb(path: &Path) -> Result<DataTable> {
    let conn = duckdb::Connection::open_in_memory()?;
    let max_rows = super::initial_load_rows();
    // Use parameter binding for the path so single quotes in filenames don't break the SQL.
    // DuckDB recognises `?` for positional parameters.
    let sql = if max_rows == usize::MAX {
        "SELECT * FROM read_parquet(?)".to_string()
    } else {
        format!("SELECT * FROM read_parquet(?) LIMIT {max_rows}")
    };
    let path_str = path.to_string_lossy().to_string();
    let mut stmt = conn.prepare(&sql)?;
    let batches: Vec<arrow::record_batch::RecordBatch> =
        stmt.query_arrow([path_str.as_str()])?.collect();

    // Mirror the native path: drop pandas index columns from both the
    // schema and the per-row data. DuckDB's `read_parquet` surfaces
    // `__index_level_0__` as a regular column on files written by
    // pandas, so without this filter it leaks into the table view.
    let (columns, keep_indices): (Vec<ColumnInfo>, Vec<usize>) =
        if let Some(first) = batches.first() {
            let schema = first.schema();
            let drop_names = pandas_index_columns(schema.metadata());
            let keep: Vec<usize> = schema
                .fields()
                .iter()
                .enumerate()
                .filter_map(|(i, f)| (!drop_names.contains(f.name())).then_some(i))
                .collect();
            let cols: Vec<ColumnInfo> = keep
                .iter()
                .map(|&i| {
                    let f = schema.field(i);
                    ColumnInfo {
                        name: f.name().clone(),
                        data_type: arrow_type_name(f.data_type()),
                    }
                })
                .collect();
            (cols, keep)
        } else {
            // Empty result - fall back to a DESCRIBE for the column list.
            // No Arrow schema metadata available here, so the pandas filter
            // shrinks to the default-name list.
            let mut describe = conn.prepare(&format!(
                "DESCRIBE SELECT * FROM read_parquet({})",
                quote_sql_literal(&path_str)
            ))?;
            let raw_cols: Vec<(String, String)> = describe
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            let drop_names: HashSet<String> = PANDAS_DEFAULT_INDEX_NAMES
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            let keep: Vec<usize> = raw_cols
                .iter()
                .enumerate()
                .filter_map(|(i, (name, _))| (!drop_names.contains(name)).then_some(i))
                .collect();
            let cols: Vec<ColumnInfo> = keep
                .iter()
                .map(|&i| {
                    let (name, ty) = &raw_cols[i];
                    ColumnInfo {
                        name: name.clone(),
                        data_type: duckdb_type_to_arrow_name(ty),
                    }
                })
                .collect();
            (cols, keep)
        };

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for batch in &batches {
        let num_rows = batch.num_rows();
        for row_idx in 0..num_rows {
            let mut row = Vec::with_capacity(keep_indices.len());
            for &col_idx in &keep_indices {
                let array = batch.column(col_idx);
                row.push(arrow_value_to_cell(array, row_idx));
            }
            rows.push(row);
        }
    }

    // The DuckDB path uses LIMIT, so we can't know the full file row count
    // cheaply. Setting total_rows = None means the UI won't show a stale
    // "+N more" - that's acceptable for the fallback path.
    let loaded = rows.len();
    let truncated_marker = if max_rows != usize::MAX && loaded == max_rows {
        // Confirm there are more rows by running a fast count via DuckDB.
        let total: Option<i64> = conn
            .query_row(
                "SELECT count(*) FROM read_parquet(?)",
                [path_str.as_str()],
                |r| r.get(0),
            )
            .ok();
        total.and_then(|n| (n as usize > loaded).then_some(n as usize))
    } else {
        None
    };

    let mut table = DataTable {
        columns,
        rows,
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("Parquet".to_string()),
        total_rows: truncated_marker,
        ..DataTable::empty()
    };
    crate::data::times::settle_interval_columns(&mut table);
    Ok(table)
}

/// Single-quote a SQL string literal (escape inner single quotes).
fn quote_sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Minimal DuckDB->Arrow type-name mapping for the rare empty-result case in
/// the DuckDB fallback. The non-empty path gets full fidelity directly from
/// the Arrow schema on the first RecordBatch.
fn duckdb_type_to_arrow_name(ty: &str) -> String {
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
    } else if upper == "TIME" || upper == "TIME_NS" {
        "Time64(µs)".into()
    } else if upper.starts_with("INTERVAL") {
        "Duration(µs)".into()
    } else if upper.contains("DATE") && !upper.contains("TIME") {
        "Date32".into()
    } else if upper.contains("TIMESTAMP") || upper.contains("DATETIME") {
        "Timestamp(Microsecond, None)".into()
    } else {
        "Utf8".into()
    }
}

/// Convert an Arrow array value at a given index to a CellValue.
pub fn arrow_value_to_cell(array: &dyn Array, idx: usize) -> CellValue {
    if array.is_null(idx) {
        return CellValue::Null;
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            CellValue::Bool(arr.value(idx))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            CellValue::Int(arr.value(idx))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            let v = arr.value(idx);
            // `as i64` wrapped anything above i64::MAX negative, and Save then
            // wrote NULL; text keeps the digits and the writer parses them back.
            i64::try_from(v).map_or_else(|_| CellValue::String(v.to_string()), CellValue::Int)
        }
        DataType::Float16 => {
            let arr = array.as_any().downcast_ref::<Float16Array>().unwrap();
            CellValue::Float(arr.value(idx).to_f64())
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            CellValue::Float(arr.value(idx) as f64)
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            CellValue::Float(arr.value(idx))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            CellValue::String(arr.value(idx).to_string())
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            CellValue::String(arr.value(idx).to_string())
        }
        DataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::FixedSizeBinary(_) => {
            let arr = array
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::BinaryView => {
            let arr = array.as_any().downcast_ref::<BinaryViewArray>().unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::Utf8View => {
            let arr = array.as_any().downcast_ref::<StringViewArray>().unwrap();
            CellValue::String(arr.value(idx).to_string())
        }
        DataType::Date32 => {
            let arr = array.as_any().downcast_ref::<Date32Array>().unwrap();
            let days = arr.value(idx);
            let date = chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163);
            match date {
                Some(d) => CellValue::Date(d.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("date32({})", days)),
            }
        }
        DataType::Date64 => {
            let arr = array.as_any().downcast_ref::<Date64Array>().unwrap();
            let ms = arr.value(idx);
            let (secs, nsecs) = epoch_split(ms, 1_000);
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::Date(dt.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("date64({})", ms)),
            }
        }
        DataType::Timestamp(unit, tz) => {
            let (secs, nsecs) = match unit {
                TimeUnit::Second => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampSecondArray>()
                        .unwrap();
                    (arr.value(idx), 0u32)
                }
                TimeUnit::Millisecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampMillisecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    epoch_split(v, 1_000)
                }
                TimeUnit::Microsecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    epoch_split(v, 1_000_000)
                }
                TimeUnit::Nanosecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampNanosecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    epoch_split(v, 1_000_000_000)
                }
            };
            match arrow_instant_to_local(secs, nsecs, tz.as_deref(), "%Y-%m-%d %H:%M:%S%.3f") {
                Some(s) => CellValue::DateTime(s),
                None => CellValue::String(format!("timestamp({})", secs)),
            }
        }
        DataType::Time32(_) | DataType::Time64(_) | DataType::Duration(_) => {
            let (kind, unit, raw) = time_raw(array, idx);
            match raw.checked_mul(unit) {
                Some(ns) => kind.cell(ns),
                None => CellValue::String(display_value(array, idx)),
            }
        }
        DataType::Interval(IntervalUnit::DayTime) => {
            let v = array
                .as_any()
                .downcast_ref::<IntervalDayTimeArray>()
                .unwrap()
                .value(idx);
            i64::from(v.days)
                .checked_mul(crate::data::times::NANOS_PER_DAY)
                .and_then(|d| d.checked_add(i64::from(v.milliseconds) * 1_000_000))
                .map_or_else(
                    || CellValue::String(display_value(array, idx)),
                    CellValue::Duration,
                )
        }
        DataType::Interval(IntervalUnit::MonthDayNano) => {
            let v = array
                .as_any()
                .downcast_ref::<IntervalMonthDayNanoArray>()
                .unwrap()
                .value(idx);
            let ns = (v.months == 0)
                .then(|| i64::from(v.days).checked_mul(crate::data::times::NANOS_PER_DAY))
                .flatten()
                .and_then(|d| d.checked_add(v.nanoseconds));
            match ns {
                Some(ns) => CellValue::Duration(ns),
                // Months are calendar amounts (decision 7): keep the text.
                None => CellValue::String(display_value(array, idx)),
            }
        }
        DataType::Decimal128(_, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            let raw = arr.value(idx);
            let scale = *scale as u32;
            if scale == 0 {
                i64::try_from(raw)
                    .map_or_else(|_| CellValue::String(raw.to_string()), CellValue::Int)
            } else {
                let divisor = 10f64.powi(scale as i32);
                CellValue::Float(raw as f64 / divisor)
            }
        }
        DataType::Decimal256(_, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal256Array>().unwrap();
            let raw = arr.value(idx);
            CellValue::String(format!("{}e-{}", raw, scale))
        }
        // A pandas `category` column: the cell is the value its key points at.
        DataType::Dictionary(_, _) => downcast_dictionary_array!(
            array => array
                .key(idx)
                .map_or(CellValue::Null, |k| arrow_value_to_cell(array.values().as_ref(), k)),
            _ => CellValue::String(display_value(array, idx))
        ),
        DataType::List(_)
        | DataType::LargeList(_)
        | DataType::FixedSizeList(_, _)
        | DataType::Struct(_)
        | DataType::Map(_, _) => CellValue::Nested(display_value(array, idx)),
        _ => CellValue::String(display_value(array, idx)),
    }
}

/// Arrow's own rendering of one value (`01:02:03`, `PT90S`, `[1, 2]`) for the
/// types with no cell of their own. The Debug dump used before put
/// `PrimitiveArray<Time64(µs)>` into the cell (a dictionary's put its whole
/// value list into every cell), and a Save wrote that text into the file.
fn display_value(array: &dyn Array, idx: usize) -> String {
    arrow::util::display::array_value_to_string(array, idx)
        .unwrap_or_else(|_| format!("{:?}", array.slice(idx, 1)))
}

/// The kind, the unit in nanoseconds and the raw count of a time or
/// duration array value.
fn time_raw(array: &dyn Array, idx: usize) -> (crate::data::times::TimeKind, i64, i64) {
    use crate::data::times::TimeKind;
    macro_rules! get {
        ($t:ty) => {
            i64::from(array.as_any().downcast_ref::<$t>().unwrap().value(idx))
        };
    }
    match array.data_type() {
        DataType::Time32(TimeUnit::Second) => {
            (TimeKind::Time, 1_000_000_000, get!(Time32SecondArray))
        }
        DataType::Time32(_) => (TimeKind::Time, 1_000_000, get!(Time32MillisecondArray)),
        DataType::Time64(TimeUnit::Microsecond) => {
            (TimeKind::Time, 1_000, get!(Time64MicrosecondArray))
        }
        DataType::Time64(_) => (TimeKind::Time, 1, get!(Time64NanosecondArray)),
        DataType::Duration(TimeUnit::Second) => {
            (TimeKind::Duration, 1_000_000_000, get!(DurationSecondArray))
        }
        DataType::Duration(TimeUnit::Millisecond) => (
            TimeKind::Duration,
            1_000_000,
            get!(DurationMillisecondArray),
        ),
        DataType::Duration(TimeUnit::Microsecond) => {
            (TimeKind::Duration, 1_000, get!(DurationMicrosecondArray))
        }
        _ => (TimeKind::Duration, 1, get!(DurationNanosecondArray)),
    }
}

/// The type name a column carries. A dictionary column (pandas `category`) is
/// named by its values, which is what its cells hold, so a Save writes them as
/// that type rather than as text.
pub fn arrow_type_name(dt: &DataType) -> String {
    match dt {
        DataType::Dictionary(_, values) => values.to_string(),
        DataType::Interval(IntervalUnit::DayTime | IntervalUnit::MonthDayNano) => {
            "Duration(ns)".to_string()
        }
        other => other.to_string(),
    }
}

/// Split a count of `per_second` units since the Unix epoch into whole seconds
/// plus nanoseconds. Euclidean div/rem keep a pre-1970 value with a fractional
/// second on the right second: `/` and `%` gave a negative remainder that
/// wrapped to a nanosecond count chrono refuses, so the cell fell back to text.
/// Shared with the ORC and Avro readers.
pub fn epoch_split(value: i64, per_second: i64) -> (i64, u32) {
    (
        value.div_euclid(per_second),
        (value.rem_euclid(per_second) * (1_000_000_000 / per_second)) as u32,
    )
}

/// Format an epoch instant as the datetime string a cell shows, reading it on
/// the wall clock the Arrow type names.
///
/// An Arrow `Timestamp` holds an **instant**; the type's timezone says which
/// clock that instant should be read on. Formatting it directly gives UTC,
/// which is exactly what the column header then contradicts: a header reading
/// `Timestamp(Microsecond, Some("Europe/Brussels"))` above a cell showing the
/// UTC wall clock, two hours behind in summer.
///
/// `tz` takes either spelling Arrow allows: an IANA name (`Europe/Brussels`)
/// or a fixed offset (`+02:00`). One that parses as neither stays in UTC
/// rather than being guessed at - a wrong hour is worse than an unconverted
/// one, because nothing on screen would say it had happened.
pub fn arrow_instant_to_local(
    secs: i64,
    nsecs: u32,
    tz: Option<&str>,
    fmt: &str,
) -> Option<String> {
    let dt = chrono::DateTime::from_timestamp(secs, nsecs)?;
    let Some(tz) = tz.map(str::trim).filter(|t| !t.is_empty()) else {
        return Some(dt.format(fmt).to_string());
    };
    if let Ok(zone) = tz.parse::<chrono_tz::Tz>() {
        return Some(dt.with_timezone(&zone).format(fmt).to_string());
    }
    if let Ok(offset) = tz.parse::<chrono::FixedOffset>() {
        return Some(dt.with_timezone(&offset).format(fmt).to_string());
    }
    Some(dt.format(fmt).to_string())
}

/// Read the timezone back out of an Arrow type name, preserving its case.
///
/// A column's `data_type` holds `format!("{}", field.data_type())`, which for
/// a zoned timestamp is `Timestamp(µs, "Europe/Brussels")`. The writer has to
/// put that zone back or a save would turn a Brussels column into a naive one.
/// Taking whatever sits between the first pair of quotes also accepts the
/// `Debug` spelling (`Timestamp(Microsecond, Some("Europe/Brussels"))`), so a
/// type string from either side is understood.
///
/// IANA names are case-sensitive, so this reads the original string rather
/// than the lowercased copy [`data_type_from_string`] matches on.
pub(crate) fn timestamp_tz_from_type_name(s: &str) -> Option<&str> {
    crate::data::timestamp_timezone(s)
}

/// Map a DataTable data_type string back to an Arrow DataType.
///
/// Invariant shared with [`build_arrow_array`]: every `DataType` this can
/// return must have a matching builder arm there, because `write_parquet`
/// (and the Arrow IPC writer) derive the schema from this function and the
/// arrays from that one; `RecordBatch::try_new` rejects any disagreement.
/// Unknown names fall back to `Utf8` in both, so the pair stays consistent.
pub fn data_type_from_string(s: &str) -> DataType {
    match s.to_lowercase().as_str() {
        "boolean" | "bool" => DataType::Boolean,
        "int8" => DataType::Int8,
        "int16" => DataType::Int16,
        "int32" => DataType::Int32,
        "int64" | "int" => DataType::Int64,
        "uint8" => DataType::UInt8,
        "uint16" => DataType::UInt16,
        "uint32" => DataType::UInt32,
        "uint64" => DataType::UInt64,
        "float16" => DataType::Float16,
        "float32" => DataType::Float32,
        "float64" | "float" | "double" => DataType::Float64,
        "utf8" | "string" => DataType::Utf8,
        "largeutf8" | "largestring" => DataType::LargeUtf8,
        "binary" => DataType::Binary,
        "largebinary" => DataType::LargeBinary,
        "date32" | "date" => DataType::Date32,
        "date64" => DataType::Date64,
        _ if crate::data::times::TimeKind::of_type(s).is_some() => {
            use crate::data::times::{TimeKind, Unit, type_unit};
            let unit = match type_unit(s) {
                Unit::Second => TimeUnit::Second,
                Unit::Milli => TimeUnit::Millisecond,
                Unit::Micro => TimeUnit::Microsecond,
                Unit::Nano => TimeUnit::Nanosecond,
            };
            match (TimeKind::of_type(s), unit) {
                (Some(TimeKind::Time), TimeUnit::Second | TimeUnit::Millisecond) => {
                    DataType::Time32(unit)
                }
                (Some(TimeKind::Time), _) => DataType::Time64(unit),
                _ => DataType::Duration(unit),
            }
        }
        "time" => DataType::Time64(TimeUnit::Microsecond),
        "duration" => DataType::Duration(TimeUnit::Microsecond),
        lower if lower.starts_with("timestamp") => DataType::Timestamp(
            TimeUnit::Microsecond,
            timestamp_tz_from_type_name(s).map(Into::into),
        ),
        "datetime" => DataType::Timestamp(TimeUnit::Microsecond, None),
        _ => DataType::Utf8, // fallback: store as string
    }
}

/// Turn a displayed datetime string back into the epoch microseconds Arrow
/// stores, reading it on `tz`'s wall clock.
///
/// The inverse of [`arrow_instant_to_local`], and it has to stay the inverse:
/// the reader converts an instant into the column's zone, so the writer must
/// convert it back out, or every save would shift the column by the offset.
/// A local time that does not exist (the spring-forward hour) or happens twice
/// (the autumn one) takes the earliest reading, which is what every other
/// timezone path in Octa does.
fn local_string_to_micros(s: &str, tz: Option<&str>) -> Option<i64> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    let Some(tz) = tz.map(str::trim).filter(|t| !t.is_empty()) else {
        return Some(naive.and_utc().timestamp_micros());
    };
    if let Ok(zone) = tz.parse::<chrono_tz::Tz>() {
        return zone
            .from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.timestamp_micros());
    }
    if let Ok(offset) = tz.parse::<chrono::FixedOffset>() {
        return offset
            .from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.timestamp_micros());
    }
    Some(naive.and_utc().timestamp_micros())
}

/// Write a DataTable to a Parquet file.
fn write_parquet(path: &Path, table: &DataTable) -> Result<()> {
    write_parquet_with(
        path,
        table,
        &crate::formats::write_options::ParquetOptions::default(),
    )
}

/// Options-aware Parquet writer. `write_parquet` delegates here with the
/// defaults, so there is one writer rather than two that can drift.
pub fn write_parquet_with(
    path: &Path,
    table: &DataTable,
    opts: &crate::formats::write_options::ParquetOptions,
) -> Result<()> {
    // Written to a sibling temp and renamed over `path`, so a failure part-way
    // leaves the existing file untouched instead of truncated.
    crate::formats::write_slices(
        path,
        &crate::formats::SliceFormat::Parquet(opts.clone()),
        &table.columns,
        [Ok(table)],
    )
}

/// A Parquet file written one batch of rows at a time, for a table too big to
/// hold whole (the database export streams pages into it). The column types
/// are fixed by the columns it was created with, and every batch is written
/// against them, so a page in which a driver typed a column differently still
/// lands in the same column.
pub struct ParquetStreamWriter {
    writer: ArrowWriter<File>,
    schema: Arc<Schema>,
    types: Vec<DataType>,
}

impl ParquetStreamWriter {
    /// Start a file at `path` with `columns`. Nothing is atomic here: the
    /// caller writes to a temp and renames (see `formats::write_atomically`).
    pub fn create(
        path: &Path,
        columns: &[ColumnInfo],
        opts: &crate::formats::write_options::ParquetOptions,
    ) -> Result<Self> {
        use parquet::file::properties::{EnabledStatistics, WriterProperties};

        let types: Vec<DataType> = columns
            .iter()
            .map(|col| match data_type_from_string(&col.data_type) {
                DataType::Duration(_) if opts.duration_as_integer => DataType::Int64,
                other => other,
            })
            .collect();
        let fields: Vec<Field> = columns
            .iter()
            .zip(&types)
            .map(|(col, ty)| Field::new(&col.name, ty.clone(), true))
            .collect();
        let schema = Arc::new(Schema::new(fields));

        let mut builder = WriterProperties::builder()
            .set_compression(crate::formats::write_options::parquet_compression(
                &opts.compression,
            ))
            .set_dictionary_enabled(opts.dictionary)
            .set_statistics_enabled(if opts.statistics {
                EnabledStatistics::Chunk
            } else {
                EnabledStatistics::None
            });
        if let Some(n) = opts.row_group_size {
            builder = builder.set_max_row_group_row_count(Some(n.max(1)));
        }
        let writer =
            ArrowWriter::try_new(File::create(path)?, schema.clone(), Some(builder.build()))?;
        Ok(Self {
            writer,
            schema,
            types,
        })
    }

    /// Append every row of `batch`, which must have the columns the file was
    /// created with.
    pub fn write(&mut self, batch: &DataTable) -> Result<()> {
        if batch.col_count() != self.types.len() {
            return Err(anyhow!(
                "a batch has {} columns, the file {}",
                batch.col_count(),
                self.types.len()
            ));
        }
        let num_rows = batch.row_count();
        let arrays: Vec<Arc<dyn Array>> = self
            .types
            .iter()
            .enumerate()
            .map(|(col_idx, ty)| build_arrow_array(ty, batch, col_idx, num_rows))
            .collect();
        let batch = arrow::record_batch::RecordBatch::try_new(self.schema.clone(), arrays)?;
        self.writer.write(&batch)?;
        Ok(())
    }

    /// Write the footer. Without it the file is not a Parquet file.
    pub fn finish(self) -> Result<()> {
        self.writer.close()?;
        Ok(())
    }
}

/// Outcome of coercing one cell to `f64` for the float builders.
enum FloatCell {
    Value(f64),
    Null,
}

fn float_cell(table: &DataTable, row: usize, col_idx: usize) -> FloatCell {
    match table.get(row, col_idx) {
        Some(CellValue::Float(v)) => FloatCell::Value(*v),
        Some(CellValue::Int(v)) => FloatCell::Value(*v as f64),
        Some(CellValue::Null) | None => FloatCell::Null,
        Some(v) => match v.to_string().parse::<f64>() {
            Ok(f) => FloatCell::Value(f),
            Err(_) => FloatCell::Null,
        },
    }
}

/// Width-correct signed integer array: cells are read as `i64` (the
/// `CellValue::Int` payload) and narrowed with `try_from`; values that do not
/// fit the target width become null, mirroring the parse-failure convention.
fn signed_int_array<T>(table: &DataTable, col_idx: usize, num_rows: usize) -> Arc<dyn Array>
where
    T: ArrowPrimitiveType,
    T::Native: TryFrom<i64>,
{
    let mut builder = PrimitiveBuilder::<T>::with_capacity(num_rows);
    for row in 0..num_rows {
        let wide: Option<i64> = match table.get(row, col_idx) {
            Some(CellValue::Int(v)) => Some(*v),
            Some(CellValue::Float(v)) => Some(*v as i64),
            Some(CellValue::Duration(ns)) => Some(ns.div_euclid(1_000)),
            Some(CellValue::Null) | None => None,
            Some(v) => v.to_string().parse::<i64>().ok(),
        };
        match wide.and_then(|v| T::Native::try_from(v).ok()) {
            Some(v) => builder.append_value(v),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

/// Width-correct unsigned integer array; negative or oversized values become
/// null rather than wrapping.
fn unsigned_int_array<T>(table: &DataTable, col_idx: usize, num_rows: usize) -> Arc<dyn Array>
where
    T: ArrowPrimitiveType,
    T::Native: TryFrom<u64>,
{
    let mut builder = PrimitiveBuilder::<T>::with_capacity(num_rows);
    for row in 0..num_rows {
        let wide: Option<u64> = match table.get(row, col_idx) {
            Some(CellValue::Int(v)) => u64::try_from(*v).ok(),
            Some(CellValue::Float(v)) if *v >= 0.0 => Some(*v as u64),
            Some(CellValue::Null) | None => None,
            Some(v) => v.to_string().parse::<u64>().ok(),
        };
        match wide.and_then(|v| T::Native::try_from(v).ok()) {
            Some(v) => builder.append_value(v),
            None => builder.append_null(),
        }
    }
    Arc::new(builder.finish())
}

/// A time or duration column in `arrow_type`'s unit. Text cells are parsed
/// (an edit, or a value that arrived as text); anything that does not parse
/// is null, the convention of every other typed column here.
fn time_array(
    arrow_type: &DataType,
    table: &DataTable,
    col_idx: usize,
    num_rows: usize,
) -> Arc<dyn Array> {
    use crate::data::times::TimeKind;
    let per: i64 = match arrow_type {
        DataType::Time32(TimeUnit::Second) | DataType::Duration(TimeUnit::Second) => 1_000_000_000,
        DataType::Time32(_) | DataType::Duration(TimeUnit::Millisecond) => 1_000_000,
        DataType::Time64(TimeUnit::Microsecond) | DataType::Duration(TimeUnit::Microsecond) => {
            1_000
        }
        _ => 1,
    };
    let kind = if matches!(arrow_type, DataType::Duration(_)) {
        TimeKind::Duration
    } else {
        TimeKind::Time
    };
    let values: Vec<Option<i64>> = (0..num_rows)
        .map(|row| {
            let ns = match table.get(row, col_idx) {
                Some(CellValue::Time(ns) | CellValue::Duration(ns)) => Some(*ns),
                Some(CellValue::String(s)) => kind.parse(s),
                _ => None,
            };
            // ponytail: digits below the column's unit are cut (div_euclid),
            // as the docs say; a nanosecond column keeps them.
            ns.map(|v| v.div_euclid(per))
        })
        .collect();
    let narrow = |values: &[Option<i64>]| -> Vec<Option<i32>> {
        values.iter().map(|v| v.map(|x| x as i32)).collect()
    };
    match arrow_type {
        DataType::Time32(TimeUnit::Second) => Arc::new(Time32SecondArray::from(narrow(&values))),
        DataType::Time32(_) => Arc::new(Time32MillisecondArray::from(narrow(&values))),
        DataType::Time64(TimeUnit::Microsecond) => Arc::new(Time64MicrosecondArray::from(values)),
        DataType::Time64(_) => Arc::new(Time64NanosecondArray::from(values)),
        DataType::Duration(TimeUnit::Second) => Arc::new(DurationSecondArray::from(values)),
        DataType::Duration(TimeUnit::Millisecond) => {
            Arc::new(DurationMillisecondArray::from(values))
        }
        DataType::Duration(TimeUnit::Microsecond) => {
            Arc::new(DurationMicrosecondArray::from(values))
        }
        _ => Arc::new(DurationNanosecondArray::from(values)),
    }
}

/// Whether Save As to Parquet should say how durations will be written:
/// the table has a Duration column and they go out as Arrow durations.
pub fn duration_note_applies(
    table: &DataTable,
    opts: &crate::formats::write_options::ParquetOptions,
) -> bool {
    use crate::data::times::TimeKind;
    !opts.duration_as_integer
        && table
            .columns
            .iter()
            .any(|c| TimeKind::of_type(&c.data_type) == Some(TimeKind::Duration))
}

/// Build an Arrow array from a column of the DataTable.
///
/// Must cover every `DataType` that [`data_type_from_string`] can produce
/// with an array of exactly that type; see the invariant note there. The
/// final `Utf8` fallback is only reached for type names that fall back to
/// `Utf8` in `data_type_from_string` too, so schema and array always agree.
pub fn build_arrow_array(
    arrow_type: &DataType,
    table: &DataTable,
    col_idx: usize,
    num_rows: usize,
) -> Arc<dyn Array> {
    match arrow_type {
        DataType::Boolean => {
            let mut builder = BooleanBuilder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Bool(v)) => builder.append_value(*v),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => {
                        let s = v.to_string().to_lowercase();
                        match s.as_str() {
                            "true" | "1" | "yes" => builder.append_value(true),
                            "false" | "0" | "no" => builder.append_value(false),
                            _ => builder.append_null(),
                        }
                    }
                }
            }
            Arc::new(builder.finish())
        }
        // Signed integers get width-correct arrays so the array type always
        // matches the schema `write_parquet` builds from the same type name
        // (`RecordBatch::try_new` validates the two against each other).
        // Out-of-range values become null, mirroring the parse-failure
        // convention below.
        DataType::Int8 => signed_int_array::<Int8Type>(table, col_idx, num_rows),
        DataType::Int16 => signed_int_array::<Int16Type>(table, col_idx, num_rows),
        DataType::Int32 => signed_int_array::<Int32Type>(table, col_idx, num_rows),
        DataType::Int64 => signed_int_array::<Int64Type>(table, col_idx, num_rows),
        DataType::UInt8 => unsigned_int_array::<UInt8Type>(table, col_idx, num_rows),
        DataType::UInt16 => unsigned_int_array::<UInt16Type>(table, col_idx, num_rows),
        DataType::UInt32 => unsigned_int_array::<UInt32Type>(table, col_idx, num_rows),
        DataType::UInt64 => unsigned_int_array::<UInt64Type>(table, col_idx, num_rows),
        DataType::Time32(_) | DataType::Time64(_) | DataType::Duration(_) => {
            time_array(arrow_type, table, col_idx, num_rows)
        }
        DataType::Float16 => {
            let mut builder = Float16Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match float_cell(table, row, col_idx) {
                    FloatCell::Value(f) => builder.append_value(half::f16::from_f64(f)),
                    FloatCell::Null => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Float32 => {
            let mut builder = Float32Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match float_cell(table, row, col_idx) {
                    FloatCell::Value(f) => builder.append_value(f as f32),
                    FloatCell::Null => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match float_cell(table, row, col_idx) {
                    FloatCell::Value(f) => builder.append_value(f),
                    FloatCell::Null => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Date32 => {
            let mut builder = Date32Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Date(s)) => {
                        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                            let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                            let days = (d - epoch).num_days() as i32;
                            builder.append_value(days);
                        } else {
                            builder.append_null();
                        }
                    }
                    Some(CellValue::Null) | None => builder.append_null(),
                    _ => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Date64 => {
            let mut builder = Date64Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                let millis = match table.get(row, col_idx) {
                    Some(CellValue::Date(s)) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .ok()
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .map(|dt| dt.and_utc().timestamp_millis()),
                    Some(CellValue::DateTime(s)) => {
                        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f")
                            .or_else(|_| {
                                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                            })
                            .ok()
                            .map(|dt| dt.and_utc().timestamp_millis())
                    }
                    _ => None,
                };
                match millis {
                    Some(ms) => builder.append_value(ms),
                    None => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Timestamp(_, tz) => {
            let mut builder = TimestampMicrosecondBuilder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::DateTime(s)) => {
                        // The cell holds the wall clock of the column's own
                        // zone (that is what the reader wrote), so the instant
                        // has to be recovered on that clock. Reading it as UTC
                        // would shift every value by the offset on each save.
                        match local_string_to_micros(s, tz.as_deref()) {
                            Some(us) => builder.append_value(us),
                            None => builder.append_null(),
                        }
                    }
                    Some(CellValue::Null) | None => builder.append_null(),
                    _ => builder.append_null(),
                }
            }
            let array = builder.finish();
            match tz {
                Some(tz) => Arc::new(array.with_timezone(tz.to_string())),
                None => Arc::new(array),
            }
        }
        DataType::LargeUtf8 => {
            let mut builder = LargeStringBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string()),
                }
            }
            Arc::new(builder.finish())
        }
        // Binary columns round-trip their bytes exactly; non-binary cells in a
        // binary column (e.g. a hand-typed edit) are stored as their UTF-8.
        DataType::Binary => {
            let mut builder = BinaryBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Binary(b)) => builder.append_value(b),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string().as_bytes()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::LargeBinary => {
            let mut builder = LargeBinaryBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Binary(b)) => builder.append_value(b),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string().as_bytes()),
                }
            }
            Arc::new(builder.finish())
        }
        _ => {
            // Default: write as Utf8 string
            let mut builder = StringBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string()),
                }
            }
            Arc::new(builder.finish())
        }
    }
}
