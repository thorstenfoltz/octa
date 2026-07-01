pub mod archive_reader;
pub mod arrow_ipc_reader;
pub mod avro_reader;
pub mod bson_reader;
pub mod csv_reader;
pub mod dbf_reader;
pub mod duckdb_reader;
pub mod epub_reader;
pub mod excel_reader;
pub mod fwf_reader;
pub mod geojson_reader;
pub mod gpkg_reader;
pub mod hdf5_reader;
pub mod json_reader;
pub mod jupyter_reader;
pub mod lakehouse_reader;
pub mod markdown_reader;
pub mod msgpack_reader;
pub mod netcdf_reader;
pub mod numpy_reader;
pub mod ods_reader;
pub mod orc_reader;
pub mod parquet_reader;
pub mod rds_reader;
pub mod sas_reader;
pub mod shapefile_reader;
pub mod sniff;
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
use std::sync::atomic::{AtomicUsize, Ordering};

/// Copy an existing file to a timestamped sidecar `<path>.bak-YYYYMMDD-HHMMSS`
/// before it is modified in place. Returns the backup path, or `Ok(None)` when
/// `path` does not exist yet (nothing to back up). Names are made unique with a
/// numeric suffix so two backups within the same second do not collide.
///
/// Callers invoke this only when `AppSettings.backup_before_modify` is on; a
/// failure here must abort the modifying save (do not touch a file we could not
/// back up).
pub fn backup_existing_file(path: &std::path::Path) -> anyhow::Result<Option<std::path::PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let base = {
        let mut s = path.as_os_str().to_os_string();
        s.push(format!(".bak-{stamp}"));
        std::path::PathBuf::from(s)
    };
    let mut candidate = base.clone();
    let mut n = 1u32;
    while candidate.exists() {
        let mut s = base.as_os_str().to_os_string();
        s.push(format!("-{n}"));
        candidate = std::path::PathBuf::from(s);
        n += 1;
    }
    std::fs::copy(path, &candidate).map_err(|e| {
        anyhow::anyhow!(
            "backing up {} -> {}: {e}",
            path.display(),
            candidate.display()
        )
    })?;
    Ok(Some(candidate))
}

/// Initial-load row cap shared by the streaming readers (Parquet, CSV, TSV).
/// Mutable at runtime via `set_initial_load_rows` so `AppSettings` can override
/// the 5 M default without each reader having to know about the settings type.
/// Background row streaming uses the same value as its per-chunk size.
/// Setting to `usize::MAX` effectively disables the cap (Settings -> Performance
/// -> "Unlimited" checkbox, CLI `--rows all`, MCP `unlimited: true`).
static INITIAL_LOAD_ROWS: AtomicUsize = AtomicUsize::new(5_000_000);

/// Returns the current first-load row cap. Streaming readers consult this
/// instead of a hardcoded constant.
pub fn initial_load_rows() -> usize {
    INITIAL_LOAD_ROWS.load(Ordering::Relaxed)
}

/// Updates the first-load row cap. Called from `OctaApp` after `AppSettings`
/// loads or whenever the user applies a new value in the Settings dialog.
/// Lower-bounded at 1 so a corrupt setting can't disable loads entirely.
pub fn set_initial_load_rows(n: usize) {
    INITIAL_LOAD_ROWS.store(n.max(1), Ordering::Relaxed);
}

/// RAII override for [`INITIAL_LOAD_ROWS`]. Constructing the guard swaps in a
/// new cap; dropping it restores the previous value. Used by the CLI (`--rows`)
/// and MCP (`unlimited: true`) to lift the cap for a single read without
/// permanently mutating process-wide state.
///
/// **Concurrency**: the swap/restore is *not* safe under concurrent reads with
/// different caps. The CLI is single-threaded; the MCP server uses a
/// current-thread tokio runtime, which serialises tool dispatch. As long as
/// the guard is held inside `spawn_blocking` for the duration of a single
/// reader call, no other guarded read can race.
pub struct InitialLoadRowsGuard {
    previous: usize,
}

impl InitialLoadRowsGuard {
    /// Temporarily set the initial-load cap for the lifetime of this guard.
    /// `temporary` is lower-bounded at 1 (matches [`set_initial_load_rows`]).
    pub fn new(temporary: usize) -> Self {
        let previous = INITIAL_LOAD_ROWS.swap(temporary.max(1), Ordering::SeqCst);
        Self { previous }
    }
}

impl Drop for InitialLoadRowsGuard {
    fn drop(&mut self) {
        INITIAL_LOAD_ROWS.store(self.previous, Ordering::SeqCst);
    }
}

/// Schema description of a single table inside a multi-table source (DB file).
///
/// `schema` is `None` for formats with no schema concept (SQLite, GeoPackage,
/// Excel, ODS, HDF5, NetCDF, ...) and `Some(schema_name)` for DuckDB files whose
/// `information_schema.tables` exposes schemas. The default DuckDB schema is
/// `main`; the picker renders `main.*` entries unqualified to match the way
/// SQL itself resolves them.
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<ColumnInfo>,
    pub row_count: Option<usize>,
}

impl TableInfo {
    /// `schema.name` for non-default schemas, bare `name` otherwise. Used as
    /// both the display label and the identifier handed to
    /// [`FormatReader::read_table`]. Readers that care about schemas split on
    /// the first `.` to recover the pair.
    pub fn qualified_name(&self) -> String {
        match &self.schema {
            Some(s) if s != "main" => format!("{}.{}", s, self.name),
            _ => self.name.clone(),
        }
    }
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

    /// Write `table` back to `path`, permitting column-set / type changes when
    /// `allow_schema_changes` is true. The diff-based DB writers (DuckDB /
    /// SQLite) override this to reconcile the on-disk schema first; every other
    /// writer rewrites the whole file, so for them column changes already work
    /// and the default just calls `write_file`.
    fn write_file_schema_aware(
        &self,
        path: &std::path::Path,
        table: &crate::data::DataTable,
        _allow_schema_changes: bool,
    ) -> anyhow::Result<()> {
        self.write_file(path, table)
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

    /// Whether a multi-table source should open *all* its tables at once
    /// (each in its own tab) rather than prompting the user to pick a single
    /// one. Excel workbooks set this so every sheet opens; DB readers keep the
    /// default `false` (single-select picker). The app still caps the
    /// auto-open count and shows a multi-select picker above that cap.
    fn opens_all_tables(&self) -> bool {
        false
    }
}

/// Bare filenames (no useful extension) that should open as text. Matched on
/// the part before the first `.` (case-insensitive), so `Dockerfile`,
/// `Dockerfile.dev`, `Containerfile`, and `Containerfile.prod` all match.
pub fn filename_reader_name(file_name: &str) -> Option<&'static str> {
    let stem = file_name.split('.').next().unwrap_or(file_name);
    match stem.to_ascii_lowercase().as_str() {
        "dockerfile" | "containerfile" => Some("Text"),
        _ => None,
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
        registry.register(Box::new(ods_reader::OdsReader));
        registry.register(Box::new(avro_reader::AvroReader));
        registry.register(Box::new(arrow_ipc_reader::ArrowIpcReader));
        registry.register(Box::new(xml_reader::XmlFormatReader));
        registry.register(Box::new(toml_reader::TomlReader));
        registry.register(Box::new(yaml_reader::YamlReader));
        registry.register(Box::new(jupyter_reader::JupyterReader));
        registry.register(Box::new(orc_reader::OrcReader));
        registry.register(Box::new(hdf5_reader::Hdf5Reader));
        registry.register(Box::new(markdown_reader::MarkdownReader));
        registry.register(Box::new(epub_reader::EpubReader));
        registry.register(Box::new(geojson_reader::GeoJsonReader));
        registry.register(Box::new(archive_reader::ArchiveReader));
        registry.register(Box::new(sqlite_reader::SqliteReader));
        registry.register(Box::new(gpkg_reader::GeoPackageReader));
        registry.register(Box::new(duckdb_reader::DuckDbReader));
        registry.register(Box::new(sas_reader::SasFormatReader));
        registry.register(Box::new(spss_reader::SpssReader));
        registry.register(Box::new(stata_reader::StataReader));
        registry.register(Box::new(dbf_reader::DbfReader));
        registry.register(Box::new(rds_reader::RdsReader));
        registry.register(Box::new(netcdf_reader::NetCdfReader));
        registry.register(Box::new(fwf_reader::FwfReader));
        registry.register(Box::new(numpy_reader::NumpyReader));
        registry.register(Box::new(msgpack_reader::MsgpackReader));
        registry.register(Box::new(bson_reader::BsonReader));
        registry.register(Box::new(shapefile_reader::ShapefileReader));
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
        if let Some(ref ext) = ext
            && let Some(reader) = self
                .readers
                .iter()
                .find(|r| r.extensions().contains(&ext.as_str()))
        {
            return Some(reader.as_ref());
        }
        // Filename match (extension-less conventions like `Dockerfile`). Runs
        // only after an extension match fails, so `Dockerfile.json` still opens
        // as JSON.
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
            && let Some(name) = filename_reader_name(file_name)
            && let Some(reader) = self.reader_by_name(name)
        {
            return Some(reader);
        }
        // No extension match (missing or unknown extension): try to identify
        // the file by content before falling back to plain text. This catches
        // extension-less files and lets a real format (Parquet, SQLite, ...)
        // open through its proper reader.
        if let Some(name) = sniff::sniff_format(path)
            && let Some(reader) = self.reader_by_name(name)
        {
            return Some(reader);
        }
        // Fallback: use Text reader for unknown/missing extensions
        self.reader_by_name("Text")
    }

    /// Find a registered reader by its [`FormatReader::name`]. Used to map a
    /// content-sniff result back to a concrete reader.
    pub fn reader_by_name(&self, name: &str) -> Option<&dyn FormatReader> {
        self.readers
            .iter()
            .find(|r| r.name() == name)
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

#[cfg(test)]
mod backup_tests {
    use super::backup_existing_file;

    #[test]
    fn copies_when_present_noops_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("data.csv");
        // Missing file: no backup, Ok(None).
        assert!(backup_existing_file(&f).unwrap().is_none());

        std::fs::write(&f, b"a,b\n1,2\n").unwrap();
        let bak1 = backup_existing_file(&f).unwrap().expect("backup made");
        assert!(bak1.exists());
        assert_eq!(std::fs::read(&bak1).unwrap(), b"a,b\n1,2\n");
        assert!(
            bak1.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".bak-"),
            "sidecar name carries .bak-: {bak1:?}"
        );
        // A second backup in the same second must not clobber the first.
        let bak2 = backup_existing_file(&f).unwrap().expect("second backup");
        assert_ne!(bak1, bak2, "backup names are unique");
        assert!(bak2.exists());
    }
}

#[cfg(test)]
mod filename_reader_tests {
    use super::*;

    #[test]
    fn dockerfile_matches_text_reader_by_filename() {
        let reg = FormatRegistry::new();
        // No extension, no file on disk needed: the filename step returns before sniff.
        let r = reg
            .reader_for_path(std::path::Path::new("Dockerfile"))
            .unwrap();
        assert_eq!(r.name(), "Text");
        let r2 = reg
            .reader_for_path(std::path::Path::new("Dockerfile.dev"))
            .unwrap();
        assert_eq!(r2.name(), "Text");
        let r3 = reg
            .reader_for_path(std::path::Path::new("Containerfile"))
            .unwrap();
        assert_eq!(r3.name(), "Text");
    }

    #[test]
    fn filename_reader_name_is_case_insensitive_on_stem() {
        assert_eq!(filename_reader_name("dockerfile"), Some("Text"));
        assert_eq!(filename_reader_name("Dockerfile.prod"), Some("Text"));
        assert_eq!(filename_reader_name("notes.txt"), None);
    }
}
