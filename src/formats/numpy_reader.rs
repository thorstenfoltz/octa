//! NumPy array reader (read-only).
//!
//! - `.npy` holds a single array. A 1-D array becomes one `value` column; a
//!   2-D array becomes one column per column index (`col_0`, `col_1`, ...).
//!   Higher dimensions are flattened past the first two (each row keeps the
//!   product of the trailing dimensions as its columns).
//! - `.npz` is a zip archive of named `.npy` entries (what `numpy.savez`
//!   writes), so it is exposed as a multi-table source: one table per array.
//!
//! The `.npy` payloads are parsed by `npyz`; `.npz` archives are unzipped with
//! the project's existing `zip` crate (so we don't pull a second copy in via
//! `npyz`'s `npz` feature).

use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use npyz::{DType, NpyFile, TypeChar};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::{FormatReader, TableInfo};

pub struct NumpyReader;

impl FormatReader for NumpyReader {
    fn name(&self) -> &str {
        "NumPy"
    }

    fn extensions(&self) -> &[&str] {
        &["npy", "npz"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        if is_npz(path) {
            // A bare open of a multi-array .npz shows its first array; the rest
            // are reachable through the table picker (list_tables/read_table).
            let names = npz_array_names(path)?;
            let first = names
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("the .npz archive contains no arrays"))?;
            self.read_table(path, &first)
        } else {
            let bytes = std::fs::read(path)
                .with_context(|| format!("reading NumPy file {}", path.display()))?;
            let table = npy_bytes_to_table(&bytes)?;
            Ok(finish(table, path))
        }
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        if !is_npz(path) {
            return Ok(None);
        }
        let names = npz_array_names(path)?;
        let infos = names
            .into_iter()
            .map(|name| TableInfo {
                name,
                schema: None,
                columns: Vec::new(),
                row_count: None,
            })
            .collect();
        Ok(Some(infos))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        if !is_npz(path) {
            return self.read_file(path);
        }
        let bytes = npz_entry_bytes(path, table)?;
        let dt = npy_bytes_to_table(&bytes)?;
        Ok(finish(dt, path))
    }
}

fn is_npz(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("npz"))
}

/// List the array names inside a `.npz` (the zip entry names with the trailing
/// `.npy` stripped, matching how `numpy.savez` stores them).
fn npz_array_names(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening .npz archive {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).context("reading .npz archive")?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).context("reading .npz entry")?;
        let raw = entry.name().to_string();
        let name = raw.strip_suffix(".npy").unwrap_or(&raw).to_string();
        names.push(name);
    }
    Ok(names)
}

/// Read the raw `.npy` bytes of one array out of a `.npz` archive. Accepts the
/// array name with or without the `.npy` suffix.
fn npz_entry_bytes(path: &Path, array: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening .npz archive {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).context("reading .npz archive")?;
    let entry_name = if array.ends_with(".npy") {
        array.to_string()
    } else {
        format!("{array}.npy")
    };
    let mut entry = archive
        .by_name(&entry_name)
        .with_context(|| format!("no array '{array}' in .npz archive"))?;
    let mut out = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut out).context("reading .npz entry")?;
    Ok(out)
}

/// Parse a `.npy` byte payload into a `DataTable`. The leading axis is the row
/// axis; any trailing axes are flattened into columns.
fn npy_bytes_to_table(bytes: &[u8]) -> Result<DataTable> {
    let npy = NpyFile::new(bytes).context("parsing .npy header")?;
    let shape: Vec<u64> = npy.shape().to_vec();
    let dtype = npy.dtype();

    let (rows, cols) = match shape.as_slice() {
        [] => (1, 1),            // 0-D scalar -> single cell
        [n] => (*n as usize, 1), // 1-D -> column vector
        [r, rest @ ..] => (*r as usize, rest.iter().product::<u64>() as usize),
    };

    let flat = read_cells(npy, &dtype)?;
    // A row-major (C-order) array fills column-by-column within each row.
    let mut data_rows: Vec<Vec<CellValue>> = Vec::with_capacity(rows);
    for r in 0..rows {
        let start = r * cols;
        let row: Vec<CellValue> = (0..cols)
            .map(|c| flat.get(start + c).cloned().unwrap_or(CellValue::Null))
            .collect();
        data_rows.push(row);
    }

    let type_name = arrow_type_name(&dtype);
    let columns: Vec<ColumnInfo> = if cols == 1 {
        vec![ColumnInfo {
            name: "value".to_string(),
            data_type: type_name.to_string(),
        }]
    } else {
        (0..cols)
            .map(|c| ColumnInfo {
                name: format!("col_{c}"),
                data_type: type_name.to_string(),
            })
            .collect()
    };

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = data_rows;
    Ok(table)
}

/// Read every element of the array into a flat `Vec<CellValue>`, dispatching on
/// the dtype's type char + byte width.
fn read_cells(npy: NpyFile<&[u8]>, dtype: &DType) -> Result<Vec<CellValue>> {
    let DType::Plain(ts) = dtype else {
        bail!("structured / record NumPy arrays are not supported");
    };
    let size = ts.num_bytes().unwrap_or(0);
    Ok(match (ts.type_char(), size) {
        (TypeChar::Bool, _) => npy
            .into_vec::<i8>()?
            .into_iter()
            .map(|v| CellValue::Bool(v != 0))
            .collect(),
        (TypeChar::Int, 1) => int_cells(npy.into_vec::<i8>()?),
        (TypeChar::Int, 2) => int_cells(npy.into_vec::<i16>()?),
        (TypeChar::Int, 4) => int_cells(npy.into_vec::<i32>()?),
        (TypeChar::Int, _) => npy
            .into_vec::<i64>()?
            .into_iter()
            .map(CellValue::Int)
            .collect(),
        (TypeChar::Uint, 1) => uint_cells(npy.into_vec::<u8>()?),
        (TypeChar::Uint, 2) => uint_cells(npy.into_vec::<u16>()?),
        (TypeChar::Uint, 4) => uint_cells(npy.into_vec::<u32>()?),
        (TypeChar::Uint, _) => npy
            .into_vec::<u64>()?
            .into_iter()
            // u64 above i64::MAX can't be an Int; keep it as a float.
            .map(|v| match i64::try_from(v) {
                Ok(i) => CellValue::Int(i),
                Err(_) => CellValue::Float(v as f64),
            })
            .collect(),
        (TypeChar::Float, 4) => npy
            .into_vec::<f32>()?
            .into_iter()
            .map(|v| CellValue::Float(f64::from(v)))
            .collect(),
        (TypeChar::Float, _) => npy
            .into_vec::<f64>()?
            .into_iter()
            .map(CellValue::Float)
            .collect(),
        (TypeChar::ByteStr, _) => npy
            .into_vec::<Vec<u8>>()?
            .into_iter()
            .map(|b| CellValue::String(String::from_utf8_lossy(&b).trim_end().to_string()))
            .collect(),
        (TypeChar::UnicodeStr, _) => npy
            .into_vec::<String>()?
            .into_iter()
            .map(CellValue::String)
            .collect(),
        (other, _) => bail!("unsupported NumPy dtype '{:?}' ({})", other, dtype.descr()),
    })
}

fn int_cells<T: Into<i64>>(v: Vec<T>) -> Vec<CellValue> {
    v.into_iter().map(|x| CellValue::Int(x.into())).collect()
}

fn uint_cells<T: Into<i64>>(v: Vec<T>) -> Vec<CellValue> {
    v.into_iter().map(|x| CellValue::Int(x.into())).collect()
}

/// Map a NumPy dtype onto one of our Arrow type-name strings.
fn arrow_type_name(dtype: &DType) -> &'static str {
    let DType::Plain(ts) = dtype else {
        return "Utf8";
    };
    let size = ts.num_bytes().unwrap_or(0);
    match (ts.type_char(), size) {
        (TypeChar::Bool, _) => "Boolean",
        (TypeChar::Int, 1) => "Int8",
        (TypeChar::Int, 2) => "Int16",
        (TypeChar::Int, 4) => "Int32",
        (TypeChar::Int, _) => "Int64",
        (TypeChar::Uint, 1) => "UInt8",
        (TypeChar::Uint, 2) => "UInt16",
        (TypeChar::Uint, 4) => "UInt32",
        (TypeChar::Uint, _) => "Int64",
        (TypeChar::Float, 4) => "Float32",
        (TypeChar::Float, _) => "Float64",
        _ => "Utf8",
    }
}

fn finish(mut table: DataTable, path: &Path) -> DataTable {
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("NumPy".to_string());
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-build a minimal little-endian `.npy` for a 1-D f8 array.
    fn npy_1d_f64(values: &[f64]) -> Vec<u8> {
        let header = format!(
            "{{'descr': '<f8', 'fortran_order': False, 'shape': ({},), }}",
            values.len()
        );
        build_npy(&header, |out| {
            for v in values {
                out.extend_from_slice(&v.to_le_bytes());
            }
        })
    }

    /// Hand-build a 2x2 int32 `.npy`.
    fn npy_2x2_i32(values: &[i32; 4]) -> Vec<u8> {
        let header = "{'descr': '<i4', 'fortran_order': False, 'shape': (2, 2), }".to_string();
        build_npy(&header, |out| {
            for v in values {
                out.extend_from_slice(&v.to_le_bytes());
            }
        })
    }

    fn build_npy(header: &str, body: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        // Magic + version 1.0, then a 2-byte header length, padded so the data
        // starts on a 64-byte boundary (the .npy spec).
        let mut head = header.as_bytes().to_vec();
        let prefix = 10; // magic(6) + version(2) + len(2)
        let unpadded = prefix + head.len() + 1; // +1 for the trailing newline
        let pad = (64 - (unpadded % 64)) % 64;
        head.extend(std::iter::repeat_n(b' ', pad));
        head.push(b'\n');

        let mut out = Vec::new();
        out.extend_from_slice(b"\x93NUMPY");
        out.push(1);
        out.push(0);
        out.extend_from_slice(&(head.len() as u16).to_le_bytes());
        out.extend_from_slice(&head);
        body(&mut out);
        out
    }

    #[test]
    fn reads_1d_float_array() {
        let bytes = npy_1d_f64(&[1.5, 2.5, 3.5]);
        let table = npy_bytes_to_table(&bytes).expect("parse 1-D npy");
        assert_eq!(table.columns.len(), 1);
        assert_eq!(table.columns[0].name, "value");
        assert_eq!(table.columns[0].data_type, "Float64");
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.get(0, 0), Some(&CellValue::Float(1.5)));
        assert_eq!(table.get(2, 0), Some(&CellValue::Float(3.5)));
    }

    #[test]
    fn reads_2d_int_array_into_columns() {
        let bytes = npy_2x2_i32(&[1, 2, 3, 4]);
        let table = npy_bytes_to_table(&bytes).expect("parse 2-D npy");
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.columns[0].name, "col_0");
        assert_eq!(table.columns[1].name, "col_1");
        assert_eq!(table.columns[0].data_type, "Int32");
        assert_eq!(table.row_count(), 2);
        // Row-major: [[1, 2], [3, 4]].
        assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));
        assert_eq!(table.get(0, 1), Some(&CellValue::Int(2)));
        assert_eq!(table.get(1, 0), Some(&CellValue::Int(3)));
        assert_eq!(table.get(1, 1), Some(&CellValue::Int(4)));
    }
}
