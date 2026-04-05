use octo::data::{CellValue, ColumnInfo, DataTable};
use octo::formats::FormatRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Once;

static INIT: Once = Once::new();

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn sample_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "active".into(),
                data_type: "Boolean".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::String("Alice".into()),
                CellValue::Bool(true),
            ],
            vec![
                CellValue::Int(2),
                CellValue::String("Bob".into()),
                CellValue::Bool(false),
            ],
            vec![
                CellValue::Int(3),
                CellValue::String("Charlie".into()),
                CellValue::Bool(true),
            ],
        ],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
    }
}

/// Generate binary fixture files (parquet, avro, arrow, xlsx, pdf) if they don't exist.
pub fn ensure_fixtures() {
    INIT.call_once(|| {
        let registry = FormatRegistry::new();
        let table = sample_table();

        let binary_fixtures: &[(&str, &str)] = &[
            ("sample.parquet", "parquet"),
            ("sample.avro", "avro"),
            ("sample.arrow", "arrow"),
            ("sample.xlsx", "xlsx"),
            ("sample.pdf", "pdf"),
        ];

        for (filename, ext) in binary_fixtures {
            let path = fixture_path(filename);
            if !path.exists() {
                let dummy_path = PathBuf::from(format!("dummy.{}", ext));
                let reader = registry.reader_for_path(&dummy_path).unwrap();
                if reader.supports_write() {
                    // For PDF, use a text-based table
                    let write_table = if *ext == "pdf" {
                        pdf_table()
                    } else {
                        table.clone()
                    };
                    reader.write_file(&path, &write_table).unwrap();
                }
            }
        }
    });
}

fn pdf_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "line".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "text".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![CellValue::Int(1), CellValue::String("Hello World".into())],
            vec![
                CellValue::Int(2),
                CellValue::String("Sample PDF content".into()),
            ],
        ],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
    }
}
