use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use arrow::ipc::reader::FileReader;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub struct ArrowIpcReader;

impl FormatReader for ArrowIpcReader {
    fn name(&self) -> &str {
        "Arrow IPC"
    }

    fn extensions(&self) -> &[&str] {
        &["arrow", "ipc", "feather"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let file = File::open(path)?;
        let reader = FileReader::try_new(file, None)?;
        let schema = reader.schema();

        let columns: Vec<ColumnInfo> = schema
            .fields()
            .iter()
            .map(|f| ColumnInfo {
                name: f.name().clone(),
                data_type: format!("{}", f.data_type()),
            })
            .collect();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        for batch_result in reader {
            let batch = batch_result?;
            for row_idx in 0..batch.num_rows() {
                let mut row = Vec::with_capacity(batch.num_columns());
                for col_idx in 0..batch.num_columns() {
                    let array = batch.column(col_idx);
                    let value = super::parquet_reader::arrow_value_to_cell(array, row_idx);
                    row.push(value);
                }
                rows.push(row);
            }
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Arrow IPC".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        use arrow::datatypes::{Field, Schema};
        use arrow::ipc::writer::FileWriter;

        let fields: Vec<Field> = table
            .columns
            .iter()
            .map(|col| {
                Field::new(
                    &col.name,
                    super::parquet_reader::data_type_from_string(&col.data_type),
                    true,
                )
            })
            .collect();
        let schema = Arc::new(Schema::new(fields));

        let file = File::create(path)?;
        let mut writer = FileWriter::try_new(file, &schema)?;

        let num_rows = table.row_count();
        let mut arrays: Vec<Arc<dyn arrow::array::Array>> = Vec::with_capacity(table.col_count());

        for col_idx in 0..table.col_count() {
            let arrow_type =
                super::parquet_reader::data_type_from_string(&table.columns[col_idx].data_type);
            let array =
                super::parquet_reader::build_arrow_array(&arrow_type, table, col_idx, num_rows);
            arrays.push(array);
        }

        let batch = arrow::record_batch::RecordBatch::try_new(schema, arrays)?;
        writer.write(&batch)?;
        writer.finish()?;

        Ok(())
    }
}
