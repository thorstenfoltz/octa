//! "Classify with Jev" dialog: ask TypeSafe AI's decision model one question
//! about every row and add the answer and its certainty as two columns. The
//! work is `octa::data::transform::classify`, shared with `--classify` and the
//! MCP tool `classify_rows`; the request goes through `octa::api::jev`.
//!
//! The chosen cells leave the machine, so the dialog says how many values a
//! run sends before it sends them, and the run happens on a worker with a
//! Stop button. Closing the dialog stops a run.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use egui::RichText;
use serde_json::Value;

use octa::api::jev::JevConfig;
use octa::data::CellValue;
use octa::data::transform::classify::{
    self as engine, Answer, Category, ClassifyMode, ClassifySpec, Progress, RunReport,
    SavedClassifier,
};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_line, control_row, control_text_edit};
use octa::ui::message::{partial_note_label, selectable_message};
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;
use super::super::tabs::TabId;
use super::widgets::multi_col_picker;

type Outcome = Result<(Vec<Option<Answer>>, RunReport), String>;

/// What a run would send, worked out once per choice of columns.
struct Pending {
    columns: Vec<usize>,
    /// Undo length of the table the states were read from.
    undo_len: usize,
    states: Arc<Vec<Value>>,
    /// For every row the index of its state.
    rows: Vec<Option<usize>>,
    /// The spec the token estimate was made for, and the estimate.
    estimate: Option<(ClassifySpec, u64)>,
}

/// A run under way.
struct Run {
    tab: TabId,
    spec: ClassifySpec,
    name: String,
    undo_len: usize,
    rows: Vec<Option<usize>>,
    progress: Arc<Progress>,
    slot: Arc<Mutex<Option<Outcome>>>,
}

pub(crate) struct ClassifyState {
    pub(crate) spec: ClassifySpec,
    pub(crate) name: String,
    pub(crate) name_edited: bool,
    pub(crate) size: DialogSize,
    pending: Option<Pending>,
    run: Option<Run>,
    /// Shown under the form: a refused run, or what a finished one left out.
    pub(crate) error: Option<String>,
}

impl Default for ClassifyState {
    fn default() -> Self {
        Self {
            spec: ClassifySpec {
                // Two empty rows to fill in: a choice needs at least two.
                categories: vec![Category::default(), Category::default()],
                levels: vec![String::new(), String::new()],
                ..ClassifySpec::default()
            },
            name: String::new(),
            name_edited: false,
            size: DialogSize::default(),
            pending: None,
            run: None,
            error: None,
        }
    }
}

impl ClassifyState {
    /// The form as a file: columns by name, the column name only when typed.
    fn to_saved(&self, col_names: &[String]) -> SavedClassifier {
        SavedClassifier {
            columns: self
                .spec
                .columns
                .iter()
                .filter_map(|&c| col_names.get(c).cloned())
                .collect(),
            mode: self.spec.mode,
            instructions: self.spec.instructions.clone(),
            categories: self.spec.categories.clone(),
            levels: self.spec.levels.clone(),
            new_column: self.name_edited.then(|| self.name.clone()),
        }
    }

    /// Fill the form from a file. Returns the columns this table lacks.
    fn apply_saved(&mut self, saved: SavedClassifier, col_names: &[String]) -> Vec<String> {
        let blank = Self::default();
        let mut missing = Vec::new();
        self.spec.columns.clear();
        for name in saved.columns {
            match col_names.iter().position(|c| *c == name) {
                Some(i) if !self.spec.columns.contains(&i) => self.spec.columns.push(i),
                Some(_) => {}
                None => missing.push(name),
            }
        }
        self.spec.mode = saved.mode;
        self.spec.instructions = saved.instructions;
        // An empty list keeps the two rows to fill in.
        self.spec.categories = match saved.categories.is_empty() {
            true => blank.spec.categories.clone(),
            false => saved.categories,
        };
        self.spec.levels = match saved.levels.is_empty() {
            true => blank.spec.levels.clone(),
            false => saved.levels,
        };
        self.name_edited = saved.new_column.is_some();
        self.name = saved.new_column.unwrap_or_default();
        missing
    }
}

impl Drop for ClassifyState {
    /// A dialog that goes away takes its run with it: nothing is left to
    /// receive the answers, so nothing more should be sent.
    fn drop(&mut self) {
        if let Some(run) = &self.run {
            run.progress.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl OctaApp {
    /// Entry from Columns > Classify with Jev... and the shortcut.
    pub(crate) fn open_classify(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.table.col_count() == 0
            || self.is_readonly()
            || !octa::api::jev::enabled(&self.settings)
        {
            return;
        }
        self.classify_dialog = Some(ClassifyState::default());
    }

    /// Start the worker for the dialog's spec. `Err` is the reason it did not.
    fn start_classify(&mut self, st: &mut ClassifyState, name: String) -> Result<(), String> {
        let config = JevConfig::check(&self.settings).map_err(|m| t(m.i18n_key()))?;
        let tab = self.tabs.get(self.active_tab).ok_or_else(String::new)?;
        let pending = st.pending.take().ok_or_else(String::new)?;
        let progress = Arc::new(Progress::default());
        let slot: Arc<Mutex<Option<Outcome>>> = Arc::new(Mutex::new(None));
        st.run = Some(Run {
            tab: tab.id,
            spec: st.spec.clone(),
            name,
            undo_len: pending.undo_len,
            rows: pending.rows,
            progress: progress.clone(),
            slot: slot.clone(),
        });
        let states = pending.states;
        let spec = st.spec.clone();
        let task = self.long_tasks.start(t("classify.title"));
        std::thread::spawn(move || {
            let _task = task;
            let send = config.sender();
            let outcome = engine::classify_states(
                &states,
                &spec,
                &config.model,
                config.parallel,
                &send,
                &progress,
            )
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
        });
        Ok(())
    }

    /// Put a finished run's answers into its tab as two new columns, one undo
    /// step. `Err` when the tab is gone or its rows moved meanwhile.
    fn apply_classify(
        &mut self,
        run: &Run,
        answers: &[Option<Answer>],
        report: &RunReport,
    ) -> Result<(), String> {
        let readonly = self.is_readonly();
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == run.tab)
            .filter(|tab| {
                !readonly
                    && tab.table.undo_stack.len() == run.undo_len
                    && tab.table.row_count() == run.rows.len()
            })
            .ok_or_else(|| t("classify.table_changed"))?;
        let table = &mut tab.table;
        let start = table.undo_stack.len();
        let first = table.col_count();
        for (i, col) in engine::result_columns(run.spec.mode, &run.name)
            .into_iter()
            .enumerate()
        {
            table.insert_column(first + i, col.name, col.data_type);
        }
        for (row, state) in run.rows.iter().enumerate() {
            if let Some(a) = state.and_then(|s| answers.get(s)?.as_ref()) {
                table.set(row, first, a.value.clone());
                if a.certainty != CellValue::Null {
                    table.set(row, first + 1, a.certainty.clone());
                }
            }
        }
        table.coalesce_undo_since(start);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        self.status_message = Some((
            t("classify.done")
                .replace("{name}", &run.name)
                .replace("{sent}", &report.sent.to_string())
                .replace("{tokens}", &report.input_tokens.to_string()),
            Instant::now(),
        ));
        Ok(())
    }
}

/// What a finished run left unanswered, for the line under the form.
fn failed_note(report: &RunReport) -> Option<String> {
    (report.failed > 0).then(|| {
        let mut note = t("classify.done_failed")
            .replace("{failed}", &report.failed.to_string())
            .replace("{sent}", &report.sent.to_string());
        if let Some(e) = &report.first_error {
            note.push('\n');
            note.push_str(e);
        }
        note
    })
}

pub(crate) fn render_classify_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.classify_dialog.take() else {
        return;
    };

    // A finished run lands first, whichever tab is in front by now.
    let finished = st
        .run
        .as_ref()
        .and_then(|run| run.slot.lock().ok().and_then(|mut g| g.take()));
    if let Some(outcome) = finished
        && let Some(run) = st.run.take()
    {
        match outcome {
            Ok((answers, report)) => match app.apply_classify(&run, &answers, &report) {
                // All answered: nothing left to say, the dialog goes.
                Ok(()) => match failed_note(&report) {
                    None => return,
                    note => st.error = note,
                },
                Err(e) => st.error = Some(e),
            },
            // Stop was the user's own doing and needs no message.
            Err(_) if run.progress.cancel.load(Ordering::Relaxed) => {}
            Err(e) => st.error = Some(e),
        }
    }

    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let table = &tab.table;
    let col_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    // A column deleted while the dialog was open drops out of the pick.
    st.spec.columns.retain(|&c| c < col_names.len());
    if !st.name_edited {
        st.name = engine::default_name(st.spec.mode).to_string();
    }
    let name = st.name.trim().to_string();
    let running = st.run.is_some();
    let partial = tab.source_has_more();

    // What a run would send. The states follow the columns, the estimate the
    // whole question, so typing does not walk the table again.
    if !running {
        let undo_len = table.undo_stack.len();
        if st.spec.columns.is_empty() {
            st.pending = None;
        } else if st
            .pending
            .as_ref()
            .is_none_or(|p| p.columns != st.spec.columns || p.undo_len != undo_len)
        {
            let (states, rows) = engine::distinct_states(table, &st.spec);
            st.pending = Some(Pending {
                columns: st.spec.columns.clone(),
                undo_len,
                states: Arc::new(states),
                rows,
                estimate: None,
            });
        }
        if let Some(p) = &mut st.pending
            && p.estimate.as_ref().is_none_or(|(s, _)| *s != st.spec)
        {
            p.estimate = Some((
                st.spec.clone(),
                engine::estimate_tokens(&p.states, &st.spec),
            ));
        }
    }
    let to_send = st.pending.as_ref().map_or(0, |p| p.states.len());
    let tokens = st
        .pending
        .as_ref()
        .and_then(|p| p.estimate.as_ref())
        .map_or(0, |(_, n)| *n);

    let taken = engine::result_columns(st.spec.mode, &name)
        .iter()
        .any(|c| col_names.contains(&c.name));
    let why_not: Option<String> = if running {
        Some(t("classify.running"))
    } else if let Some(key) = st.spec.problem() {
        Some(t(key))
    } else if name.is_empty() {
        Some(t("classify.need_name"))
    } else if taken {
        Some(t("hashcols.name_taken"))
    } else if to_send == 0 {
        Some(t("classify.nothing_to_send"))
    } else {
        None
    };

    let mut close = false;
    let mut start = false;
    let mut report = false;
    let mut save = false;
    let mut load = false;
    let dialog_id = egui::Id::new("octa_classify_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(560.0, 640.0));
    let window = egui::Window::new("octa_classify")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(640.0)
            .min_width(380.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "classify_header",
            RichText::new(t("classify.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("classify_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    if octa::ui::menu_entry::menu_entry(
                        ui,
                        &t("classify.run"),
                        &t("classify.run_hint"),
                        why_not.as_deref(),
                    ) {
                        start = true;
                    }
                    // Jev's labels are model output: the Store asks for a way
                    // to report them from where they are made.
                    if ui
                        .button(t("ai_report.button"))
                        .on_hover_text(t("ai_report.jev_button_hint"))
                        .clicked()
                    {
                        report = true;
                    }
                    if let Some(run) = &st.run {
                        if ui
                            .button(t("classify.stop"))
                            .on_hover_text(t("classify.stop_hint"))
                            .clicked()
                        {
                            run.progress.cancel.store(true, Ordering::Relaxed);
                        }
                        ui.spinner();
                        ui.label(
                            t("classify.progress")
                                .replace(
                                    "{done}",
                                    &run.progress.done.load(Ordering::Relaxed).to_string(),
                                )
                                .replace(
                                    "{total}",
                                    &run.progress.total.load(Ordering::Relaxed).to_string(),
                                ),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(150));
                    } else if ui
                        .button(t("common.close"))
                        .on_hover_text(t("classify.close_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    RichText::new(t("classify.intro"))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(t("jev.warning")).color(ui.visuals().warn_fg_color));
                if partial {
                    partial_note_label(ui, &t("classify.partial_note"));
                }
                ui.add_space(6.0);

                ui.add_enabled_ui(!running, |ui| {
                    control_row(ui, |ui| {
                        load |= ui
                            .button(t("dialog.rename_load_file"))
                            .on_hover_text(t("classify.load_hint"))
                            .clicked();
                        save |= ui
                            .button(t("binview.save"))
                            .on_hover_text(t("classify.save_hint"))
                            .clicked();
                    });
                    ui.add_space(6.0);
                    ui.label(t("classify.columns"))
                        .on_hover_text(t("classify.columns_hint"));
                    multi_col_picker(ui, "classify_columns", &mut st.spec.columns, &col_names);
                    ui.add_space(6.0);

                    control_grid(ui, "classify_grid", |ui| {
                        ui.label(t("classify.mode"))
                            .on_hover_text(t("classify.mode_hint"));
                        egui::ComboBox::from_id_salt("classify_mode")
                            .selected_text(t(st.spec.mode.i18n_key()))
                            .show_ui(ui, |ui| {
                                for m in ClassifyMode::ALL {
                                    ui.selectable_value(&mut st.spec.mode, m, t(m.i18n_key()));
                                }
                            })
                            .response
                            .on_hover_text(t("classify.mode_hint"));
                        ui.end_row();

                        ui.label(t("classify.question"))
                            .on_hover_text(t("classify.question_hint"));
                        ui.add(
                            egui::TextEdit::multiline(&mut st.spec.instructions)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        )
                        .on_hover_text(t("classify.question_hint"));
                        ui.end_row();

                        ui.label(t("classify.name"))
                            .on_hover_text(t("classify.name_hint"));
                        if control_text_edit(ui, 220.0, egui::TextEdit::singleline(&mut st.name))
                            .on_hover_text(t("classify.name_hint"))
                            .changed()
                        {
                            st.name_edited = true;
                        }
                        ui.end_row();
                    });
                    if taken {
                        ui.colored_label(ui.visuals().warn_fg_color, t("hashcols.name_taken"));
                    }

                    ui.add_space(6.0);
                    match st.spec.mode {
                        ClassifyMode::Categories => categories_editor(ui, &mut st.spec.categories),
                        ClassifyMode::Scale => levels_editor(ui, &mut st.spec.levels),
                        ClassifyMode::YesNo => {}
                    }
                });

                ui.add_space(8.0);
                if !st.spec.columns.is_empty() && !running {
                    ui.label(
                        t("classify.will_send")
                            .replace("{n}", &to_send.to_string())
                            .replace("{tokens}", &tokens.to_string()),
                    );
                }
                if let Some(e) = &st.error {
                    selectable_message(ui, ui.visuals().error_fg_color, e);
                }
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if save
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("classifier.json")
            .save_file()
    {
        match st.to_saved(&col_names).save(&path) {
            Ok(()) => {
                st.error = None;
                app.status_message = Some((
                    t("binview.saved").replace("{path}", &path.display().to_string()),
                    Instant::now(),
                ));
            }
            Err(e) => st.error = Some(format!("{e:#}")),
        }
    }
    if load
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
    {
        st.error = match SavedClassifier::load(&path) {
            Ok(saved) => {
                let missing = st.apply_saved(saved, &col_names);
                (!missing.is_empty())
                    .then(|| t("classify.missing_columns").replace("{names}", &missing.join(", ")))
            }
            Err(e) => Some(format!("{e:#}")),
        };
    }
    if report {
        app.show_ai_report_dialog = true;
    }
    if start && !app.is_readonly() {
        st.error = app.start_classify(&mut st, name).err();
    }
    if !close {
        app.classify_dialog = Some(st);
    }
}

/// One row per category: its name, what belongs there, a remove button.
fn categories_editor(ui: &mut egui::Ui, categories: &mut Vec<Category>) {
    ui.label(t("classify.categories"))
        .on_hover_text(t("classify.categories_hint"));
    let mut remove = None;
    control_grid(ui, "classify_categories", |ui| {
        for (i, c) in categories.iter_mut().enumerate() {
            control_text_edit(
                ui,
                140.0,
                egui::TextEdit::singleline(&mut c.name).hint_text(t("classify.category_name")),
            )
            .on_hover_text(t("classify.categories_hint"));
            control_line(ui, |ui| {
                control_text_edit(
                    ui,
                    260.0,
                    egui::TextEdit::singleline(&mut c.description)
                        .hint_text(t("classify.category_description")),
                )
                .on_hover_text(t("classify.categories_hint"));
                if ui
                    .button(t("classify.remove"))
                    .on_hover_text(t("classify.remove_hint"))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        categories.remove(i);
    }
    if octa::ui::menu_entry::menu_entry(
        ui,
        &t("classify.add_category"),
        &t("classify.categories_hint"),
        (categories.len() >= engine::MAX_CATEGORIES)
            .then(|| t("classify.too_many_categories"))
            .as_deref(),
    ) {
        categories.push(Category::default());
    }
}

/// One row per level of the scale, lowest first.
fn levels_editor(ui: &mut egui::Ui, levels: &mut Vec<String>) {
    ui.label(t("classify.levels"))
        .on_hover_text(t("classify.levels_hint"));
    let mut remove = None;
    for (i, level) in levels.iter_mut().enumerate() {
        control_line(ui, |ui| {
            ui.label(format!("{}", i + 1));
            control_text_edit(ui, 260.0, egui::TextEdit::singleline(level))
                .on_hover_text(t("classify.levels_hint"));
            if ui
                .button(t("classify.remove"))
                .on_hover_text(t("classify.remove_hint"))
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        levels.remove(i);
    }
    if octa::ui::menu_entry::menu_entry(
        ui,
        &t("classify.add_level"),
        &t("classify.levels_hint"),
        (levels.len() >= engine::MAX_LEVELS)
            .then(|| t("classify.too_many_levels"))
            .as_deref(),
    ) {
        levels.push(String::new());
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
