pub mod parquet_reader;

use crate::data::DataTable;
use anyhow::Result;
use std::path::Path;

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
        // Register built-in formats here.
        // To add a new format, just add another line:
        registry.register(Box::new(parquet_reader::ParquetReader));
        registry
    }

    /// Register a new format reader.
    pub fn register(&mut self, reader: Box<dyn FormatReader>) {
        self.readers.push(reader);
    }

    /// Find a reader that can handle the given file path based on extension.
    pub fn reader_for_path(&self, path: &Path) -> Option<&dyn FormatReader> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())?;
        self.readers
            .iter()
            .find(|r| r.extensions().contains(&ext.as_str()))
            .map(|r| r.as_ref())
    }

    /// Get reader names and their extensions for display.
    pub fn format_descriptions(&self) -> Vec<(String, Vec<String>)> {
        self.readers
            .iter()
            .map(|r| {
                (
                    r.name().to_string(),
                    r.extensions().iter().map(|e| e.to_string()).collect(),
                )
            })
            .collect()
    }
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}
