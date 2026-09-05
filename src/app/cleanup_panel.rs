//! Clean-up suggestions panel: a bottom dock listing what is wrong with the
//! open table, one Apply button per problem.
//!
//! Modelled on `src/app/multi_search.rs`: the scan runs on a worker thread
//! holding a table snapshot, a cloned `egui::Context` and shared `Arc` slots
//! the UI polls per frame.
//!
//! The scan is expensive (several detection passes), so it is tied to the one
//! gesture that means the user wants it: **opening the panel**. Nothing runs
//! while the panel is closed, which is why the feature needs no setting to gate
//! it and no Scan button to defer it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::data::cleanup::{CleanupLimits, Severity, Suggestion, suggest_cleanups};
use octa::i18n::t;

use super::file_io::resync_db_meta_baseline;
use super::state::OctaApp;
use crate::ui;

/// Per-app state for the clean-up panel. One scan at a time, app-level like
/// the SQL and chat panels.
pub(crate) struct CleanupPanelState {
    pub(crate) visible: bool,
    /// Suggestions from the last completed scan.
    pub(crate) results: Arc<Mutex<Vec<Suggestion>>>,
    /// Set while a scan is in flight; drives Scan vs Cancel.
    pub(crate) running: Arc<AtomicBool>,
    /// Flipped by Cancel; the engine polls it between checks.
    pub(crate) cancel: Arc<AtomicBool>,
    /// True once a scan has completed, so an empty list can read as
    /// "nothing found" rather than "not scanned yet".
    pub(crate) scanned: bool,
    /// True when the last scan hit the row cap.
    pub(crate) capped: bool,
    /// Suggestions the user dismissed this session, by list position at the
    /// time of the scan. Session-only, never persisted.
    pub(crate) ignored: Vec<usize>,
    /// Suggestions whose examples are currently shown, by list position.
    pub(crate) expanded: HashSet<usize>,
}

impl Default for CleanupPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            results: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            scanned: false,
            capped: false,
            ignored: Vec::new(),
            expanded: HashSet::new(),
        }
    }
}

impl OctaApp {
    /// Kick off a scan of the active table on a worker thread.
    pub(crate) fn start_cleanup_scan(&mut self, ctx: &egui::Context) {
        if self.cleanup_panel.running.load(Ordering::Relaxed) {
            return;
        }
        let mut snapshot = self.tabs[self.active_tab].table.clone();
        snapshot.apply_edits();
        let limits = CleanupLimits::default();

        self.cleanup_panel.capped = snapshot.row_count() > limits.max_rows;
        self.cleanup_panel.ignored.clear();
        self.cleanup_panel.expanded.clear();
        self.cleanup_panel.scanned = true;
        self.cleanup_panel.cancel.store(false, Ordering::Relaxed);
        self.cleanup_panel.running.store(true, Ordering::Relaxed);
        if let Ok(mut slot) = self.cleanup_panel.results.lock() {
            slot.clear();
        }

        let results = Arc::clone(&self.cleanup_panel.results);
        let running = Arc::clone(&self.cleanup_panel.running);
        let cancel = Arc::clone(&self.cleanup_panel.cancel);
        let ctx = ctx.clone();

        // Detached on purpose: the thread owns its snapshot and only writes
        // through the shared slots, so there is nothing to join.
        std::thread::spawn(move || {
            // A panic in the scan would otherwise leave `running` true for the
            // rest of the session: the panel would spin "Scanning..." forever
            // and `start_cleanup_scan` would refuse to try again.
            let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
            let found = suggest_cleanups(&snapshot, &limits, &cancel);
            if let Ok(mut slot) = results.lock() {
                *slot = found;
            }
            ctx.request_repaint();
        });
    }

    /// Open or close the panel. Opening clears `scanned`, which is what makes
    /// the next frame start a fresh scan: the single place either the menu
    /// entry or the shortcut goes through, so the two cannot drift.
    pub(crate) fn toggle_cleanup_panel(&mut self) {
        self.cleanup_panel.visible = !self.cleanup_panel.visible;
        if self.cleanup_panel.visible {
            self.cleanup_panel.scanned = false;
        }
    }

    /// Render the docked clean-up panel. Returns early when not visible so
    /// calling it every frame is cheap.
    ///
    /// Opening the panel is what triggers the scan; there is no Scan button.
    /// Nothing here runs while the panel is closed, which is the whole reason
    /// the feature can be always-on.
    pub(crate) fn render_cleanup_panel(&mut self, parent_ui: &mut egui::Ui) {
        if !self.cleanup_panel.visible {
            return;
        }
        let ctx = parent_ui.ctx().clone();
        if !self.cleanup_panel.scanned && !self.cleanup_panel.running.load(Ordering::Relaxed) {
            self.start_cleanup_scan(&ctx);
        }
        // Leave the table something to live in once the window is small;
        // `panel_fit::clamp` is a no-op on a normal window.
        let (default_size, min_size) =
            octa::ui::panel_fit::clamp(parent_ui.available_height(), 220.0, 140.0);
        egui::Panel::bottom("cleanup_panel")
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .show(parent_ui, |ui| {
                self.draw_cleanup_header(ui);
                ui.separator();
                self.draw_cleanup_results(ui);
            });
    }

    fn draw_cleanup_header(&mut self, ui: &mut egui::Ui) {
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let running = self.cleanup_panel.running.load(Ordering::Relaxed);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(t("cleanup.title"))
                    .strong()
                    .color(colors.text_primary),
            );
            ui.separator();
            if running {
                ui.spinner();
                ui.label(t("cleanup.scanning"));
                if ui.button(t("cleanup.cancel")).clicked() {
                    self.cleanup_panel.cancel.store(true, Ordering::Relaxed);
                }
            }
            if self.cleanup_panel.capped {
                ui.weak(
                    t("cleanup.capped")
                        .replace("{n}", &CleanupLimits::default().max_rows.to_string()),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("x").clicked() {
                    self.cleanup_panel.visible = false;
                }
            });
        });
    }

    fn draw_cleanup_results(&mut self, ui: &mut egui::Ui) {
        let running = self.cleanup_panel.running.load(Ordering::Relaxed);
        let found = match self.cleanup_panel.results.lock() {
            Ok(slot) => slot.clone(),
            Err(_) => Vec::new(),
        };

        if found.is_empty() {
            ui.add_space(8.0);
            ui.label(if running {
                t("cleanup.scanning")
            } else {
                t("cleanup.none")
            });
            return;
        }

        let labels: Vec<String> = found.iter().map(|s| self.cleanup_column_label(s)).collect();
        let readonly = self.is_readonly();
        let mut apply: Option<usize> = None;
        let mut ignore: Option<usize> = None;
        let mut toggle_example: Option<usize> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (idx, s) in found.iter().enumerate() {
                    if self.cleanup_panel.ignored.contains(&idx) {
                        continue;
                    }
                    let shown = self.cleanup_panel.expanded.contains(&idx);
                    ui.horizontal(|ui| {
                        ui.label(severity_label(s.severity));
                        ui.strong(&labels[idx]);
                        ui.label(describe(s));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t("cleanup.ignore")).clicked() {
                                ignore = Some(idx);
                            }
                            if ui
                                .add_enabled(
                                    !readonly,
                                    egui::Button::new(t("cleanup.apply")).small(),
                                )
                                .clicked()
                            {
                                apply = Some(idx);
                            }
                            // Only offered when there is something to show:
                            // an empty cell and an empty column have no value
                            // worth printing.
                            if !s.examples.is_empty()
                                && ui
                                    .small_button(if shown {
                                        t("cleanup.hide")
                                    } else {
                                        t("cleanup.show")
                                    })
                                    .clicked()
                            {
                                toggle_example = Some(idx);
                            }
                        });
                    });
                    // What Apply would actually do to this table, named
                    // column by column. Always visible: a button whose effect
                    // you have to click to discover is not a fix, it is a
                    // gamble, and the direct-fix / opens-a-dialog split is the
                    // whole design of the panel.
                    ui.indent(("cleanup_action", idx), |ui| {
                        ui.weak(describe_action(s, &labels[idx]));
                    });
                    if shown {
                        ui.indent(("cleanup_example", idx), |ui| {
                            for example in &s.examples {
                                ui.weak(example);
                            }
                        });
                    }
                    ui.separator();
                }
            });

        if let Some(idx) = toggle_example
            && !self.cleanup_panel.expanded.remove(&idx)
        {
            self.cleanup_panel.expanded.insert(idx);
        }
        if let Some(idx) = ignore {
            self.cleanup_panel.ignored.push(idx);
        }
        if let Some(idx) = apply
            && let Some(s) = found.get(idx)
        {
            if self.apply_cleanup(s.clone()) {
                // The table changed, so every remaining suggestion (and every
                // column index in it) may now be stale. Rescan rather than act
                // on a list that no longer describes the table.
                self.cleanup_panel.scanned = false;
            } else {
                // A dialog opened instead; nothing changed yet, so just stop
                // the row from nagging.
                self.cleanup_panel.ignored.push(idx);
            }
        }
    }

    /// Column name for a suggestion, or the whole-table label.
    fn cleanup_column_label(&self, s: &Suggestion) -> String {
        match s.column {
            None => t("cleanup.whole_table"),
            Some(c) => self.tabs[self.active_tab]
                .table
                .columns
                .get(c)
                .map(|col| col.name.clone())
                .unwrap_or_default(),
        }
    }

    /// Apply one suggestion. Returns whether the table was actually changed,
    /// so the caller knows whether the remaining suggestions are still valid.
    ///
    /// Two classes, and the split is the whole point of the feature. An
    /// unambiguous fix runs here, through the same undoable path the manual
    /// operation uses, and returns `true`. A fix that needs a choice from the
    /// user opens the existing dialog pre-filled and returns `false`; nothing
    /// is reimplemented and nothing has changed yet.
    pub(crate) fn apply_cleanup(&mut self, s: Suggestion) -> bool {
        use octa::data::cleanup::CleanupKind as K;
        if self.is_readonly() {
            return false;
        }
        let changed = !matches!(
            s.kind,
            K::MissingValues | K::Outliers | K::PersonalData { .. } | K::UnitsOrCurrency { .. }
        );
        match s.kind {
            K::TrimWhitespace => {
                if let Some(col) = s.column {
                    self.cleanup_apply_trim(col);
                }
            }
            K::UntidyHeaders => self.cleanup_apply_clean_headers(),
            K::Mojibake => {
                if let Some(col) = s.column {
                    self.cleanup_apply_mojibake(col);
                }
            }
            K::EmptyColumn | K::ConstantColumn => {
                if let Some(col) = s.column {
                    self.cleanup_apply_drop_column(col);
                }
            }
            K::TypeMismatch { ref target } => {
                if let Some(col) = s.column {
                    self.cleanup_apply_cast(col, target);
                }
            }
            K::DuplicateRows => {
                let cols = self.tabs[self.active_tab].table.col_count();
                super::dialogs::dedupe::apply_dedupe(
                    self,
                    super::state::DedupeState::new_all_cols(cols),
                );
            }
            // A unit split is a question, not a fix: which columns to add is
            // the user's call, so this opens its dialog with a live preview.
            K::UnitsOrCurrency { .. } => {
                if let Some(col) = s.column {
                    self.open_units_dialog(col);
                }
            }
            // These three need a decision the panel cannot make for the user,
            // so they open the matching dialog with the column pre-filled.
            K::MissingValues => {
                self.impute_dialog = Some(super::state::ImputeState {
                    col: s.column.unwrap_or(0),
                    ..Default::default()
                });
            }
            K::Outliers => {
                let mut st =
                    super::state::OutlierState::for_table(&self.tabs[self.active_tab].table);
                if let Some(col) = s.column {
                    // Tick only the column this suggestion is about.
                    for (i, sel) in st.col_selected.iter_mut().enumerate() {
                        *sel = i == col;
                    }
                }
                self.outlier_dialog = Some(st);
            }
            K::PersonalData { .. } => {
                let mut st = super::state::AnonymizeState::default();
                if let (Some(col), Some(rule)) = (s.column, st.rules.first_mut()) {
                    rule.columns.clear();
                    rule.columns.insert(col);
                }
                self.anonymize_dialog = Some(st);
            }
        }
        self.tabs[self.active_tab].filter_dirty = true;
        if changed {
            self.status_message = Some((t("cleanup.applied"), std::time::Instant::now()));
        }
        changed
    }

    /// Trim every string cell in one column, through `DataTable::set` so the
    /// edits land on the undo stack, coalesced into a single Ctrl+Z step.
    /// Repair garbled characters in one column.
    ///
    /// Mirrors `cleanup_apply_trim`: collect first, then write, then coalesce
    /// the whole column into one undo entry so a single Ctrl+Z reverts it.
    /// `mojibake::repair` returns `None` for anything it cannot prove, so
    /// untouched cells are genuinely untouched.
    fn cleanup_apply_mojibake(&mut self, col: usize) {
        let tab = &mut self.tabs[self.active_tab];
        let start = tab.table.undo_stack.len();
        let fixes: Vec<(usize, String)> = (0..tab.table.row_count())
            .filter_map(|row| match tab.table.get(row, col) {
                Some(octa::data::CellValue::String(v)) => {
                    octa::data::mojibake::repair(v).map(|fixed| (row, fixed))
                }
                _ => None,
            })
            .collect();
        for (row, value) in fixes {
            tab.table
                .set(row, col, octa::data::CellValue::String(value));
        }
        tab.table.coalesce_undo_since(start);
        resync_db_meta_baseline(tab);
    }

    fn cleanup_apply_trim(&mut self, col: usize) {
        let tab = &mut self.tabs[self.active_tab];
        let start = tab.table.undo_stack.len();
        let trimmed: Vec<(usize, String)> = (0..tab.table.row_count())
            .filter_map(|row| match tab.table.get(row, col) {
                Some(octa::data::CellValue::String(v)) if v.trim() != v.as_str() => {
                    Some((row, v.trim().to_string()))
                }
                _ => None,
            })
            .collect();
        for (row, value) in trimmed {
            tab.table
                .set(row, col, octa::data::CellValue::String(value));
        }
        tab.table.coalesce_undo_since(start);
        resync_db_meta_baseline(tab);
    }

    /// Snake_case every column title, through `DataTable::rename_column` so the
    /// renames are undoable as one step. The names come from
    /// `trim::planned_header_names`, the same rule the load-time pass uses.
    fn cleanup_apply_clean_headers(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let planned = octa::data::trim::planned_header_names(&tab.table);
        let start = tab.table.undo_stack.len();
        for (idx, name) in planned.into_iter().enumerate() {
            if tab.table.columns.get(idx).is_some_and(|c| c.name != name) {
                tab.table.rename_column(idx, name);
            }
        }
        tab.table.coalesce_undo_since(start);
        resync_db_meta_baseline(tab);
        tab.table_state.widths_initialized = false;
    }

    /// Drop one all-empty column, keeping the selection valid afterwards the
    /// same way the Delete columns dialog does.
    fn cleanup_apply_drop_column(&mut self, col: usize) {
        let tab = &mut self.tabs[self.active_tab];
        tab.table.delete_column(col);
        tab.table_state.editing_cell = None;
        if tab.table.col_count() == 0 {
            tab.table_state.selected_cell = None;
        } else if let Some((row, c)) = tab.table_state.selected_cell {
            tab.table_state.selected_cell = Some((row, c.min(tab.table.col_count() - 1)));
        }
        tab.table_state.widths_initialized = false;
        resync_db_meta_baseline(tab);
    }

    /// Cast one column. `convert_column` pushes its own undo entry and refuses
    /// a cast that would lose data, so a failure here is a silent no-op.
    fn cleanup_apply_cast(&mut self, col: usize, target: &str) {
        let tab = &mut self.tabs[self.active_tab];
        tab.table.convert_column(col, target);
        resync_db_meta_baseline(tab);
    }
}

fn severity_label(sev: Severity) -> String {
    match sev {
        Severity::High => t("cleanup.sev_high"),
        Severity::Medium => t("cleanup.sev_medium"),
        Severity::Low => t("cleanup.sev_low"),
    }
}

/// What pressing Apply would do, in the same plain language as `describe`,
/// with the real column name and count filled in.
///
/// Each sentence also says which of the two classes the fix is in: the direct
/// ones end by naming undo, the deferred ones say which dialog opens. The
/// user should never have to click Apply to find out which kind it was.
fn describe_action(s: &Suggestion, column_label: &str) -> String {
    use octa::data::cleanup::CleanupKind as K;
    let key = match &s.kind {
        K::TrimWhitespace => "cleanup.action_trim",
        K::TypeMismatch { .. } => "cleanup.action_type",
        K::DuplicateRows => "cleanup.action_duplicates",
        K::MissingValues => "cleanup.action_missing",
        K::Outliers => "cleanup.action_outliers",
        K::PersonalData { .. } => "cleanup.action_pii",
        K::EmptyColumn => "cleanup.action_empty",
        K::UntidyHeaders => "cleanup.action_headers",
        K::Mojibake => "cleanup.action_mojibake",
        K::UnitsOrCurrency { .. } => "cleanup.action_units",
        K::ConstantColumn => "cleanup.action_constant",
    };
    t(key)
        .replace("{n}", &s.affected.to_string())
        .replace("{col}", column_label)
}

/// The localized sentence for one suggestion, with its count substituted in.
fn describe(s: &Suggestion) -> String {
    use octa::data::cleanup::CleanupKind as K;
    let key = match &s.kind {
        K::TrimWhitespace => "cleanup.kind_trim",
        K::TypeMismatch { .. } => "cleanup.kind_type",
        K::DuplicateRows => "cleanup.kind_duplicates",
        K::MissingValues => "cleanup.kind_missing",
        K::Outliers => "cleanup.kind_outliers",
        K::PersonalData { .. } => "cleanup.kind_pii",
        K::EmptyColumn => "cleanup.kind_empty",
        K::UntidyHeaders => "cleanup.kind_headers",
        K::Mojibake => "cleanup.kind_mojibake",
        K::ConstantColumn => "cleanup.kind_constant",
        K::UnitsOrCurrency { flavour, unit } => {
            // The unit is data, not a translatable word, so it is substituted
            // into the sentence rather than being part of it.
            let key = match flavour.as_str() {
                "magnitude" => "cleanup.kind_units_magnitude",
                "currency" => "cleanup.kind_units_currency",
                "percent" => "cleanup.kind_units_percent",
                _ => "cleanup.kind_units",
            };
            return t(key).replace("{n}", &s.detail).replace("{unit}", unit);
        }
    };
    t(key).replace("{n}", &s.detail)
}
