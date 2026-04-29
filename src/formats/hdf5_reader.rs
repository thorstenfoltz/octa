use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

pub struct Hdf5Reader;

impl FormatReader for Hdf5Reader {
    fn name(&self) -> &str {
        "HDF5"
    }

    fn extensions(&self) -> &[&str] {
        &["h5", "hdf5", "hdf"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let file = hdf5_reader::Hdf5File::open(path)?;
        let root = file.root_group()?;

        let (groups, datasets) = root.members()?;

        // Strategy 1: If root has datasets directly, try the first one
        if !datasets.is_empty() {
            return read_first_dataset(path, &datasets);
        }

        // Strategy 2: If root has groups, try the first group's datasets
        for group in &groups {
            let (_, sub_datasets) = group.members()?;
            if !sub_datasets.is_empty() {
                return read_first_dataset(path, &sub_datasets);
            }
        }

        anyhow::bail!("No readable datasets found in HDF5 file")
    }
}

fn read_first_dataset(path: &Path, datasets: &[hdf5_reader::Dataset<'_>]) -> Result<DataTable> {
    use hdf5_reader::messages::datatype::Datatype;

    let dataset = &datasets[0];
    let dtype = dataset.dtype();
    let shape = dataset.shape();

    match dtype {
        // Compound dataset (like a pandas DataFrame stored in HDF5)
        Datatype::Compound { fields, .. } => read_compound_dataset(path, dataset, fields),
        // String dataset
        Datatype::String { .. } | Datatype::VarLen { .. } => {
            read_string_dataset(path, dataset, shape)
        }
        // Numeric 2D array
        Datatype::FixedPoint { size, signed, .. } => {
            read_numeric_dataset(path, dataset, shape, *size, *signed)
        }
        Datatype::FloatingPoint { size, .. } => read_float_dataset(path, dataset, shape, *size),
        _ => anyhow::bail!("Unsupported HDF5 dataset type: {:?}", dtype),
    }
}

fn read_compound_dataset(
    path: &Path,
    dataset: &hdf5_reader::Dataset<'_>,
    fields: &[hdf5_reader::messages::datatype::CompoundField],
) -> Result<DataTable> {
    use hdf5_reader::dtype_element_size;
    let columns: Vec<ColumnInfo> = fields
        .iter()
        .map(|f| ColumnInfo {
            name: f.name.clone(),
            data_type: hdf5_type_to_string(&f.datatype),
        })
        .collect();

    // Read every record into a flat Vec<u8> via a one-off H5Type wrapper,
    // then slice each field from its byte offset using the declared layout.
    let array = dataset.read_array::<CompoundRow>()?;
    let record_size = dtype_element_size(dataset.dtype());

    let mut rows = Vec::with_capacity(array.len());
    for row_bytes in array.iter() {
        let bytes = &row_bytes.0;
        let mut row: Vec<CellValue> = Vec::with_capacity(fields.len());
        for field in fields {
            let offset = field.byte_offset as usize;
            let size = dtype_element_size(&field.datatype);
            let slice = if offset + size <= bytes.len() {
                &bytes[offset..offset + size]
            } else {
                // Truncated record (shouldn't happen for well-formed files).
                &[]
            };
            row.push(decode_compound_field(slice, &field.datatype));
        }
        debug_assert_eq!(record_size, bytes.len());
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("HDF5".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

/// Wraps a single compound record as an opaque byte buffer so we can use
/// `Dataset::read_array::<CompoundRow>()` to traverse the layout once and
/// then slice each declared field out of the bytes ourselves.
#[derive(Clone)]
struct CompoundRow(Vec<u8>);

impl hdf5_reader::H5Type for CompoundRow {
    fn hdf5_type() -> hdf5_reader::Datatype {
        // Placeholder — `read_array` never compares this against the
        // dataset's datatype; it only uses our `from_bytes`/`element_size`.
        hdf5_reader::Datatype::Opaque {
            size: 0,
            tag: String::new(),
        }
    }

    fn from_bytes(bytes: &[u8], dtype: &hdf5_reader::Datatype) -> hdf5_reader::error::Result<Self> {
        let n = hdf5_reader::dtype_element_size(dtype);
        let mut buf = vec![0u8; n];
        let take = bytes.len().min(n);
        buf[..take].copy_from_slice(&bytes[..take]);
        Ok(CompoundRow(buf))
    }

    fn element_size(dtype: &hdf5_reader::Datatype) -> usize {
        hdf5_reader::dtype_element_size(dtype)
    }
}

fn decode_compound_field(bytes: &[u8], dtype: &hdf5_reader::Datatype) -> CellValue {
    use hdf5_reader::ByteOrder;
    use hdf5_reader::Datatype;
    use hdf5_reader::messages::datatype::StringSize;

    let little_endian = |bo: ByteOrder| matches!(bo, ByteOrder::LittleEndian);

    match dtype {
        Datatype::FixedPoint {
            size,
            signed,
            byte_order,
        } => {
            if bytes.len() < *size as usize {
                return CellValue::Null;
            }
            let buf = &bytes[..*size as usize];
            macro_rules! read_int {
                ($t:ty) => {{
                    let mut arr = [0u8; std::mem::size_of::<$t>()];
                    arr.copy_from_slice(buf);
                    if little_endian(*byte_order) {
                        <$t>::from_le_bytes(arr) as i64
                    } else {
                        <$t>::from_be_bytes(arr) as i64
                    }
                }};
            }
            let v = match (size, signed) {
                (1, true) => read_int!(i8),
                (1, false) => read_int!(u8),
                (2, true) => read_int!(i16),
                (2, false) => read_int!(u16),
                (4, true) => read_int!(i32),
                (4, false) => read_int!(u32),
                (8, true) => read_int!(i64),
                (8, false) => {
                    // u64 truncates above i64::MAX into negative i64; keep as f64 in that case.
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(buf);
                    let raw = if little_endian(*byte_order) {
                        u64::from_le_bytes(arr)
                    } else {
                        u64::from_be_bytes(arr)
                    };
                    if raw <= i64::MAX as u64 {
                        return CellValue::Int(raw as i64);
                    } else {
                        return CellValue::Float(raw as f64);
                    }
                }
                _ => return CellValue::Null,
            };
            CellValue::Int(v)
        }
        Datatype::FloatingPoint { size, byte_order } => {
            if bytes.len() < *size as usize {
                return CellValue::Null;
            }
            match size {
                4 => {
                    let mut arr = [0u8; 4];
                    arr.copy_from_slice(&bytes[..4]);
                    let v = if little_endian(*byte_order) {
                        f32::from_le_bytes(arr)
                    } else {
                        f32::from_be_bytes(arr)
                    };
                    CellValue::Float(v as f64)
                }
                8 => {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&bytes[..8]);
                    let v = if little_endian(*byte_order) {
                        f64::from_le_bytes(arr)
                    } else {
                        f64::from_be_bytes(arr)
                    };
                    CellValue::Float(v)
                }
                _ => CellValue::Null,
            }
        }
        Datatype::String {
            size: StringSize::Fixed(n),
            ..
        } => {
            let take = bytes.len().min(*n as usize);
            let raw = &bytes[..take];
            // Trim trailing null and space padding.
            let trimmed_end = raw
                .iter()
                .rposition(|b| *b != 0 && *b != b' ')
                .map(|i| i + 1)
                .unwrap_or(0);
            let s = String::from_utf8_lossy(&raw[..trimmed_end]).into_owned();
            CellValue::String(s)
        }
        // Variable-length strings inside a compound record contain a heap
        // pointer that requires global-heap traversal — out of scope for
        // the embedded compound decoder. Fall back to a placeholder so the
        // user knows the cell exists but isn't decoded.
        Datatype::String {
            size: StringSize::Variable,
            ..
        }
        | Datatype::VarLen { .. } => CellValue::String("(varlen)".to_string()),
        _ => CellValue::Null,
    }
}

fn read_string_dataset(
    path: &Path,
    dataset: &hdf5_reader::Dataset<'_>,
    shape: &[u64],
) -> Result<DataTable> {
    let strings = dataset.read_strings()?;
    let ncols = if shape.len() >= 2 {
        shape[1] as usize
    } else {
        1
    };
    let nrows = if shape.is_empty() {
        strings.len()
    } else {
        shape[0] as usize
    };

    let columns: Vec<ColumnInfo> = (0..ncols)
        .map(|i| ColumnInfo {
            name: if ncols == 1 {
                "value".to_string()
            } else {
                format!("col_{i}")
            },
            data_type: "Utf8".to_string(),
        })
        .collect();

    let mut rows = Vec::with_capacity(nrows);
    for row_idx in 0..nrows {
        let mut row = Vec::with_capacity(ncols);
        for col_idx in 0..ncols {
            let flat_idx = row_idx * ncols + col_idx;
            if flat_idx < strings.len() {
                row.push(CellValue::String(strings[flat_idx].clone()));
            } else {
                row.push(CellValue::Null);
            }
        }
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("HDF5".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

fn read_numeric_dataset(
    path: &Path,
    dataset: &hdf5_reader::Dataset<'_>,
    shape: &[u64],
    size: u8,
    signed: bool,
) -> Result<DataTable> {
    let ncols = if shape.len() >= 2 {
        shape[1] as usize
    } else {
        1
    };
    let nrows = shape.first().copied().unwrap_or(1) as usize;

    let columns: Vec<ColumnInfo> = (0..ncols)
        .map(|i| ColumnInfo {
            name: if ncols == 1 {
                "value".to_string()
            } else {
                format!("col_{i}")
            },
            data_type: match (size, signed) {
                (1, true) => "Int8".to_string(),
                (1, false) => "UInt8".to_string(),
                (2, true) => "Int16".to_string(),
                (2, false) => "UInt16".to_string(),
                (4, true) => "Int32".to_string(),
                (4, false) => "UInt32".to_string(),
                (8, true) => "Int64".to_string(),
                (8, false) => "UInt64".to_string(),
                _ => "Int64".to_string(),
            },
        })
        .collect();

    // Read based on size and signedness
    let mut rows = Vec::with_capacity(nrows);
    match (size, signed) {
        (4, true) => {
            let array = dataset.read_array::<i32>()?;
            for row_idx in 0..nrows {
                let mut row = Vec::with_capacity(ncols);
                for col_idx in 0..ncols {
                    let idx: Vec<usize> = if ncols == 1 {
                        vec![row_idx]
                    } else {
                        vec![row_idx, col_idx]
                    };
                    let val = array.get(idx.as_slice()).copied().unwrap_or(0);
                    row.push(CellValue::Int(val as i64));
                }
                rows.push(row);
            }
        }
        (8, true) => {
            let array = dataset.read_array::<i64>()?;
            for row_idx in 0..nrows {
                let mut row = Vec::with_capacity(ncols);
                for col_idx in 0..ncols {
                    let idx: Vec<usize> = if ncols == 1 {
                        vec![row_idx]
                    } else {
                        vec![row_idx, col_idx]
                    };
                    let val = array.get(idx.as_slice()).copied().unwrap_or(0);
                    row.push(CellValue::Int(val));
                }
                rows.push(row);
            }
        }
        (1, false) => {
            let array = dataset.read_array::<u8>()?;
            for row_idx in 0..nrows {
                let mut row = Vec::with_capacity(ncols);
                for col_idx in 0..ncols {
                    let idx: Vec<usize> = if ncols == 1 {
                        vec![row_idx]
                    } else {
                        vec![row_idx, col_idx]
                    };
                    let val = array.get(idx.as_slice()).copied().unwrap_or(0);
                    row.push(CellValue::Int(val as i64));
                }
                rows.push(row);
            }
        }
        (2, true) => {
            let array = dataset.read_array::<i16>()?;
            for row_idx in 0..nrows {
                let mut row = Vec::with_capacity(ncols);
                for col_idx in 0..ncols {
                    let idx: Vec<usize> = if ncols == 1 {
                        vec![row_idx]
                    } else {
                        vec![row_idx, col_idx]
                    };
                    let val = array.get(idx.as_slice()).copied().unwrap_or(0);
                    row.push(CellValue::Int(val as i64));
                }
                rows.push(row);
            }
        }
        _ => {
            // Fallback: read as i64
            let array = dataset.read_array::<i64>()?;
            for row_idx in 0..nrows {
                let mut row = Vec::with_capacity(ncols);
                for col_idx in 0..ncols {
                    let idx: Vec<usize> = if ncols == 1 {
                        vec![row_idx]
                    } else {
                        vec![row_idx, col_idx]
                    };
                    let val = array.get(idx.as_slice()).copied().unwrap_or(0);
                    row.push(CellValue::Int(val));
                }
                rows.push(row);
            }
        }
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("HDF5".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

fn read_float_dataset(
    path: &Path,
    dataset: &hdf5_reader::Dataset<'_>,
    shape: &[u64],
    size: u8,
) -> Result<DataTable> {
    let ncols = if shape.len() >= 2 {
        shape[1] as usize
    } else {
        1
    };
    let nrows = shape.first().copied().unwrap_or(1) as usize;

    let columns: Vec<ColumnInfo> = (0..ncols)
        .map(|i| ColumnInfo {
            name: if ncols == 1 {
                "value".to_string()
            } else {
                format!("col_{i}")
            },
            data_type: if size == 4 {
                "Float32".to_string()
            } else {
                "Float64".to_string()
            },
        })
        .collect();

    let mut rows = Vec::with_capacity(nrows);
    if size == 4 {
        let array = dataset.read_array::<f32>()?;
        for row_idx in 0..nrows {
            let mut row = Vec::with_capacity(ncols);
            for col_idx in 0..ncols {
                let idx: Vec<usize> = if ncols == 1 {
                    vec![row_idx]
                } else {
                    vec![row_idx, col_idx]
                };
                let val = array.get(idx.as_slice()).copied().unwrap_or(0.0);
                row.push(CellValue::Float(val as f64));
            }
            rows.push(row);
        }
    } else {
        let array = dataset.read_array::<f64>()?;
        for row_idx in 0..nrows {
            let mut row = Vec::with_capacity(ncols);
            for col_idx in 0..ncols {
                let idx: Vec<usize> = if ncols == 1 {
                    vec![row_idx]
                } else {
                    vec![row_idx, col_idx]
                };
                let val = array.get(idx.as_slice()).copied().unwrap_or(0.0);
                row.push(CellValue::Float(val));
            }
            rows.push(row);
        }
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("HDF5".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

fn hdf5_type_to_string(dt: &hdf5_reader::messages::datatype::Datatype) -> String {
    use hdf5_reader::messages::datatype::Datatype;
    match dt {
        Datatype::FixedPoint {
            size, signed: true, ..
        } => match size {
            1 => "Int8".to_string(),
            2 => "Int16".to_string(),
            4 => "Int32".to_string(),
            8 => "Int64".to_string(),
            _ => "Int64".to_string(),
        },
        Datatype::FixedPoint {
            size,
            signed: false,
            ..
        } => match size {
            1 => "UInt8".to_string(),
            2 => "UInt16".to_string(),
            4 => "UInt32".to_string(),
            8 => "UInt64".to_string(),
            _ => "UInt64".to_string(),
        },
        Datatype::FloatingPoint { size, .. } => match size {
            4 => "Float32".to_string(),
            8 => "Float64".to_string(),
            _ => "Float64".to_string(),
        },
        Datatype::String { .. } | Datatype::VarLen { .. } => "Utf8".to_string(),
        _ => "Utf8".to_string(),
    }
}
