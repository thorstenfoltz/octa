//! Read R datasets (`.rds`, `.rdata`, `.rda`) — tabular subset only.
//!
//! Scope: `.rds` files whose root SEXP is a `data.frame` or `tibble`. Common
//! column types are mapped: logical, integer, double, character, factor,
//! `Date`, `POSIXct`. `NA` becomes `CellValue::Null`. Non-tabular `.rds`
//! files (a single vector, a fitted model, an S4 object, etc.) are
//! rejected with a clear error rather than silently rendered as a single
//! column.
//!
//! `.rdata` / `.rda` workspaces are not yet wired up — the rds2rust parser
//! handles only the `X\n` magic of single-object RDS files, and a workspace
//! adds a different `RDX2\n` envelope around a Pairlist of named bindings.
//! Until that wrapper is implemented, opening one of those files returns
//! a clear error pointing the user at `saveRDS()` to convert.

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result, anyhow};
use chrono::DateTime;
use rds2rust::{Attributes, DataFrameData, FactorData, Logical, RObject, VectorData};
use std::path::Path;
use std::sync::Arc;

pub struct RdsReader;

impl FormatReader for RdsReader {
    fn name(&self) -> &str {
        "R Dataset"
    }

    fn extensions(&self) -> &[&str] {
        &["rds", "rdata", "rda"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "rdata" || ext == "rda" {
            return Err(anyhow!(
                "RData workspace files are not yet supported. \
                 In R, run `saveRDS(obj, 'file.rds')` to convert a single \
                 object, then open the .rds file."
            ));
        }

        let parsed = rds2rust::read_rds_from_path(path)
            .with_context(|| format!("opening RDS file {}", path.display()))?;
        let object = parsed.object.into_concrete_deep();

        let df = match object {
            RObject::DataFrame(df) => *df,
            other => {
                return Err(anyhow!(
                    "RDS file does not contain a data.frame (got {}). \
                     Octa's RDS reader only handles tabular objects (data.frame, tibble).",
                    other.variant_name()
                ));
            }
        };

        dataframe_to_table(path, df)
    }
}

fn dataframe_to_table(path: &Path, df: DataFrameData) -> Result<DataTable> {
    let DataFrameData {
        columns,
        row_names: _,
    } = df;

    let n_cols = columns.len();
    let mut col_infos: Vec<ColumnInfo> = Vec::with_capacity(n_cols);
    let mut col_cells: Vec<Vec<CellValue>> = Vec::with_capacity(n_cols);
    let mut max_rows = 0usize;

    for (name, col) in columns.into_iter() {
        let (data_type, cells) = column_to_cells(&col)?;
        max_rows = max_rows.max(cells.len());
        col_infos.push(ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
        });
        col_cells.push(cells);
    }

    // Pad short columns with Null so all columns have the same length —
    // an RDS data.frame should already have equal-length columns, but be
    // defensive against malformed input.
    for cells in col_cells.iter_mut() {
        if cells.len() < max_rows {
            cells.resize(max_rows, CellValue::Null);
        }
    }

    // Transpose column-major Vec<Vec<CellValue>> into row-major.
    let rows: Vec<Vec<CellValue>> = (0..max_rows)
        .map(|r| col_cells.iter().map(|col| col[r].clone()).collect())
        .collect();

    Ok(DataTable {
        columns: col_infos,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("R Dataset".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

/// Returns `(data_type_string, cells)` for one column. `data_type_string`
/// uses the same Arrow-style names (`Int64`, `Float64`, `Boolean`, `Utf8`,
/// `Date`, `DateTime`) the rest of the codebase agrees on.
fn column_to_cells(obj: &RObject) -> Result<(&'static str, Vec<CellValue>)> {
    match obj {
        RObject::Integer(v) => Ok(("Int64", integer_cells(v))),
        RObject::Real(v) => Ok(("Float64", real_cells(v))),
        RObject::Logical(v) => Ok(("Boolean", logical_cells(v))),
        RObject::Character(v) => Ok(("Utf8", character_cells(v))),
        RObject::Factor(f) => Ok(("Utf8", factor_cells(f))),
        RObject::WithAttributes { object, attributes } => {
            decode_with_attributes(object, attributes)
        }
        // Lists, complex types, etc. — render as a JSON-ish placeholder so
        // the row count stays correct but the user sees something for the
        // cell rather than a silent empty.
        other => {
            let n = vector_len(other);
            let placeholder = format!("({} not supported)", other.variant_name());
            Ok(("Utf8", vec![CellValue::Nested(placeholder); n]))
        }
    }
}

fn integer_cells(v: &VectorData<i32>) -> Vec<CellValue> {
    if !v.is_loaded() {
        return Vec::new();
    }
    v.as_vec()
        .iter()
        .map(|i| {
            if RObject::is_na_integer(*i) {
                CellValue::Null
            } else {
                CellValue::Int(i64::from(*i))
            }
        })
        .collect()
}

fn real_cells(v: &VectorData<f64>) -> Vec<CellValue> {
    if !v.is_loaded() {
        return Vec::new();
    }
    v.as_vec()
        .iter()
        .map(|f| {
            if f.is_nan() {
                // R's NA_real_ is encoded as a NaN with a specific bit
                // pattern. We treat all NaN as Null since the user-visible
                // semantics ("missing value") match.
                CellValue::Null
            } else {
                CellValue::Float(*f)
            }
        })
        .collect()
}

fn logical_cells(v: &VectorData<Logical>) -> Vec<CellValue> {
    if !v.is_loaded() {
        return Vec::new();
    }
    v.as_vec()
        .iter()
        .map(|l| match l {
            Logical::True => CellValue::Bool(true),
            Logical::False => CellValue::Bool(false),
            Logical::Na => CellValue::Null,
        })
        .collect()
}

fn character_cells(v: &VectorData<Arc<str>>) -> Vec<CellValue> {
    if !v.is_loaded() {
        return Vec::new();
    }
    // rds2rust 0.1 represents NA_character_ as the literal string "NA";
    // we cannot disambiguate it from a real "NA" value here, so we keep
    // it as a string. (Documented limitation.)
    v.as_vec()
        .iter()
        .map(|s| CellValue::String(s.to_string()))
        .collect()
}

fn factor_cells(f: &FactorData) -> Vec<CellValue> {
    f.values
        .iter()
        .map(|&idx| {
            // Factor values are 1-based; 0 (or NA_INTEGER) means NA.
            if idx <= 0 || RObject::is_na_integer(idx) {
                CellValue::Null
            } else {
                let level = f.levels.get((idx - 1) as usize);
                match level {
                    Some(s) => CellValue::String(s.to_string()),
                    None => CellValue::Null,
                }
            }
        })
        .collect()
}

fn decode_with_attributes(
    object: &RObject,
    attributes: &Attributes,
) -> Result<(&'static str, Vec<CellValue>)> {
    let class = read_class(attributes);

    if class.iter().any(|c| c == "Date") {
        return Ok(("Date", date_cells(object)));
    }
    if class.iter().any(|c| c == "POSIXct") {
        return Ok(("DateTime", posixct_cells(object)));
    }

    // Unrecognized class — fall through to the underlying type.
    column_to_cells(object)
}

fn read_class(attributes: &Attributes) -> Vec<String> {
    let Some(class_obj) = attributes.get("class") else {
        return Vec::new();
    };
    match class_obj.as_concrete() {
        RObject::Character(v) if v.is_loaded() => {
            v.as_vec().iter().map(|s| s.to_string()).collect()
        }
        _ => Vec::new(),
    }
}

fn date_cells(object: &RObject) -> Vec<CellValue> {
    let real_days = match object {
        RObject::Real(v) if v.is_loaded() => v.as_vec().clone(),
        RObject::Integer(v) if v.is_loaded() => v.as_vec().iter().map(|i| f64::from(*i)).collect(),
        _ => return Vec::new(),
    };
    real_days
        .iter()
        .map(|days| {
            if days.is_nan() {
                return CellValue::Null;
            }
            // R's Date is stored as days since 1970-01-01, can be fractional
            // for sub-day precision (rare). Truncate to day.
            let secs = (days * 86_400.0) as i64;
            DateTime::from_timestamp(secs, 0)
                .map(|dt| CellValue::Date(dt.format("%Y-%m-%d").to_string()))
                .unwrap_or(CellValue::Null)
        })
        .collect()
}

fn posixct_cells(object: &RObject) -> Vec<CellValue> {
    let secs = match object {
        RObject::Real(v) if v.is_loaded() => v.as_vec().clone(),
        RObject::Integer(v) if v.is_loaded() => v.as_vec().iter().map(|i| f64::from(*i)).collect(),
        _ => return Vec::new(),
    };
    secs.iter()
        .map(|s| {
            if s.is_nan() {
                return CellValue::Null;
            }
            let whole = s.trunc() as i64;
            let nanos = ((s.fract().abs()) * 1_000_000_000.0) as u32;
            DateTime::from_timestamp(whole, nanos)
                .map(|dt| CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
                .unwrap_or(CellValue::Null)
        })
        .collect()
}

fn vector_len(obj: &RObject) -> usize {
    match obj {
        RObject::Integer(v) => v.len(),
        RObject::Real(v) => v.len(),
        RObject::Logical(v) => v.len(),
        RObject::Character(v) => v.len(),
        RObject::Raw(v) => v.len(),
        RObject::Complex(v) => v.len(),
        RObject::List(v) => v.len(),
        RObject::Factor(f) => f.values.len(),
        _ => 0,
    }
}
