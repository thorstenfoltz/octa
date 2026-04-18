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
    let columns: Vec<ColumnInfo> = fields
        .iter()
        .map(|f| ColumnInfo {
            name: f.name.clone(),
            data_type: hdf5_type_to_string(&f.datatype),
        })
        .collect();

    let num_rows = dataset.shape().first().copied().unwrap_or(1) as usize;

    // For compound datasets, we need to read the raw bytes and parse per-field
    // This is complex, so we try reading individual field datasets if available
    // or fall back to a simple representation
    let mut rows = Vec::with_capacity(num_rows);

    // Try to read each field as a separate typed column
    let mut col_data: Vec<Vec<CellValue>> = Vec::new();
    for field in fields {
        let field_values = read_compound_field_data(dataset, field, num_rows);
        col_data.push(field_values);
    }

    for row_idx in 0..num_rows {
        let row: Vec<CellValue> = col_data.iter().map(|col| col[row_idx].clone()).collect();
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

fn read_compound_field_data(
    _dataset: &hdf5_reader::Dataset<'_>,
    _field: &hdf5_reader::messages::datatype::CompoundField,
    num_rows: usize,
) -> Vec<CellValue> {
    // Compound field reading requires parsing raw bytes at offsets.
    // For now return placeholder - compound datasets are complex.
    vec![CellValue::String("(compound)".to_string()); num_rows]
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
