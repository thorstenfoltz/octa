//! Recipes in the GUI: recording what the user does to a tab, the docked
//! Recipe panel that lists it, saving it as a `.ocp` file, and the Apply
//! recipe dialog that replays one on the current tab.
//!
//! The engine is `octa::data::recipe`; this file only records and shows.
//! Recording happens at each dialog's apply point through
//! [`OctaApp::record_step`], by column name, so the steps mean something on
//! another file. A step remembers the undo-stack length it left behind:
//! undoing past it moves it to `recipe_undone`, and redoing brings it back,
//! so the recipe always matches what the table shows.

use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use octa::data::recipe::{EXTENSION, Recipe, RecipeStep, StepOutcome, apply_recipe};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::state::{OctaApp, TabState};

/// One recorded step on a tab.
#[derive(Debug, Clone)]
pub(crate) struct RecordedStep {
    pub(crate) step: RecipeStep,
    /// `table.undo_stack.len()` right after the step ran.
    pub(crate) undo_mark: usize,
    /// Unticked steps stay listed but are left out of a saved recipe.
    pub(crate) enabled: bool,
    /// Hand edits recorded before the tab had an ID column: the rows as they
    /// looked, so the edits can be tied to an ID once the user names one.
    pub(crate) pending: Option<Vec<PendingEdit>>,
}

/// One hand edit waiting for an ID column: its row, whole, as text (the
/// edited cell holding its value from before the edit).
#[derive(Debug, Clone)]
pub(crate) struct PendingEdit {
    row: Vec<(String, String)>,
    column: String,
    value: Option<String>,
}

/// The "which column identifies a row?" dialog.
pub(crate) struct RecipeKeyState {
    tab: usize,
    cols: Vec<usize>,
    /// Save the recipe once the key is chosen (Save recipe asked for it).
    then_save: bool,
}

/// The Apply recipe dialog: a loaded recipe, then what the replay did.
pub(crate) struct ApplyRecipeState {
    size: DialogSize,
    path: PathBuf,
    recipe: Recipe,
    outcome: Option<Vec<StepOutcome>>,
}

impl TabState {
    /// Undo just ran: steps it took back move aside.
    pub(crate) fn recipe_after_undo(&mut self) {
        let len = self.table.undo_stack.len();
        self.recipe_seen = len;
        while self.recipe.last().is_some_and(|s| s.undo_mark > len) {
            let s = self.recipe.pop().expect("checked");
            self.recipe_undone.push(s);
        }
    }

    /// Redo just ran: steps it brought back return.
    pub(crate) fn recipe_after_redo(&mut self) {
        let len = self.table.undo_stack.len();
        // The redone edit is the returning step, not a new hand edit.
        self.recipe_seen = len;
        while self
            .recipe_undone
            .last()
            .is_some_and(|s| s.undo_mark <= len)
        {
            let s = self.recipe_undone.pop().expect("checked");
            self.recipe.push(s);
        }
    }

    /// The per-tab half of [`OctaApp::sync_hand_edits`]. Returns whether a
    /// step was recorded.
    pub(crate) fn record_hand_edits(&mut self) -> bool {
        use octa::data::UndoAction;
        let tab = self;
        let len = tab.table.undo_stack.len();
        if len <= tab.recipe_seen {
            tab.recipe_seen = len;
            return false;
        }
        let covered = tab.recipe.last().map_or(0, |s| s.undo_mark);
        let start = tab.recipe_seen.max(covered).min(len);
        tab.recipe_seen = len;
        let range = &tab.table.undo_stack[start..len];
        if range.iter().any(|a| {
            matches!(
                a,
                UndoAction::InsertRow { .. } | UndoAction::InsertColumn { .. }
            )
        }) {
            return false;
        }
        let edits: Vec<(usize, usize, octa::data::CellValue, octa::data::CellValue)> = range
            .iter()
            .filter_map(|a| match a {
                UndoAction::CellEdit {
                    row,
                    col,
                    old_value,
                    new_value,
                } => Some((*row, *col, old_value.clone(), new_value.clone())),
                _ => None,
            })
            .collect();
        if edits.is_empty() {
            return false;
        }
        if tab.recipe_key.is_none() {
            tab.recipe_key = octa::data::recipe::guess_row_key(&tab.table).map(|k| vec![k]);
        }
        let text = |v: &octa::data::CellValue| match v {
            octa::data::CellValue::Null => None,
            v => Some(v.to_string()),
        };
        let key_cols: Option<Vec<usize>> = tab.recipe_key.as_ref().and_then(|k| {
            k.iter()
                .map(|n| tab.table.columns.iter().position(|c| &c.name == n))
                .collect()
        });
        let mut cells = Vec::new();
        let mut pending = Vec::new();
        for (row, col, old, new) in edits {
            let Some(column) = tab.table.columns.get(col).map(|c| c.name.clone()) else {
                continue;
            };
            // The row as it was before this edit, so an edited key cell
            // still finds its row on replay.
            let before = |c: usize| {
                if c == col {
                    old.to_string()
                } else {
                    tab.table
                        .get(row, c)
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                }
            };
            match &key_cols {
                Some(kc) => cells.push(octa::data::recipe::CellEdit {
                    row: kc.iter().map(|&c| before(c)).collect(),
                    column,
                    value: text(&new),
                }),
                None => pending.push(PendingEdit {
                    row: (0..tab.table.col_count())
                        .map(|c| (tab.table.columns[c].name.clone(), before(c)))
                        .collect(),
                    column,
                    value: text(&new),
                }),
            }
        }
        let (key, pending) = match key_cols {
            Some(_) => (tab.recipe_key.clone().unwrap_or_default(), None),
            None => (Vec::new(), Some(pending)),
        };
        if let Some(p) = &pending {
            cells = p
                .iter()
                .map(|e| octa::data::recipe::CellEdit {
                    row: Vec::new(),
                    column: e.column.clone(),
                    value: e.value.clone(),
                })
                .collect();
        }
        tab.recipe.push(RecordedStep {
            step: RecipeStep::SetCells(octa::data::recipe::SetCells { key, cells }),
            undo_mark: len,
            enabled: true,
            pending,
        });
        tab.recipe_undone.clear();
        true
    }

    /// Tie every waiting hand edit to `key`, using the row as it was.
    pub(crate) fn resolve_recipe_key(&mut self, key: Vec<String>) {
        for s in &mut self.recipe {
            let Some(pending) = s.pending.take() else {
                continue;
            };
            let cells = pending
                .into_iter()
                .map(|e| octa::data::recipe::CellEdit {
                    row: key
                        .iter()
                        .map(|k| {
                            e.row
                                .iter()
                                .find(|(n, _)| n == k)
                                .map(|(_, v)| v.clone())
                                .unwrap_or_default()
                        })
                        .collect(),
                    column: e.column,
                    value: e.value,
                })
                .collect();
            s.step = RecipeStep::SetCells(octa::data::recipe::SetCells {
                key: key.clone(),
                cells,
            });
        }
        self.recipe_key = Some(key);
    }

    /// The ticked steps, as a recipe.
    pub(crate) fn recipe_to_save(&self) -> Recipe {
        Recipe::new(
            self.recipe
                .iter()
                .filter(|s| s.enabled && s.pending.is_none())
                .map(|s| s.step.clone())
                .collect(),
        )
    }

    /// A file name for this tab's recipe: the data file's stem, else the
    /// tab's label.
    fn recipe_file_name(&self) -> String {
        let stem = self
            .table
            .source_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .or_else(|| self.custom_tab_label.clone())
            .unwrap_or_else(|| "recipe".to_string());
        format!("{stem}.{EXTENSION}")
    }
}

impl OctaApp {
    /// Record `step` on the active tab. Call right after the step's change
    /// was made, so the undo mark covers it.
    pub(crate) fn record_step(&mut self, step: RecipeStep) {
        let tab = &mut self.tabs[self.active_tab];
        tab.recipe.push(RecordedStep {
            step,
            undo_mark: tab.table.undo_stack.len(),
            enabled: true,
            pending: None,
        });
        tab.recipe_undone.clear();
        self.autosave_recipe();
    }

    /// Record a sort on the active tab's columns, `(index, ascending)`,
    /// first key first.
    pub(crate) fn record_sort_keys(&mut self, keys: &[(usize, bool)]) {
        let cols = &self.tabs[self.active_tab].table.columns;
        let by = keys
            .iter()
            .filter_map(|&(c, asc)| {
                cols.get(c).map(|ci| octa::data::recipe::SortKey {
                    column: ci.name.clone(),
                    descending: !asc,
                })
            })
            .collect();
        self.record_step(RecipeStep::Sort(octa::data::recipe::Sort { by }));
    }

    pub(crate) fn record_sort(&mut self, col: usize, ascending: bool) {
        self.record_sort_keys(&[(col, ascending)]);
    }

    /// The names of the active tab's columns at `indices`.
    pub(crate) fn column_names(&self, indices: &[usize]) -> Vec<String> {
        let cols = &self.tabs[self.active_tab].table.columns;
        indices
            .iter()
            .filter_map(|&c| cols.get(c).map(|ci| ci.name.clone()))
            .collect()
    }

    /// Settings > Files > Save recipes automatically: rewrite the tab's
    /// recipe into the chosen folder after every recorded step.
    fn autosave_recipe(&mut self) {
        if !self.settings.recipe_autosave {
            return;
        }
        let Some(dir) = self.settings.recipe_dir() else {
            return;
        };
        let tab = &self.tabs[self.active_tab];
        let path = dir.join(tab.recipe_file_name());
        // The folder (default or chosen) is created on first use.
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", dir.display()))
            .and_then(|()| tab.recipe_to_save().save(&path));
        if let Err(e) = saved {
            self.status_message = Some((
                t("recipe.autosave_failed").replace("{error}", &format!("{e:#}")),
                std::time::Instant::now(),
            ));
        }
    }

    /// Record hand edits: once per frame, the undo entries no recorded step
    /// covers are looked at, and plain cell edits among them become a
    /// `set_cells` step whose rows are named by their ID, not their number.
    ///
    /// Two things are deliberately left out. A frame that also inserted a row
    /// or column (Add column, Duplicate row) fills cells that will not exist
    /// in next month's file. And a dialog's writes arrive folded into one
    /// `Batch` entry, so only top-level `CellEdit`s count as typing, pasting
    /// or find-and-replace.
    /// ponytail: a hand edit and a recorded step in the SAME frame fold into
    /// the step's undo mark and the edit is not recorded; one action per
    /// frame is the norm.
    pub(crate) fn sync_hand_edits(&mut self) {
        for ti in 0..self.tabs.len() {
            if self.tabs[ti].record_hand_edits() && ti == self.active_tab {
                self.autosave_recipe();
            }
        }
    }

    /// Open the "which column identifies a row?" dialog for the active tab.
    pub(crate) fn open_recipe_key_dialog(&mut self, then_save: bool) {
        let tab = &self.tabs[self.active_tab];
        let cols = tab
            .recipe_key
            .as_ref()
            .map(|k| {
                k.iter()
                    .filter_map(|n| tab.table.columns.iter().position(|c| &c.name == n))
                    .collect()
            })
            .unwrap_or_default();
        self.recipe_key_dialog = Some(RecipeKeyState {
            tab: self.active_tab,
            cols,
            then_save,
        });
    }

    /// The user named the ID: tie every waiting edit to it.
    fn set_recipe_key(&mut self, tab_idx: usize, key: Vec<String>) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        tab.resolve_recipe_key(key);
        if tab_idx == self.active_tab {
            self.autosave_recipe();
        }
    }

    pub(crate) fn toggle_recipe_panel(&mut self) {
        self.recipe_panel_visible = !self.recipe_panel_visible;
    }

    /// Save the active tab's ticked steps through a file picker. Hand edits
    /// still waiting for an ID column ask for it first.
    fn save_recipe_as(&mut self) {
        let tab = &self.tabs[self.active_tab];
        if tab.recipe.iter().any(|s| s.enabled && s.pending.is_some()) {
            self.open_recipe_key_dialog(true);
            return;
        }
        let recipe = tab.recipe_to_save();
        let mut picker = rfd::FileDialog::new()
            .add_filter(t("recipe.file_filter"), &[EXTENSION])
            .set_file_name(tab.recipe_file_name());
        if let Some(dir) = self.settings.recipe_dir().filter(|d| d.is_dir()) {
            picker = picker.set_directory(dir);
        }
        let Some(path) = picker.save_file() else {
            return;
        };
        let msg = match recipe.save(&path) {
            Ok(()) => t("recipe.saved").replace("{path}", &path.display().to_string()),
            Err(e) => format!("{e:#}"),
        };
        self.status_message = Some((msg, std::time::Instant::now()));
    }

    /// File picker for a `.ocp`, then the Apply recipe dialog.
    pub(crate) fn pick_and_apply_recipe(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(t("recipe.file_filter"), &[EXTENSION])
            .pick_file()
        {
            self.open_apply_recipe(path);
        }
    }

    /// Open the Apply recipe dialog for `path`. Also what opening a `.ocp`
    /// file does, from the File menu, the sidebar or the command line.
    pub(crate) fn open_apply_recipe(&mut self, path: PathBuf) {
        match Recipe::load(&path) {
            Ok(recipe) => {
                self.apply_recipe_dialog = Some(ApplyRecipeState {
                    size: DialogSize::Normal,
                    path,
                    recipe,
                    outcome: None,
                });
            }
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
            }
        }
    }

    /// Replay on the active tab as ONE undo step, and record the replayed
    /// steps that ran so this tab's recipe keeps growing from here.
    fn run_apply_recipe(&mut self, recipe: &Recipe) -> Vec<StepOutcome> {
        let tab = &mut self.tabs[self.active_tab];
        let start = tab.table.undo_stack.len();
        let outcome = apply_recipe(&mut tab.table, recipe);
        tab.table.coalesce_undo_since(start);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        let mark = tab.table.undo_stack.len();
        for (step, o) in recipe.steps.iter().zip(&outcome) {
            if o.error.is_none() {
                tab.recipe.push(RecordedStep {
                    step: step.clone(),
                    undo_mark: mark,
                    enabled: true,
                    pending: None,
                });
            }
        }
        tab.recipe_undone.clear();
        self.autosave_recipe();
        outcome
    }

    /// The docked Recipe panel on the right.
    pub(crate) fn render_recipe_panel(&mut self, ui: &mut egui::Ui) {
        if !self.recipe_panel_visible {
            return;
        }
        let (default_size, min_size) =
            octa::ui::panel_fit::clamp(ui.available_width(), 300.0, 200.0);
        let mut close = false;
        let mut save = false;
        let mut apply = false;
        let mut clear = false;
        let mut remove: Option<usize> = None;
        let mut choose_key = false;
        egui::Panel::right("octa_recipe_panel")
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                let tab = &mut self.tabs[self.active_tab];
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            t("recipe.panel_title")
                                .replace("{count}", &tab.recipe.len().to_string()),
                        )
                        .strong(),
                    )
                    .on_hover_text(t("recipe.panel_hint"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui
                            .button("x")
                            .on_hover_text(t("recipe.close_hint"))
                            .clicked();
                    });
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let any = tab.recipe.iter().any(|s| s.enabled);
                    let b = ui.add_enabled(any, egui::Button::new(t("recipe.save")));
                    save = if any {
                        b.on_hover_text(t("recipe.save_hint")).clicked()
                    } else {
                        b.on_disabled_hover_text(t("recipe.save_disabled"));
                        false
                    };
                    apply = ui
                        .button(t("recipe.apply"))
                        .on_hover_text(t("recipe.apply_hint"))
                        .clicked();
                    if tab.recipe.iter().any(|s| s.pending.is_some()) {
                        choose_key = ui
                            .button(t("recipe.choose_key"))
                            .on_hover_text(t("recipe.choose_key_hint"))
                            .clicked();
                    }
                    let b = ui
                        .add_enabled(!tab.recipe.is_empty(), egui::Button::new(t("recipe.clear")));
                    clear = if tab.recipe.is_empty() {
                        b.on_disabled_hover_text(t("recipe.clear_disabled"));
                        false
                    } else {
                        b.on_hover_text(t("recipe.clear_hint")).clicked()
                    };
                });
                ui.separator();
                if tab.recipe.is_empty() {
                    ui.label(
                        RichText::new(t("recipe.empty")).color(ui.visuals().weak_text_color()),
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("recipe_steps")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, s) in tab.recipe.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut s.enabled, "")
                                    .on_hover_text(t("recipe.step_enabled_hint"));
                                ui.label(format!("{}.", i + 1));
                                if ui
                                    .small_button("x")
                                    .on_hover_text(t("recipe.remove_hint"))
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                                ui.add(egui::Label::new(s.step.describe()).wrap());
                                if s.pending.is_some() {
                                    ui.label(
                                        RichText::new(t("recipe.needs_key"))
                                            .color(ui.visuals().warn_fg_color),
                                    )
                                    .on_hover_text(t("recipe.choose_key_hint"));
                                }
                            });
                        }
                    });
            });
        let tab = &mut self.tabs[self.active_tab];
        if let Some(i) = remove {
            tab.recipe.remove(i);
        }
        if clear {
            tab.recipe.clear();
            tab.recipe_undone.clear();
        }
        if close {
            self.recipe_panel_visible = false;
        }
        if choose_key {
            self.open_recipe_key_dialog(false);
        }
        if save {
            self.save_recipe_as();
        }
        if apply {
            self.pick_and_apply_recipe();
        }
    }
}

/// The Apply recipe dialog: the steps with a warning beside any whose columns
/// this tab does not have, then Apply; afterwards, what ran and what was
/// skipped and why.
pub(crate) fn render_apply_recipe_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.apply_recipe_dialog.take() else {
        return;
    };
    let mut close = false;
    let mut run = false;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let readonly = app.is_readonly();
    let has_table = app.tabs[app.active_tab].table.col_count() > 0;

    let dialog_id = egui::Id::new("octa_apply_recipe_dialog");
    let window = egui::Window::new("octa_apply_recipe")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(380.0)
            .min_width(360.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("apply_recipe_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("recipe.apply_title")).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::Panel::bottom("apply_recipe_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if st.outcome.is_none() {
                        let ok = !readonly && has_table && !st.recipe.steps.is_empty();
                        let b = ui.add_enabled(ok, egui::Button::new(t("recipe.apply_run")));
                        if ok {
                            run = b.on_hover_text(t("recipe.apply_run_hint")).clicked();
                        } else {
                            let why = if readonly {
                                "recipe.apply_readonly"
                            } else if !has_table {
                                "recipe.apply_no_table"
                            } else {
                                "recipe.apply_no_steps"
                            };
                            b.on_disabled_hover_text(t(why));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if st.outcome.is_some() {
                            t("recipe.close")
                        } else {
                            t("common.cancel")
                        };
                        if ui.button(label).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(st.path.display().to_string()).color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            let table = &app.tabs[app.active_tab].table;
            egui::ScrollArea::vertical()
                .id_salt("apply_recipe_steps")
                .auto_shrink([false, true])
                .show(ui, |ui| match &st.outcome {
                    None => {
                        ui.label(t("recipe.apply_intro"));
                        ui.add_space(4.0);
                        for (i, step) in st.recipe.steps.iter().enumerate() {
                            ui.label(format!("{}. {}", i + 1, step.describe()));
                            let missing = step.missing_columns(table);
                            if !missing.is_empty() {
                                ui.label(
                                    RichText::new(
                                        t("recipe.missing_columns")
                                            .replace("{columns}", &missing.join(", ")),
                                    )
                                    .color(ui.visuals().warn_fg_color),
                                );
                            }
                        }
                    }
                    Some(outcome) => {
                        let ran = outcome.iter().filter(|o| o.error.is_none()).count();
                        draw_result_message(
                            ui,
                            ran == outcome.len(),
                            &t("recipe.applied")
                                .replace("{ran}", &ran.to_string())
                                .replace("{total}", &outcome.len().to_string()),
                        );
                        ui.add_space(4.0);
                        for (i, o) in outcome.iter().enumerate() {
                            match &o.error {
                                None => {
                                    ui.label(format!("{}. {}", i + 1, o.description));
                                }
                                Some(e) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}. {}: {}",
                                            i + 1,
                                            o.description,
                                            t("recipe.skipped").replace("{reason}", e)
                                        ))
                                        .color(ui.visuals().warn_fg_color),
                                    );
                                }
                            }
                        }
                    }
                });
        });
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        let recipe = st.recipe.clone();
        st.outcome = Some(app.run_apply_recipe(&recipe));
    }
    if !close {
        app.apply_recipe_dialog = Some(st);
    }
}

/// "Which column identifies a row?": asked when hand edits were recorded on a
/// table with no plain ID column. Explains why, shows which columns tell
/// every row apart, and warns when the chosen ones do not.
pub(crate) fn render_recipe_key_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.recipe_key_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(st.tab) else {
        return;
    };
    let table = &tab.table;
    let labels: Vec<String> = (0..table.col_count())
        .map(|c| {
            let name = &table.columns[c].name;
            if octa::data::recipe::duplicate_key_rows(table, &[c]) == 0 {
                format!("{name}  ({})", t("recipe.key_unique"))
            } else {
                name.clone()
            }
        })
        .collect();
    let dupes = if st.cols.is_empty() {
        0
    } else {
        octa::data::recipe::duplicate_key_rows(table, &st.cols)
    };
    let (mut close, mut done) = (false, false);
    let dialog_id = egui::Id::new("octa_recipe_key_dialog");
    let mut size = DialogSize::Normal;
    let window = egui::Window::new("octa_recipe_key")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(400.0)
            .min_width(340.0)
            .min_height(240.0)
    });
    let inner = window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("recipe.key_title")).strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if draw_window_controls(ui, &mut size) {
                    close = true;
                }
            });
        });
        ui.add_space(4.0);
        ui.label(t("recipe.key_explain"));
        ui.add_space(8.0);
        super::dialogs::widgets::multi_col_picker(ui, "recipe_key_cols", &mut st.cols, &labels);
        if dupes > 0 {
            ui.add_space(4.0);
            ui.label(
                RichText::new(t("recipe.key_duplicates").replace("{n}", &dupes.to_string()))
                    .color(ui.visuals().warn_fg_color),
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ok = !st.cols.is_empty();
            let b = ui.add_enabled(ok, egui::Button::new(t("recipe.key_use")));
            if ok {
                done = b.on_hover_text(t("recipe.key_use_hint")).clicked();
            } else {
                b.on_disabled_hover_text(t("recipe.key_need_one"));
            }
            if ui.button(t("common.cancel")).clicked() {
                close = true;
            }
        });
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    if done {
        let key: Vec<String> = st
            .cols
            .iter()
            .filter_map(|&c| {
                app.tabs[st.tab]
                    .table
                    .columns
                    .get(c)
                    .map(|ci| ci.name.clone())
            })
            .collect();
        app.set_recipe_key(st.tab, key);
        if st.then_save && st.tab == app.active_tab {
            app.save_recipe_as();
        }
        return;
    }
    if !close {
        app.recipe_key_dialog = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use octa::data::recipe::{RecipeStep, SetCells};
    use octa::data::{CellValue, ColumnInfo, DataTable};

    use super::TabState;

    fn tab(headers: &[&str], rows: &[&[&str]]) -> TabState {
        let mut t = TabState::new(octa::data::SearchMode::Plain);
        let mut d = DataTable::empty();
        d.columns = headers
            .iter()
            .map(|h| ColumnInfo {
                name: h.to_string(),
                data_type: "Utf8".into(),
            })
            .collect();
        d.rows = rows
            .iter()
            .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
            .collect();
        t.table = d;
        t
    }

    fn set_cells(t: &TabState, i: usize) -> SetCells {
        match &t.recipe[i].step {
            RecipeStep::SetCells(s) => s.clone(),
            other => panic!("expected set_cells, got {other:?}"),
        }
    }

    #[test]
    fn a_typed_value_is_recorded_against_the_rows_id() {
        let mut t = tab(&["name", "id"], &[&["a", "7"], &["b", "8"]]);
        t.table.set(1, 0, CellValue::String("B".into()));
        assert!(t.record_hand_edits());
        let s = set_cells(&t, 0);
        assert_eq!(s.key, ["id"]);
        assert_eq!(s.cells[0].row, ["8"]);
        assert_eq!(s.cells[0].column, "name");
        assert_eq!(s.cells[0].value.as_deref(), Some("B"));
        // Nothing new: nothing recorded twice.
        assert!(!t.record_hand_edits());
    }

    #[test]
    fn editing_the_id_itself_keeps_the_old_id_to_find_the_row() {
        let mut t = tab(&["id", "v"], &[&["7", "x"]]);
        t.table.set(0, 0, CellValue::String("70".into()));
        t.record_hand_edits();
        assert_eq!(set_cells(&t, 0).cells[0].row, ["7"]);
    }

    #[test]
    fn without_an_id_the_edit_waits_and_resolves_when_one_is_chosen() {
        let mut t = tab(&["city", "price"], &[&["Oslo", "1"], &["Oslo", "2"]]);
        t.table.set(1, 1, CellValue::String("9".into()));
        t.record_hand_edits();
        assert!(t.recipe[0].pending.is_some());
        assert!(
            t.recipe_to_save().steps.is_empty(),
            "pending edits are not saved"
        );
    }

    #[test]
    fn undo_and_redo_move_the_edit_without_recording_it_again() {
        let mut t = tab(&["id", "v"], &[&["1", "x"]]);
        t.table.set(0, 1, CellValue::String("y".into()));
        t.record_hand_edits();
        t.table.undo();
        t.recipe_after_undo();
        assert!(t.recipe.is_empty());
        t.table.redo();
        t.recipe_after_redo();
        assert!(!t.record_hand_edits());
        assert_eq!(t.recipe.len(), 1);
    }

    #[test]
    fn cells_filled_by_an_inserted_column_are_not_typed_values() {
        let mut t = tab(&["id"], &[&["1"]]);
        t.table.insert_column(1, "new".into(), "Utf8".into());
        t.table.set(0, 1, CellValue::String("f".into()));
        assert!(!t.record_hand_edits());
        assert!(t.recipe.is_empty());
    }
}
