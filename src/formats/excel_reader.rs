use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use calamine::{Data, Reader, open_workbook_auto};
use rust_xlsxwriter::Workbook;
use std::path::Path;

pub struct ExcelReader;

impl FormatReader for ExcelReader {
    fn name(&self) -> &str {
        "Excel"
    }

    fn extensions(&self) -> &[&str] {
        &["xlsx", "xls", "xlsm", "xlsb", "xlm", "ods"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut workbook = open_workbook_auto(path)?;
        let sheet_names = workbook.sheet_names().to_vec();

        if sheet_names.is_empty() {
            return Ok(DataTable::empty());
        }

        let range = workbook
            .worksheet_range(&sheet_names[0])
            .map_err(|e| anyhow::anyhow!("Failed to read sheet: {}", e))?;

        let mut rows_iter = range.rows();

        // First row = headers
        let header_row = match rows_iter.next() {
            Some(r) => r,
            None => return Ok(DataTable::empty()),
        };

        let columns: Vec<ColumnInfo> = header_row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let name = match cell {
                    Data::String(s) => s.clone(),
                    Data::Float(f) => format!("{}", f),
                    Data::Int(i) => format!("{}", i),
                    Data::Bool(b) => format!("{}", b),
                    _ => format!("Column{}", i + 1),
                };
                ColumnInfo {
                    name,
                    data_type: "Utf8".to_string(),
                }
            })
            .collect();

        let col_count = columns.len();
        let mut rows: Vec<Vec<CellValue>> = Vec::new();

        for row in rows_iter {
            let mut cells: Vec<CellValue> = row
                .iter()
                .map(|cell| match cell {
                    Data::Empty => CellValue::Null,
                    Data::String(s) => CellValue::String(s.clone()),
                    Data::Float(f) => CellValue::Float(*f),
                    Data::Int(i) => CellValue::Int(*i),
                    Data::Bool(b) => CellValue::Bool(*b),
                    Data::DateTime(dt) => CellValue::DateTime(format!("{}", dt)),
                    Data::DateTimeIso(s) => CellValue::DateTime(s.clone()),
                    Data::DurationIso(s) => CellValue::String(s.clone()),
                    Data::Error(e) => CellValue::String(format!("#ERR: {:?}", e)),
                })
                .collect();
            // Pad or truncate to match column count
            cells.resize(col_count, CellValue::Null);
            rows.push(cells);
        }

        // Refine column types based on data
        let mut final_columns = columns;
        for (col_idx, col) in final_columns.iter_mut().enumerate() {
            let mut has_int = false;
            let mut has_float = false;
            let mut has_bool = false;
            let mut has_datetime = false;
            let mut has_string = false;

            for row in &rows {
                match row.get(col_idx) {
                    Some(CellValue::Int(_)) => has_int = true,
                    Some(CellValue::Float(_)) => has_float = true,
                    Some(CellValue::Bool(_)) => has_bool = true,
                    Some(CellValue::DateTime(_)) => has_datetime = true,
                    Some(CellValue::String(_)) => has_string = true,
                    _ => {}
                }
            }

            col.data_type = if has_string {
                "Utf8".to_string()
            } else if has_datetime {
                "Timestamp(Microsecond, None)".to_string()
            } else if has_float {
                "Float64".to_string()
            } else if has_int {
                "Int64".to_string()
            } else if has_bool {
                "Boolean".to_string()
            } else {
                "Utf8".to_string()
            };
        }

        Ok(DataTable {
            columns: final_columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Excel".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Write headers
        for (col_idx, col) in table.columns.iter().enumerate() {
            worksheet.write_string(0, col_idx as u16, &col.name)?;
        }

        // Write data rows
        for row_idx in 0..table.row_count() {
            let xlsx_row = (row_idx + 1) as u32;
            for col_idx in 0..table.col_count() {
                if let Some(cell) = table.get(row_idx, col_idx) {
                    match cell {
                        CellValue::Int(i) => {
                            worksheet.write_number(xlsx_row, col_idx as u16, *i as f64)?;
                        }
                        CellValue::Float(f) => {
                            worksheet.write_number(xlsx_row, col_idx as u16, *f)?;
                        }
                        CellValue::Bool(b) => {
                            worksheet.write_boolean(xlsx_row, col_idx as u16, *b)?;
                        }
                        CellValue::Null => {}
                        other => {
                            worksheet.write_string(xlsx_row, col_idx as u16, other.to_string())?;
                        }
                    }
                }
            }
        }

        workbook.save(path)?;
        Ok(())
    }
}
