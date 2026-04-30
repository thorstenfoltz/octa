pub mod arrow_ipc_reader;
pub mod avro_reader;
pub mod csv_reader;
pub mod dbf_reader;
pub mod duckdb_reader;
pub mod excel_reader;
pub mod gpkg_reader;
pub mod hdf5_reader;
pub mod json_reader;
pub mod jupyter_reader;
pub mod markdown_reader;
pub mod orc_reader;
pub mod parquet_reader;
pub mod pdf_reader;
pub mod rds_reader;
pub mod sas_reader;
pub mod spss_reader;
pub mod sqlite_reader;
pub mod stata_reader;
pub mod text_reader;
pub mod toml_reader;
pub mod xml_reader;
pub mod yaml_reader;

use crate::data::{ColumnInfo, DataTable};
use anyhow::Result;
use std::path::Path;

/// Schema description of a single table inside a multi-table source (DB file).
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub row_count: Option<usize>,
}

/// Trait that every format reader must implement.
/// To add a new format, create a struct that implements this trait
/// and register it in `FormatRegistry::default()`.
pub trait FormatReader: Send + Sync {
    /// Human-readable name of the format (e.g., "Parquet", "CSV").
    fn name(&self) -> &str;

    /// File extensions this reader handles (lowercase, without dot).
    fn extensions(&self) -> &[&str];

    /// Read a file into a DataTable.
    fn read_file(&self, path: &Path) -> Result<DataTable>;

    /// Optionally write a DataTable back to a file.
    /// Returns an error by default (read-only format).
    fn write_file(&self, _path: &Path, _table: &DataTable) -> Result<()> {
        anyhow::bail!("Writing is not supported for this format")
    }

    /// Whether this reader supports writing.
    fn supports_write(&self) -> bool {
        false
    }

    /// For container formats (DBs) that hold multiple tables, list the
    /// available tables with their schemas. Returns `Ok(None)` when the
    /// format is single-table (the default), so callers can decide whether
    /// to show a picker dialog.
    fn list_tables(&self, _path: &Path) -> Result<Option<Vec<TableInfo>>> {
        Ok(None)
    }

    /// Read a specific named table from a multi-table source. Default
    /// implementation falls back to `read_file` and ignores the table name.
    fn read_table(&self, path: &Path, _table: &str) -> Result<DataTable> {
        self.read_file(path)
    }
}

/// Registry of all available format readers.
/// New formats are added here.
pub struct FormatRegistry {
    readers: Vec<Box<dyn FormatReader>>,
}

impl FormatRegistry {
    /// Create a registry with all built-in readers.
    pub fn new() -> Self {
        let mut registry = Self {
            readers: Vec::new(),
        };
        // Register built-in formats
        registry.register(Box::new(parquet_reader::ParquetReader));
        registry.register(Box::new(csv_reader::CsvReader));
        registry.register(Box::new(csv_reader::TsvReader));
        registry.register(Box::new(json_reader::JsonReader));
        registry.register(Box::new(json_reader::JsonlReader));
        registry.register(Box::new(excel_reader::ExcelReader));
        registry.register(Box::new(avro_reader::AvroReader));
        registry.register(Box::new(arrow_ipc_reader::ArrowIpcReader));
        registry.register(Box::new(xml_reader::XmlFormatReader));
        registry.register(Box::new(pdf_reader::PdfReader));
        registry.register(Box::new(toml_reader::TomlReader));
        registry.register(Box::new(yaml_reader::YamlReader));
        registry.register(Box::new(jupyter_reader::JupyterReader));
        registry.register(Box::new(orc_reader::OrcReader));
        registry.register(Box::new(hdf5_reader::Hdf5Reader));
        registry.register(Box::new(markdown_reader::MarkdownReader));
        registry.register(Box::new(sqlite_reader::SqliteReader));
        registry.register(Box::new(gpkg_reader::GeoPackageReader));
        registry.register(Box::new(duckdb_reader::DuckDbReader));
        registry.register(Box::new(sas_reader::SasFormatReader));
        registry.register(Box::new(spss_reader::SpssReader));
        registry.register(Box::new(stata_reader::StataReader));
        registry.register(Box::new(dbf_reader::DbfReader));
        registry.register(Box::new(rds_reader::RdsReader));
        registry.register(Box::new(text_reader::TextReader));
        registry
    }

    /// Register a new format reader.
    pub fn register(&mut self, reader: Box<dyn FormatReader>) {
        self.readers.push(reader);
    }

    /// Find a reader that can handle the given file path based on extension.
    /// Falls back to the Text reader for unknown extensions.
    pub fn reader_for_path(&self, path: &Path) -> Option<&dyn FormatReader> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        if let Some(ref ext) = ext {
            if let Some(reader) = self
                .readers
                .iter()
                .find(|r| r.extensions().contains(&ext.as_str()))
            {
                return Some(reader.as_ref());
            }
        }
        // Fallback: use Text reader for unknown/missing extensions
        self.readers
            .iter()
            .find(|r| r.name() == "Text")
            .map(|r| r.as_ref())
    }

    /// Get format filter labels and their extensions for file dialogs.
    /// Labels use dotted extensions (e.g. ".csv, .tsv") instead of format names.
    pub fn format_descriptions(&self) -> Vec<(String, Vec<String>)> {
        self.readers
            .iter()
            .map(|r| {
                let exts: Vec<String> = r.extensions().iter().map(|e| e.to_string()).collect();
                let label = exts
                    .iter()
                    .map(|e| format!(".{}", e))
                    .collect::<Vec<_>>()
                    .join(", ");
                (label, exts)
            })
            .collect()
    }

    /// Get individual extension filters for save dialogs.
    /// Each extension is its own entry (e.g. ".csv", ".json", ".xlsx" separately).
    pub fn save_format_descriptions(&self) -> Vec<(String, Vec<String>)> {
        let mut result = Vec::new();
        for r in &self.readers {
            if r.supports_write() {
                for ext in r.extensions() {
                    result.push((format!(".{}", ext), vec![ext.to_string()]));
                }
            }
        }
        result
    }

    /// Build a combined filter string with all supported extensions.
    pub fn all_extensions(&self) -> Vec<String> {
        self.readers
            .iter()
            .flat_map(|r| r.extensions().iter().map(|e| e.to_string()))
            .collect()
    }
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}
