//! "Parse in new tab" modal.
//!
//! Triggered from Edit → "Parse in new tab" or the cell right-click
//! context menu. Lets the user pick a parser format and (for CSV/TSV) a
//! delimiter, then opens the parsed result in a new tab via the existing
//! reader infrastructure. The whole flow runs through a temporary file
//! so it touches the same code path as a normal file open — no parallel
//! "in-memory reader" universe to maintain.

use std::io::Write;

use eframe::egui;

use octa::ui::toolbar::ParseScope;

use super::super::state::{OctaApp, TabState};

/// Each entry is `(human-readable name, primary file extension)`. The
/// name is what the user picks in the dropdown; the extension is what
/// we name the tempfile so `FormatRegistry::reader_for_path` picks the
/// right reader. Order is the order the dropdown shows them in.
///
/// Limited to text-style formats — parsing arbitrary cell content as
/// Parquet/Excel/HDF5/etc. would need binary bytes and produce noise.
const PARSE_FORMATS: &[(&str, &str)] = &[
    ("JSON", "json"),
    ("JSON Lines", "jsonl"),
    ("YAML", "yaml"),
    ("TOML", "toml"),
    ("XML", "xml"),
    ("CSV", "csv"),
    ("TSV", "tsv"),
    ("Markdown", "md"),
    ("Plain Text", "txt"),
];

/// Modal state held on `OctaApp.pending_parse_modal`. Captures the source
/// scope as raw strings up front so the dialog doesn't need to re-walk
/// the live table every frame (which would also let edits to the source
/// table sneak into the in-flight parse).
pub(crate) struct ParseModalState {
    /// Original scope picked by the user. Drives the label and table-mode
    /// serialization choice.
    pub scope: ParseScope,
    /// Display label, e.g. `"Cell R5:C2"` or `"Column 'addr'"`. Computed
    /// when the modal opens.
    pub source_label: String,
    /// Cell strings captured at modal-open time. `None` for Table scope —
    /// those go through a format-writer instead of cell concatenation.
    pub cells: Option<Vec<String>>,
    /// Source-column headers captured at modal-open time, used to keep
    /// the new tab's column names aligned with the source table instead
    /// of letting the format reader treat the first cell value as the
    /// header. For Row scope this is the full column list (length ==
    /// `cells.len()`); for Column and Cell scopes it's a single-element
    /// vec carrying just the affected column's name. Empty for Table
    /// scope (whole-table serialization preserves headers anyway).
    pub headers: Vec<String>,
    /// Index into [`PARSE_FORMATS`].
    pub format_idx: usize,
    /// Delimiter the user picked for CSV / TSV (ignored for other formats).
    pub csv_delimiter: String,
}

impl ParseModalState {
    pub(crate) fn new(
        scope: ParseScope,
        source_label: String,
        cells: Option<Vec<String>>,
        headers: Vec<String>,
    ) -> Self {
        Self {
            scope,
            source_label,
            cells,
            headers,
            // Default to JSON — the original motivation for this feature
            // was un-flattening JSON-shaped cell payloads.
            format_idx: 0,
            csv_delimiter: ",".to_string(),
        }
    }
}

/// Render the modal each frame. No-op when `pending_parse_modal` is None.
pub(crate) fn render_parse_in_new_tab_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pending_parse_modal.is_none() {
        return;
    }

    let mut should_open = false;
    let mut should_cancel = false;

    egui::Window::new("Parse in new tab")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let Some(state) = app.pending_parse_modal.as_mut() else {
                return;
            };

            ui.label(egui::RichText::new(&state.source_label).strong());
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("Parse as:");
                egui::ComboBox::from_id_salt("parse_format_combo")
                    .selected_text(PARSE_FORMATS[state.format_idx].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in PARSE_FORMATS.iter().enumerate() {
                            ui.selectable_value(&mut state.format_idx, i, *name);
                        }
                    });
            });

            // CSV / TSV delimiter sub-option. Hide for other formats so
            // the dialog stays compact.
            let ext = PARSE_FORMATS[state.format_idx].1;
            if ext == "csv" || ext == "tsv" {
                ui.horizontal(|ui| {
                    ui.label("Delimiter:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.csv_delimiter).desired_width(48.0),
                    );
                });
                // Pre-fill TSV with a tab if the user just switched.
                if ext == "tsv" && state.csv_delimiter == "," {
                    state.csv_delimiter = "\t".to_string();
                }
                if ext == "csv" && state.csv_delimiter == "\t" {
                    state.csv_delimiter = ",".to_string();
                }
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(scope_hint(&state.scope))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Open in new tab").clicked() {
                    should_open = true;
                }
                if ui.button("Cancel").clicked() {
                    should_cancel = true;
                }
            });
        });

    if should_open {
        if let Some(state) = app.pending_parse_modal.take() {
            execute_parse(app, state);
        }
    } else if should_cancel {
        app.pending_parse_modal = None;
    }
}

/// One-line hint under the format chooser explaining how the chosen
/// scope's contents are combined. Helps the user predict what the new
/// tab will look like before they click Open.
fn scope_hint(scope: &ParseScope) -> &'static str {
    match scope {
        ParseScope::Cell { .. } => {
            "Serialized as a 1x1 table with the source column name as header, \
             then reopened in the chosen format. (Plain Text passes through verbatim.)"
        }
        ParseScope::Row { .. } => {
            "Serialized as a single-row table with the source column names as headers, \
             then reopened in the chosen format."
        }
        ParseScope::Column { .. } => {
            "Serialized as a single-column table with the source column name as header, \
             then reopened in the chosen format."
        }
        ParseScope::Table => "The whole table is serialized to the chosen format, then reopened.",
    }
}

/// Build the input text for the parser based on scope + format, write it
/// to a temp file, and route the temp file through `OctaApp::load_file`.
/// The new tab's source_path is then cleared so the user doesn't
/// accidentally save back to /tmp.
fn execute_parse(app: &mut OctaApp, state: ParseModalState) {
    let (format_name, ext) = PARSE_FORMATS[state.format_idx];

    // Render the bytes to write to the tempfile.
    let body: Vec<u8> = match &state.scope {
        ParseScope::Table => match serialize_active_table(app, ext) {
            Ok(b) => b,
            Err(e) => {
                app.status_message = Some((
                    format!("Parse in new tab: {}", e),
                    std::time::Instant::now(),
                ));
                return;
            }
        },
        _ => {
            // For cell / row / column the cells field is always Some.
            let cells: &[String] = state
                .cells
                .as_deref()
                .expect("non-Table scope must carry cell strings");
            // Plain Text has no schema, so synthesising a 1-col table
            // would just inject a synthetic header line into the output.
            // Pass the cell text through verbatim and let the TextReader
            // render it as raw lines.
            if ext == "txt" {
                cells.join("\n").into_bytes()
            } else {
                match build_synthetic_payload(&state.scope, cells, &state.headers, ext) {
                    Ok(b) => b,
                    Err(e) => {
                        app.status_message = Some((
                            format!("Parse in new tab: {}", e),
                            std::time::Instant::now(),
                        ));
                        return;
                    }
                }
            }
        }
    };

    // Create a NamedTempFile with the right extension so the registry
    // picks the right reader.
    let suffix = format!(".{}", ext);
    let tmp = tempfile::Builder::new()
        .prefix("octa-parse-")
        .suffix(&suffix)
        .tempfile();
    let mut tmp = match tmp {
        Ok(t) => t,
        Err(e) => {
            app.status_message = Some((
                format!("Parse in new tab: temp file: {}", e),
                std::time::Instant::now(),
            ));
            return;
        }
    };
    if let Err(e) = tmp.write_all(&body) {
        app.status_message = Some((
            format!("Parse in new tab: write: {}", e),
            std::time::Instant::now(),
        ));
        return;
    }
    let path = tmp.path().to_path_buf();
    // Hold the temp file alive past load by leaking the handle — the
    // reader may stream from disk, and dropping the handle before the
    // read finishes would delete the file out from under it. The OS
    // cleans `/tmp` on reboot.
    let _ = tmp.keep();

    app.load_file(path);

    // Strip the tempfile path so the new tab acts like an unsaved
    // scratch buffer. Without this, "Save" would silently overwrite a
    // /tmp file the user never asked to keep.
    if let Some(tab) = app.tabs.get_mut(app.active_tab) {
        tab.table.source_path = None;
        let label = scope_friendly_name(&state.scope, &state.source_label);
        // Stash a friendly display name. The tab bar's tooltip uses
        // `source_path`, which we just cleared, so this matters less,
        // but the format-line stays informative.
        if let Some(fmt) = tab.table.format_name.as_mut() {
            *fmt = format!("{} (parsed from {})", format_name, label);
        } else {
            tab.table.format_name = Some(format!("{} (parsed from {})", format_name, label));
        }
    }
}

fn scope_friendly_name(scope: &ParseScope, _fallback: &str) -> String {
    match scope {
        ParseScope::Cell { row, col } => format!("R{}:C{}", row + 1, col + 1),
        ParseScope::Row { row } => format!("row {}", row + 1),
        ParseScope::Column { col } => format!("col {}", col + 1),
        ParseScope::Table => "whole table".to_string(),
    }
}

/// Build the parsed-payload bytes for a Cell / Row / Column scope by
/// constructing a small synthetic `DataTable` and running it through
/// the format-registry writer. This is the same code path Whole-Table
/// scope uses, so headers are preserved consistently across all
/// formats (CSV / TSV: written as the first data row; JSON / JSONL /
/// YAML / TOML / Markdown: written as the object keys / table
/// columns). Plain Text is short-circuited by the caller — it has no
/// schema concept.
///
/// Shapes:
/// * Cell scope: 1 row × 1 column, header from the source column.
/// * Row scope: 1 row × N columns, headers from the source columns.
/// * Column scope: N rows × 1 column, single header from the source column.
fn build_synthetic_payload(
    scope: &ParseScope,
    cells: &[String],
    headers: &[String],
    ext: &str,
) -> Result<Vec<u8>, String> {
    let rows: Vec<Vec<String>> = match scope {
        ParseScope::Cell { .. } | ParseScope::Row { .. } => vec![cells.to_vec()],
        ParseScope::Column { .. } => cells.iter().map(|c| vec![c.clone()]).collect(),
        ParseScope::Table => unreachable!("Table scope uses serialize_active_table"),
    };
    let table = synthetic_table(headers, rows);
    serialize_table_bytes(&table, ext)
}

/// Build a `DataTable` with the given headers and string-typed rows.
/// All cells go in as `CellValue::String`; the format writers we route
/// through (CSV / TSV / JSON / JSONL / YAML / TOML / Markdown / XML)
/// serialize string cells without complaint.
fn synthetic_table(headers: &[String], rows: Vec<Vec<String>>) -> octa::data::DataTable {
    let columns = headers
        .iter()
        .map(|h| octa::data::ColumnInfo {
            name: h.clone(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    let cell_rows: Vec<Vec<octa::data::CellValue>> = rows
        .into_iter()
        .map(|row| row.into_iter().map(octa::data::CellValue::String).collect())
        .collect();
    octa::data::DataTable {
        columns,
        rows: cell_rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

/// Round-trip a `DataTable` to bytes via the format registry's writer.
/// Shared by `build_synthetic_payload` (Cell / Row / Column) and
/// `serialize_active_table` (Table).
fn serialize_table_bytes(table: &octa::data::DataTable, ext: &str) -> Result<Vec<u8>, String> {
    let tmp = tempfile::Builder::new()
        .prefix("octa-parse-src-")
        .suffix(&format!(".{}", ext))
        .tempfile()
        .map_err(|e| e.to_string())?;
    let tmp_path = tmp.path().to_path_buf();
    let registry = octa::formats::FormatRegistry::new();
    let Some(reader) = registry.reader_for_path(&tmp_path) else {
        return Err(format!("no writer registered for .{}", ext));
    };
    if !reader.supports_write() {
        return Err(format!("{} is read-only", reader.name()));
    }
    reader
        .write_file(&tmp_path, table)
        .map_err(|e| e.to_string())?;
    std::fs::read(&tmp_path).map_err(|e| e.to_string())
}

/// Serialize the currently active tab's table to bytes in the chosen
/// format. Drives the Table-scope code path. Uses the format registry's
/// writer so this stays in sync with "Save As" semantics.
fn serialize_active_table(app: &mut OctaApp, ext: &str) -> Result<Vec<u8>, String> {
    let tab_idx = app.active_tab;
    let Some(tab) = app.tabs.get_mut(tab_idx) else {
        return Err("no active tab".to_string());
    };
    // Apply pending edits so the serialized table matches what the user
    // sees on screen.
    tab.table.apply_edits();
    let table_clone: octa::data::DataTable = tab.table.clone();
    serialize_table_bytes(&table_clone, ext)
}

/// Helper: given a [`ParseScope`] picked from a menu, build the
/// [`ParseModalState`] by extracting the relevant cells out of the
/// active tab. Returns `None` if the scope's coordinates are out of
/// bounds (defensive — shouldn't happen in practice).
pub(crate) fn build_modal_state(tab: &TabState, scope: ParseScope) -> Option<ParseModalState> {
    let table = &tab.table;
    let bdm = octa::data::BinaryDisplayMode::default();
    match scope {
        ParseScope::Cell { row, col } => {
            let value = table.get(row, col)?;
            let text = value.display_with_binary_mode(bdm);
            let col_name = table
                .columns
                .get(col)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "value".to_string());
            let label = format!("Cell R{}:C{} ({})", row + 1, col + 1, col_name);
            Some(ParseModalState::new(
                scope,
                label,
                Some(vec![text]),
                vec![col_name],
            ))
        }
        ParseScope::Row { row } => {
            if row >= table.row_count() {
                return None;
            }
            let cells: Vec<String> = (0..table.col_count())
                .filter_map(|c| table.get(row, c))
                .map(|v| v.display_with_binary_mode(bdm))
                .collect();
            let headers: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
            let label = format!("Row {} ({} cells)", row + 1, cells.len());
            Some(ParseModalState::new(scope, label, Some(cells), headers))
        }
        ParseScope::Column { col } => {
            if col >= table.col_count() {
                return None;
            }
            let col_name = table.columns[col].name.clone();
            let cells: Vec<String> = (0..table.row_count())
                .filter_map(|r| table.get(r, col))
                .map(|v| v.display_with_binary_mode(bdm))
                .collect();
            let label = format!("Column '{}' ({} cells)", col_name, cells.len());
            Some(ParseModalState::new(
                scope,
                label,
                Some(cells),
                vec![col_name],
            ))
        }
        ParseScope::Table => {
            let label = format!(
                "Whole table ({} rows × {} cols)",
                table.row_count(),
                table.col_count()
            );
            Some(ParseModalState::new(scope, label, None, Vec::new()))
        }
    }
}
