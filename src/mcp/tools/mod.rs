//! Shared helpers for MCP tool handlers. Each tool lives in its own
//! submodule so adding one is a drop-in (write the file, add it to the
//! `mod` list here, add a wrapper method to `OctaMcpServer`).

pub mod compare_schemas;
pub mod convert;
pub mod correlation;
pub mod count_rows;
/// Chat-only (rendered from chat dispatch, not registered with the MCP server).
pub mod create_chart;
pub mod describe_file;
pub mod diff_tables;
pub mod edit_table;
pub mod export_schema;
pub mod find_duplicates;
pub mod grep_files;
pub mod list_tables;
pub mod pivot;
pub mod profile;
pub mod read_table;
/// Chat-only (rendered from chat dispatch, not registered with the MCP server).
pub mod read_text;
pub mod run_sql;
pub mod sample;
pub mod schema;
pub mod search;
pub mod tail;
pub mod transform_columns;
pub mod unique_columns;
pub mod validate_schema;
pub mod value_frequency;
pub mod write_table;
/// Chat-only (rendered from chat dispatch, not registered with the MCP server).
pub mod write_text;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;

/// A snapshot of an open GUI tab's table. Handed to the in-GUI chat agent so
/// tools can read in-memory (possibly edited) data from a worker thread
/// without borrowing `TabState`. The MCP server always builds an empty list,
/// so its behaviour is unchanged.
#[derive(Clone)]
pub struct TableSnapshot {
    /// Stable handle for this tab within a turn (e.g. `"#1"`), so tabs that
    /// share a display name are still unambiguously addressable.
    pub handle: String,
    /// Tab title shown in the GUI (used to address the tab by name).
    pub display_name: String,
    /// The file the tab was loaded from, if any (unsaved tabs have none).
    pub source_path: Option<String>,
    /// A clone of the tab's table with edits already materialised.
    pub table: DataTable,
}

/// Where a tool should read its primary table from. File sources go through
/// the format registry; tab sources clone an in-memory [`TableSnapshot`].
pub enum Source {
    /// A file on disk (optionally a named inner table for multi-table sources).
    Path {
        path: PathBuf,
        table: Option<String>,
    },
    /// A specific open tab, addressed by its display name.
    OpenTab(String),
    /// Whichever tab is currently active.
    ActiveTab,
}

/// Execution context shared by the MCP handlers and the in-GUI chat agent.
/// Carries the open-tab snapshots (empty for MCP) plus the row / cell caps a
/// tool applies to its response.
pub struct ToolContext {
    /// Snapshots of the open GUI tabs (empty when running under `--mcp`).
    pub open_tabs: Vec<TableSnapshot>,
    /// Index into `open_tabs` of the active tab, if any.
    pub active_tab: Option<usize>,
    /// Default response row cap when a call omits `limit` (None = unlimited).
    pub default_row_limit: Option<usize>,
    /// Per-cell byte cap for serialised responses (0 = unlimited).
    pub cell_byte_cap: usize,
    /// When set (the in-GUI chat agent), file access is sandboxed: reads are
    /// limited to [`Self::allowed_read_paths`] and writes are routed through
    /// [`Self::resolve_write_path`]. The MCP server / CLI leave this `false`.
    pub restrict_filesystem: bool,
    /// Canonicalised paths the chat agent may read from (the open tabs' source
    /// files). Ignored unless `restrict_filesystem`.
    pub allowed_read_paths: Vec<PathBuf>,
    /// Directory the chat agent writes exports into when the caller gives a
    /// bare/relative filename. `None` for MCP / CLI (no confinement).
    pub export_dir: Option<PathBuf>,
}

impl ToolContext {
    /// Context for the MCP server: no open tabs, caps from `AppSettings`, no
    /// filesystem sandbox (the CLI / MCP client is trusted).
    pub fn for_mcp(default_row_limit: Option<usize>, cell_byte_cap: usize) -> Self {
        Self {
            open_tabs: Vec::new(),
            active_tab: None,
            default_row_limit,
            cell_byte_cap,
            restrict_filesystem: false,
            allowed_read_paths: Vec::new(),
            export_dir: None,
        }
    }

    /// Resolve a [`Source`] into a concrete [`DataTable`]. File sources read
    /// through the format registry; tab sources clone the snapshot.
    pub fn resolve(&self, source: &Source) -> anyhow::Result<DataTable> {
        match source {
            Source::Path { path, table } => {
                if path.as_os_str().is_empty() {
                    anyhow::bail!("no `path` or `open_tab` was provided");
                }
                // The model often addresses an open tab via `path` (its handle
                // `#2`, its name, or its file name) instead of `open_tab`. When
                // no specific inner table is requested, honor that and use the
                // in-memory snapshot. (With an inner `table` we fall through so
                // sibling sheets/tables of an open file still read from disk.)
                if table.is_none()
                    && let Some(snap) = self.snapshot_for_pathish(&path.to_string_lossy())
                {
                    return Ok(snap.table.clone());
                }
                self.ensure_readable(path)?;
                read_with_registry(path, table.as_deref())
            }
            Source::ActiveTab => {
                let idx = self
                    .active_tab
                    .ok_or_else(|| anyhow::anyhow!("there is no active tab"))?;
                let snap = self
                    .open_tabs
                    .get(idx)
                    .ok_or_else(|| anyhow::anyhow!("active tab index is out of range"))?;
                Ok(snap.table.clone())
            }
            Source::OpenTab(name) => {
                // Prefer the stable handle (e.g. "#2"), so tabs that share a
                // display name stay addressable; fall back to a name match.
                let snap = self
                    .open_tabs
                    .iter()
                    .find(|t| t.handle == *name)
                    .or_else(|| self.open_tabs.iter().find(|t| &t.display_name == name))
                    .ok_or_else(|| anyhow::anyhow!("no open tab named \"{name}\""))?;
                Ok(snap.table.clone())
            }
        }
    }

    /// Map a path-ish string - a tab handle (`#2`), a tab display name, or a
    /// file name / full path - to an open tab, if one matches. Lets the agent
    /// reach open data however the model phrases the reference (e.g. when it
    /// puts the handle or filename in `path` instead of `open_tab`).
    pub fn snapshot_for_pathish(&self, s: &str) -> Option<&TableSnapshot> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        self.open_tabs.iter().find(|t| {
            if t.handle == s || t.display_name == s {
                return true;
            }
            match t.source_path.as_deref() {
                Some(sp) => {
                    sp == s
                        || Path::new(sp)
                            .file_name()
                            .map(|n| n == std::ffi::OsStr::new(s))
                            .unwrap_or(false)
                }
                None => false,
            }
        })
    }

    /// Enforce the read sandbox: when `restrict_filesystem`, a file path is
    /// only readable if it is the source of an open tab (which also unlocks the
    /// other sheets/tables of an open Excel / DuckDB / SQLite file). Returns a
    /// user-facing error otherwise.
    pub fn ensure_readable(&self, path: &Path) -> anyhow::Result<()> {
        if !self.restrict_filesystem {
            return Ok(());
        }
        let want = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if self.allowed_read_paths.iter().any(|p| p == &want) {
            return Ok(());
        }
        anyhow::bail!(
            "The assistant can only read files that are open in Octa. \"{}\" is not open. If you \
meant a tab that is open, pass it as `open_tab` (a handle like `#2`, `@active`, or the tab name), \
not `path`. Otherwise ask the user to open it in Octa (File > Open). (For another sheet or table \
of an open workbook/database, use list_tables then read_table with that open file's `path` and \
the inner table name.)",
            path.display()
        )
    }

    /// Resolve the destination for a chat write. Without the sandbox, the path
    /// is used as-is (MCP / CLI). With the sandbox, writes are confined to
    /// `export_dir`: a bare/relative name is placed there (basename only, no
    /// traversal), and an absolute path is accepted only when it already
    /// points inside the export directory. The export directory is created if
    /// missing. (In-place writes to open tabs go through `ensure_readable`
    /// instead and are unaffected.)
    pub fn resolve_write_path(&self, requested: &Path) -> anyhow::Result<PathBuf> {
        if !self.restrict_filesystem {
            return Ok(requested.to_path_buf());
        }
        let dir = self.export_dir.as_ref().ok_or_else(|| {
            anyhow::anyhow!("no export directory is configured - set one in Settings > Chat")
        })?;
        std::fs::create_dir_all(dir)
            .map_err(|e| anyhow::anyhow!("creating export directory {}: {e}", dir.display()))?;
        let canon_dir = std::fs::canonicalize(dir)
            .map_err(|e| anyhow::anyhow!("resolving export directory {}: {e}", dir.display()))?;
        let confined_err = || {
            anyhow::anyhow!(
                "Writes are confined to the export directory \"{}\". Give a bare file name and \
Octa will write it there; the user can change the directory in Settings > Chat.",
                canon_dir.display()
            )
        };
        if requested.is_absolute() {
            // Honoured only when it already points inside the export dir.
            let parent = requested.parent().ok_or_else(confined_err)?;
            let canon_parent = std::fs::canonicalize(parent).map_err(|_| confined_err())?;
            if canon_parent != canon_dir {
                return Err(confined_err());
            }
        }
        let name = requested
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("a file name is required for the export"))?;
        let candidate = canon_dir.join(name);
        // A pre-existing symlink at the target must not redirect the write
        // outside the export directory.
        if let Ok(meta) = std::fs::symlink_metadata(&candidate)
            && meta.file_type().is_symlink()
        {
            let resolved = std::fs::canonicalize(&candidate).map_err(|_| confined_err())?;
            if !resolved.starts_with(&canon_dir) {
                return Err(confined_err());
            }
        }
        Ok(candidate)
    }

    /// Effective response row cap for one call. Precedence: caller's `Some(0)`
    /// -> unlimited; `Some(n)` -> that value; `None` -> the configured default.
    pub fn resolve_row_cap(&self, requested: Option<usize>) -> Option<usize> {
        match requested {
            Some(0) => None,
            Some(n) => Some(n),
            None => self.default_row_limit,
        }
    }

    /// One JSON summary per open tab (name, active flag, path, row count,
    /// schema) for the chat system prompt so the model knows what is loaded.
    pub fn open_tab_summaries(&self) -> Vec<Value> {
        self.open_tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let cols: Vec<Value> = t
                    .table
                    .columns
                    .iter()
                    .map(|c| {
                        let mut m = Map::new();
                        m.insert("name".to_string(), Value::String(c.name.clone()));
                        m.insert("type".to_string(), Value::String(c.data_type.clone()));
                        Value::Object(m)
                    })
                    .collect();
                let mut m = Map::new();
                m.insert("handle".to_string(), Value::String(t.handle.clone()));
                m.insert(
                    "display_name".to_string(),
                    Value::String(t.display_name.clone()),
                );
                m.insert(
                    "active".to_string(),
                    Value::Bool(self.active_tab == Some(i)),
                );
                m.insert(
                    "source_path".to_string(),
                    t.source_path.clone().map_or(Value::Null, Value::String),
                );
                m.insert("row_count".to_string(), Value::from(t.table.row_count()));
                m.insert("column_count".to_string(), Value::from(t.table.col_count()));
                m.insert("columns".to_string(), Value::Array(cols));
                Value::Object(m)
            })
            .collect()
    }
}

/// Build a [`Source`] from a tool's flat params. `"@active"` selects the
/// active tab; any other `open_tab` value selects that named tab; absence of
/// `open_tab` falls back to the file `path`.
pub fn source_from(open_tab: &Option<String>, path: &Path, table: &Option<String>) -> Source {
    match open_tab.as_deref() {
        Some("@active") => Source::ActiveTab,
        Some(name) => Source::OpenTab(name.to_string()),
        None => Source::Path {
            path: path.to_path_buf(),
            table: table.clone(),
        },
    }
}

/// Read a file with the registry. Honours `table` when the source supports
/// multi-table dispatch (SQLite, DuckDB, GeoPackage), otherwise falls back
/// to `read_file`. Returns a friendly error when no reader claims the path.
pub fn read_with_registry(path: &Path, table: Option<&str>) -> anyhow::Result<DataTable> {
    let registry = FormatRegistry::new();
    let reader = registry
        .reader_for_path(path)
        .ok_or_else(|| anyhow::anyhow!("no reader available for {}", path.display()))?;
    match table {
        Some(name) => reader.read_table(path, name),
        None => reader.read_file(path),
    }
}

/// Serialise a `DataTable` into MCP-friendly JSON, capping the number of
/// rows at `row_cap` (None = unlimited) and the on-wire size of each cell
/// at `cell_byte_cap` (0 = unlimited). The shape is:
/// ```json
/// {
///   "schema": [{"name": "...", "type": "..."}, ...],
///   "rows":   [[v, v, ...], ...],
///   "row_count": N,
///   "truncated": false,
///   "total_rows_available": null,
///   "cell_truncated": false
/// }
/// ```
/// `truncated` is true when the table held more rows than `row_cap` and the
/// returned `rows` were shortened. `cell_truncated` is true when at least
/// one cell was replaced with a marker because it exceeded `cell_byte_cap`.
/// `total_rows_available` is only populated when we know it cheaply (i.e.
/// the table is already fully materialised in memory).
pub fn table_to_json(table: &DataTable, row_cap: Option<usize>, cell_byte_cap: usize) -> Value {
    let total = table.row_count();
    let emit = match row_cap {
        None => total,
        Some(0) => total,
        Some(n) => n.min(total),
    };
    let truncated = emit < total;

    let schema: Vec<Value> = table
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();

    let mut cell_truncated = false;
    let mut rows: Vec<Value> = Vec::with_capacity(emit);
    for r in 0..emit {
        let mut row: Vec<Value> = Vec::with_capacity(table.col_count());
        for c in 0..table.col_count() {
            let (v, was_truncated) =
                cell_to_json(table.get(r, c).unwrap_or(&CellValue::Null), cell_byte_cap);
            if was_truncated {
                cell_truncated = true;
            }
            row.push(v);
        }
        rows.push(Value::Array(row));
    }

    let mut out = Map::new();
    out.insert("schema".to_string(), Value::Array(schema));
    out.insert("rows".to_string(), Value::Array(rows));
    out.insert("row_count".to_string(), Value::from(emit));
    out.insert("truncated".to_string(), Value::Bool(truncated));
    out.insert("total_rows_available".to_string(), Value::from(total));
    out.insert("cell_truncated".to_string(), Value::Bool(cell_truncated));
    Value::Object(out)
}

/// Convert a single cell to JSON, honouring `cell_byte_cap` (0 = unlimited).
/// Returns `(value, was_truncated)`.
fn cell_to_json(cell: &CellValue, cell_byte_cap: usize) -> (Value, bool) {
    let v = match cell {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Int(i) => Value::from(*i),
        CellValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => Value::String(s.clone()),
        CellValue::Binary(b) => {
            // Hex-encoded; ASCII so byte length == char length.
            let mut s = String::with_capacity(b.len() * 2);
            for byte in b {
                use std::fmt::Write;
                let _ = write!(&mut s, "{byte:02x}");
            }
            Value::String(s)
        }
    };
    if cell_byte_cap == 0 {
        return (v, false);
    }
    let Value::String(s) = &v else {
        return (v, false);
    };
    if s.len() <= cell_byte_cap {
        return (v, false);
    }
    let marker = format!(
        "[truncated: {} bytes; cap {} bytes. Slice the value with --sql / run_sql to fetch the rest.]",
        s.len(),
        cell_byte_cap
    );
    (Value::String(marker), true)
}

/// Convert a JSON value into a [`CellValue`], biased by the column's Arrow
/// type string. Inverse of [`cell_to_json`]. JSON arrays / objects are stored
/// verbatim as `Nested` (their serialized text). A string targeting a `Binary`
/// column is hex-decoded when it is valid hex; otherwise it stays a `String`.
pub fn cell_from_json(value: &Value, data_type: &str) -> CellValue {
    match value {
        Value::Null => CellValue::Null,
        Value::Bool(b) => CellValue::Bool(*b),
        Value::Number(n) => {
            let wants_int = data_type.starts_with("Int") || data_type.starts_with("UInt");
            // Int column + integral JSON -> Int. Otherwise prefer Float, then
            // fall back to Int for big integers a column type didn't ask for.
            if let Some(i) = n.as_i64().filter(|_| wants_int) {
                CellValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                CellValue::Float(f)
            } else if let Some(i) = n.as_i64() {
                CellValue::Int(i)
            } else {
                CellValue::String(n.to_string())
            }
        }
        Value::String(s) => {
            if data_type == "Date32" || data_type == "Date64" {
                CellValue::Date(s.clone())
            } else if data_type.starts_with("Timestamp") {
                CellValue::DateTime(s.clone())
            } else if data_type == "Binary" {
                match hex_decode(s) {
                    Some(bytes) => CellValue::Binary(bytes),
                    None => CellValue::String(s.clone()),
                }
            } else {
                CellValue::String(s.clone())
            }
        }
        Value::Array(_) | Value::Object(_) => CellValue::Nested(value.to_string()),
    }
}

/// Decode an even-length ASCII hex string into bytes. Returns `None` on any
/// non-hex character or odd length.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

/// Build a fresh [`DataTable`] from a column spec (name + Arrow type) and
/// positional rows (array-of-arrays, matching the shape `table_to_json`
/// emits). Every row must have exactly `columns.len()` cells. `source_path`
/// and `format_name` are left unset for the caller to fill if desired.
pub fn build_data_table(
    columns: &[(String, String)],
    rows: &[Vec<Value>],
) -> anyhow::Result<DataTable> {
    if columns.is_empty() {
        anyhow::bail!("at least one column is required");
    }
    let col_count = columns.len();
    let column_infos: Vec<ColumnInfo> = columns
        .iter()
        .map(|(name, data_type)| ColumnInfo {
            name: name.clone(),
            data_type: data_type.clone(),
        })
        .collect();

    let mut table_rows: Vec<Vec<CellValue>> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        if row.len() != col_count {
            anyhow::bail!(
                "row {i} has {} cell(s) but the table has {col_count} column(s)",
                row.len()
            );
        }
        let cells: Vec<CellValue> = row
            .iter()
            .enumerate()
            .map(|(c, v)| cell_from_json(v, &columns[c].1))
            .collect();
        table_rows.push(cells);
    }

    let mut table = DataTable::empty();
    table.columns = column_infos;
    table.rows = table_rows;
    Ok(table)
}

/// Serialise a `DataTable`'s schema only (no rows).
pub fn schema_to_json(table: &DataTable) -> Value {
    let schema: Vec<Value> = table
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();
    let mut out = Map::new();
    out.insert("columns".to_string(), Value::Array(schema));
    out.insert("column_count".to_string(), Value::from(table.col_count()));
    Value::Object(out)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
