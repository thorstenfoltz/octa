//! Write-back dialog for the SQL panel.
//!
//! Pre-fills its target with the active tab's source path when that tab is
//! DuckDB- or SQLite-backed, so the user can `CREATE TABLE` a new table
//! inside the open file in one click. For any other target the user
//! browses to a file. The dialog composes the `WriteTarget`, calls
//! `SqlWorkspace::write_result_to_db`, and surfaces the result via the
//! status bar.

use std::path::{Path, PathBuf};

use eframe::egui;

use octa::sql::{AttachKind, WriteMode, WriteTarget};

use crate::app::state::OctaApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBackKind {
    DuckDb,
    Sqlite,
}

impl WriteBackKind {
    fn from_path(p: &Path) -> Self {
        match AttachKind::from_path(p) {
            AttachKind::DuckDb => WriteBackKind::DuckDb,
            AttachKind::Sqlite => WriteBackKind::Sqlite,
        }
    }
    fn to_attach(self) -> AttachKind {
        match self {
            WriteBackKind::DuckDb => AttachKind::DuckDb,
            WriteBackKind::Sqlite => AttachKind::Sqlite,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBackMode {
    Create,
    Replace,
    Append,
}

impl WriteBackMode {
    fn to_write_mode(self) -> WriteMode {
        match self {
            Self::Create => WriteMode::Create,
            Self::Replace => WriteMode::Replace,
            Self::Append => WriteMode::Append,
        }
    }
}

pub struct SqlWriteBackState {
    pub target_path: PathBuf,
    pub kind: WriteBackKind,
    pub schema: String,
    pub table: String,
    pub mode: WriteBackMode,
    pub create_schema_if_missing: bool,
    pub error: Option<String>,
}

impl SqlWriteBackState {
    pub fn for_active_tab(tab_source: Option<&str>, default_table_hint: &str) -> Self {
        // Pre-fill with the tab's source file when it looks like a real DB
        // file; otherwise leave the path blank so the user picks one.
        let (target_path, kind) = match tab_source.map(PathBuf::from) {
            Some(p)
                if matches!(
                    p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
                    Some(ref ext) if matches!(ext.as_str(), "duckdb" | "ddb" | "sqlite" | "db" | "sqlite3")
                ) =>
            {
                let k = WriteBackKind::from_path(&p);
                (p, k)
            }
            _ => (PathBuf::new(), WriteBackKind::DuckDb),
        };
        Self {
            target_path,
            kind,
            schema: "main".to_string(),
            table: default_table_hint.to_string(),
            mode: WriteBackMode::Create,
            create_schema_if_missing: true,
            error: None,
        }
    }
}

impl OctaApp {
    pub(crate) fn open_sql_write_back_dialog(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        // Require a successful prior SELECT so we have a source query.
        if tab.sql_last_query.trim().is_empty() {
            tab.sql_error = Some(octa::i18n::t("dialog.swb_run_select_first"));
            return;
        }
        let hint = default_table_name_hint(&tab.sql_query);
        let source = tab.table.source_path.clone();
        tab.sql_write_back = Some(SqlWriteBackState::for_active_tab(source.as_deref(), &hint));
    }
}

pub(crate) fn render_sql_write_back_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.tabs[app.active_tab].sql_write_back.is_none() {
        return;
    }
    let mut open = true;
    let mut do_write = false;
    let mut do_cancel = false;

    // Take the state out so we can hand `&mut OctaApp` to the helpers without
    // tripping the borrow checker.
    let mut state = app.tabs[app.active_tab].sql_write_back.take().unwrap();
    let preview_sql = compose_preview(&state, &app.tabs[app.active_tab].sql_last_query);

    egui::Window::new(octa::i18n::t("dialog.swb_title"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            egui::Grid::new("sql_write_back_grid")
                .num_columns(2)
                .spacing(egui::vec2(12.0, 6.0))
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("dialog.swb_target_file"));
                    ui.horizontal(|ui| {
                        let mut tmp = state.target_path.to_string_lossy().into_owned();
                        if ui
                            .add(egui::TextEdit::singleline(&mut tmp).desired_width(380.0))
                            .changed()
                        {
                            state.target_path = PathBuf::from(&tmp);
                            state.kind = WriteBackKind::from_path(&state.target_path);
                        }
                        if ui.button(octa::i18n::t("dialog.swb_browse")).clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .set_title(octa::i18n::t("dialog.swb_browse_title"))
                                .add_filter(
                                    "DuckDB / SQLite",
                                    &["duckdb", "ddb", "sqlite", "db", "sqlite3"],
                                )
                                .save_file()
                        {
                            state.target_path = p;
                            state.kind = WriteBackKind::from_path(&state.target_path);
                        }
                    });
                    ui.end_row();

                    ui.label(octa::i18n::t("dialog.swb_format"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut state.kind, WriteBackKind::DuckDb, "DuckDB");
                        ui.radio_value(&mut state.kind, WriteBackKind::Sqlite, "SQLite");
                    });
                    ui.end_row();

                    ui.label(octa::i18n::t("dialog.swb_schema"));
                    ui.add_enabled_ui(state.kind == WriteBackKind::DuckDb, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut state.schema).desired_width(200.0));
                    });
                    ui.end_row();

                    ui.label(octa::i18n::t("dialog.swb_table"));
                    ui.add(egui::TextEdit::singleline(&mut state.table).desired_width(200.0));
                    ui.end_row();

                    ui.label(octa::i18n::t("dialog.swb_mode"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut state.mode,
                            WriteBackMode::Create,
                            octa::i18n::t("dialog.swb_mode_create"),
                        );
                        ui.radio_value(
                            &mut state.mode,
                            WriteBackMode::Replace,
                            octa::i18n::t("dialog.swb_mode_replace"),
                        );
                        ui.radio_value(
                            &mut state.mode,
                            WriteBackMode::Append,
                            octa::i18n::t("dialog.swb_mode_append"),
                        );
                    });
                    ui.end_row();

                    ui.label("");
                    ui.add_enabled_ui(state.kind == WriteBackKind::DuckDb, |ui| {
                        ui.checkbox(
                            &mut state.create_schema_if_missing,
                            octa::i18n::t("dialog.swb_create_schema"),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(egui::RichText::new(octa::i18n::t("dialog.swb_preview")).strong());
            ui.add_space(2.0);
            egui::ScrollArea::vertical()
                .id_salt("sql_write_back_preview")
                .max_height(120.0)
                .show(ui, |ui| {
                    let mut p = preview_sql.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut p)
                            .interactive(false)
                            .desired_rows(4)
                            .font(egui::TextStyle::Monospace),
                    );
                });

            if let Some(err) = &state.error {
                ui.add_space(4.0);
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    do_cancel = true;
                }
                let enabled =
                    !state.target_path.as_os_str().is_empty() && !state.table.trim().is_empty();
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(octa::i18n::t("dialog.swb_write")),
                    )
                    .clicked()
                {
                    do_write = true;
                }
            });
        });

    if !open || do_cancel {
        return;
    }

    if do_write {
        let last_query = app.tabs[app.active_tab].sql_last_query.clone();
        let schema = if state.kind == WriteBackKind::DuckDb && !state.schema.trim().is_empty() {
            Some(state.schema.trim().to_string())
        } else {
            None
        };
        // Guard against the in-place footer: writing to the open file's
        // current `meta.table_name` would collide with the diff-save path.
        let tab_source_path = app.tabs[app.active_tab]
            .table
            .source_path
            .as_deref()
            .map(PathBuf::from);
        let collides = {
            let tab = &app.tabs[app.active_tab];
            tab.table.db_meta.as_ref().is_some_and(|meta| {
                tab_source_path.as_ref() == Some(&state.target_path)
                    && meta.table_name == state.table
                    && matches!(state.mode, WriteBackMode::Append | WriteBackMode::Replace)
            })
        };
        if collides {
            state.error = Some(octa::i18n::t("dialog.swb_err_collide"));
            app.tabs[app.active_tab].sql_write_back = Some(state);
            return;
        }
        let result = {
            let tab = &mut app.tabs[app.active_tab];
            let ws = match tab.sql_workspace.as_mut() {
                Some(w) => w,
                None => {
                    state.error = Some(octa::i18n::t("dialog.swb_err_no_ws"));
                    tab.sql_write_back = Some(state);
                    return;
                }
            };
            ws.write_result_to_db(&WriteTarget {
                path: state.target_path.clone(),
                kind: state.kind.to_attach(),
                schema: schema.clone(),
                table: state.table.trim().to_string(),
                mode: state.mode.to_write_mode(),
                source_query: last_query,
                create_schema_if_missing: state.create_schema_if_missing,
            })
        };
        match result {
            Ok(report) => {
                app.status_message = Some((
                    format!(
                        "{} {} {} {}",
                        octa::i18n::t("dialog.swb_wrote"),
                        report.rows_written,
                        octa::i18n::t("dialog.swb_rows_to"),
                        report.target_display
                    ),
                    std::time::Instant::now(),
                ));
                // Auto-reload the active tab if its source is the same file we
                // just wrote into so the picker reflects the new entry.
                if let Some(src) = app.tabs[app.active_tab].table.source_path.clone() {
                    let src_path = PathBuf::from(&src);
                    if src_path == state.target_path {
                        app.load_file(src_path);
                    }
                }
            }
            Err(e) => {
                state.error = Some(e.to_string());
                app.tabs[app.active_tab].sql_write_back = Some(state);
            }
        }
        return;
    }

    // Either still open with no action, or `open == true` but user is still
    // editing - put the state back.
    app.tabs[app.active_tab].sql_write_back = Some(state);
}

fn compose_preview(state: &SqlWriteBackState, last_query: &str) -> String {
    let body = last_query.trim().trim_end_matches(';');
    let qualified = match state.kind {
        WriteBackKind::DuckDb => {
            let schema = if state.schema.trim().is_empty() {
                "main".to_string()
            } else {
                state.schema.trim().to_string()
            };
            format!("\"{}\".\"{}\"", schema, state.table.trim())
        }
        WriteBackKind::Sqlite => format!("\"{}\"", state.table.trim()),
    };
    match state.mode {
        WriteBackMode::Create => format!("CREATE TABLE {qualified} AS\n{body}"),
        WriteBackMode::Replace => format!("CREATE OR REPLACE TABLE {qualified} AS\n{body}"),
        WriteBackMode::Append => format!("INSERT INTO {qualified}\n{body}"),
    }
}

fn default_table_name_hint(query: &str) -> String {
    // Heuristic: use the first ALIAS we can spot after FROM, otherwise
    // fall back to "result". The user can rewrite the name in the dialog.
    let lower = query.to_ascii_lowercase();
    if let Some(idx) = lower.find(" from ")
        && let Some(token) = lower[idx + 6..]
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(')
            .find(|s| !s.is_empty())
    {
        let cleaned: String = token
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    "result".to_string()
}
