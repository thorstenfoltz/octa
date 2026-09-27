//! Re-type one column after load (**Columns -> Change type...**).
//!
//! The dialog is the slow door onto [`octa::data::retype`]: pick a column and
//! a target, read how many values convert and which ones will not, then
//! apply. The fast door is the column header's **Change type** submenu, which
//! applies straight away when everything converts and raises this dialog when
//! something does not, so the two can never disagree about the rules: both
//! end in [`OctaApp::run_retype`].
//!
//! Applying is one entry on the undo stack (the engine pushes it), and the
//! values that kept their text land in `TabState::retype_kept_as_text`, so
//! F10 walks them like any other flagged cell.

use eframe::egui;
use egui::RichText;

use octa::data::retype::{RetypeOutcome, TargetType, preview_retype};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, RetypeState};

/// How many failing values the dialog lists. The engine samples up to
/// `retype::FAILURE_SAMPLE`; this is the shorter list a person actually
/// reads, and the count beside it is always the true total.
const LISTED_FAILURES: usize = 10;

/// Apply is live only when the conversion would do something and the session
/// is writable. The disabled state carries a hover saying which of the two
/// reasons applies.
pub(crate) fn apply_enabled(convertible: usize, readonly: bool) -> bool {
    convertible > 0 && !readonly
}

impl OctaApp {
    /// The fast path from the header submenu: convert straight away when
    /// every value converts, and raise the dialog when one does not, rather
    /// than silently half-converting a column the user clicked through in
    /// one gesture.
    pub(crate) fn retype_or_prompt(&mut self, col: usize, target: TargetType) {
        if self.is_readonly() {
            self.status_message = Some((t("retype.readonly"), std::time::Instant::now()));
            return;
        }
        if col >= self.tabs[self.active_tab].table.columns.len() {
            return;
        }
        let preview = preview_retype(&self.tabs[self.active_tab].table, col, target);
        if preview.failed == 0 {
            self.run_retype(col, target);
        } else {
            self.retype_dialog = Some(RetypeState::new(col, target));
        }
    }

    /// Convert the column and report it. The single place either door ends
    /// in, so the instant path and the dialog cannot drift apart.
    pub(crate) fn run_retype(&mut self, col: usize, target: TargetType) -> RetypeOutcome {
        self.run_retype_with(col, target, false)
    }

    /// [`Self::run_retype`] with the strictness chosen explicitly.
    pub(crate) fn run_retype_with(
        &mut self,
        col: usize,
        target: TargetType,
        strict: bool,
    ) -> RetypeOutcome {
        if self.is_readonly() {
            self.status_message = Some((t("retype.readonly"), std::time::Instant::now()));
            return RetypeOutcome::default();
        }
        let strictness = if strict {
            octa::data::retype::Strictness::Strict
        } else {
            octa::data::retype::Strictness::Mixed
        };
        let tab = &mut self.tabs[self.active_tab];
        let outcome =
            octa::data::retype::apply_retype_with(&mut tab.table, col, target, strictness);
        if outcome.refused.is_some() {
            // Refused means untouched, so nothing downstream is dirty.
            return outcome;
        }
        let column = tab.table.columns[col].name.clone();
        // Replaced, not extended: these coordinates describe this conversion
        // only, so a later undo cannot leave phantom problem cells behind.
        tab.retype_kept_as_text = outcome.kept_as_text.iter().copied().collect();
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;

        self.record_step(octa::data::recipe::RecipeStep::ChangeType(
            octa::data::recipe::ChangeType::new(column, target, strict),
        ));
        let kept = outcome.kept_as_text.len();
        let message = if kept == 0 {
            t("retype.done").replace("{count}", &outcome.converted.to_string())
        } else {
            t("retype.done_partial")
                .replace("{count}", &outcome.converted.to_string())
                .replace("{kept}", &kept.to_string())
        };
        self.status_message = Some((message, std::time::Instant::now()));
        outcome
    }
}

pub(crate) fn render_retype_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.retype_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // Close silently if the table has no columns to re-type.
    if col_names.is_empty() {
        app.retype_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.retype_dialog.take().unwrap();

    // The column may have gone while the dialog was open.
    if st.col >= col_names.len() {
        st.col = 0;
        st.preview_key = None;
    }

    let readonly = app.is_readonly();
    let table = &app.tabs[app.active_tab].table;
    let partial = table.partial_note();
    let current_type = table.columns[st.col].data_type.clone();

    // Recompute the preview only when what it describes has changed: a date
    // preview parses every value under seven layouts.
    let key = (
        st.col,
        st.target_idx,
        table.row_count(),
        // Not `edits.len()` alone: replacing one edit with another leaves the
        // map the same size, and every edit pushes an undo entry.
        table.undo_stack.len(),
    );
    if st.preview_key != Some(key) {
        st.preview = Some(preview_retype(table, st.col, st.target()));
        st.preview_key = Some(key);
    }
    let preview = st.preview.clone().unwrap_or_default();

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_retype_dialog");
    let window = egui::Window::new("octa_retype")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(340.0)
            .min_width(340.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("retype_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("retype.title")).strong().size(16.0));
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

        egui::Panel::bottom("retype_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(err) = &st.error {
                        ui.add_space(4.0);
                        let colour = ui.visuals().error_fg_color;
                        octa::ui::message::selectable_message(ui, colour, err);
                    }
                    let enabled = apply_enabled(preview.convertible, readonly);
                    let btn = ui.add_enabled(enabled, egui::Button::new(t("retype.apply")));
                    let btn = if enabled {
                        btn.on_hover_text(t("retype.apply_hint"))
                    } else if readonly {
                        btn.on_disabled_hover_text(t("retype.readonly"))
                    } else {
                        btn.on_disabled_hover_text(t("retype.apply_nothing_hint"))
                    };
                    if btn.clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(t("retype.cancel"))
                            .on_hover_text(t("retype.cancel_hint"))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("retype.column_label")).strong());
                let col_text = col_names.get(st.col).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt("retype_col")
                    .selected_text(col_text)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in col_names.iter().enumerate() {
                            if ui.selectable_label(st.col == i, name).clicked() {
                                st.col = i;
                            }
                        }
                    })
                    .response
                    .on_hover_text(t("retype.column_hint"));
            });
            ui.weak(t("retype.current_type").replace("{type}", &current_type));

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new(t("retype.target_label")).strong());
                egui::ComboBox::from_id_salt("retype_target")
                    .selected_text(t(st.target().i18n_key()))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (idx, &target) in TargetType::ALL.iter().enumerate() {
                            if ui
                                .selectable_label(st.target_idx == idx, t(target.i18n_key()))
                                .clicked()
                            {
                                st.target_idx = idx;
                            }
                        }
                    })
                    .response
                    .on_hover_text(t("retype.target_hint"));
            });

            ui.add_space(6.0);
            ui.checkbox(&mut st.strict, t("retype.strict"))
                .on_hover_text(t("retype.strict_hint"));

            ui.add_space(8.0);
            ui.separator();

            ui.label(
                RichText::new(
                    t("retype.summary")
                        .replace("{ok}", &preview.convertible.to_string())
                        .replace("{total}", &preview.total.to_string())
                        .replace("{bad}", &preview.failed.to_string()),
                )
                .strong(),
            );
            if let Some((loaded, known_total)) = partial {
                octa::ui::message::partial_note(ui, loaded, known_total);
            }

            if preview.failed > 0 {
                ui.add_space(4.0);
                ui.weak(t("retype.keeps_text_note"));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (row, value) in preview.failures.iter().take(LISTED_FAILURES) {
                            ui.label(
                                t("retype.failure_row")
                                    .replace("{row}", &(row + 1).to_string())
                                    .replace("{value}", value),
                            );
                        }
                        if preview.failed > LISTED_FAILURES {
                            ui.weak(t("retype.more_failures").replace(
                                "{count}",
                                &(preview.failed - LISTED_FAILURES).to_string(),
                            ));
                        }
                    });
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if apply {
        let outcome = app.run_retype_with(st.col, st.target(), st.strict);
        if let Some(blocked) = outcome.refused {
            // Nothing changed, so the dialog stays open with the reason
            // rather than closing on a conversion that did not happen.
            st.error = Some(t("retype.refused").replace("{count}", &blocked.to_string()));
            app.retype_dialog = Some(st);
            return;
        }
        return;
    }
    if !close {
        app.retype_dialog = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog must not offer Apply on a table whose preview says nothing
    /// converts: that is a user error worth stopping, not a silent no-op.
    #[test]
    fn apply_is_offered_only_when_something_converts() {
        assert!(!apply_enabled(0, false), "nothing converts");
        assert!(apply_enabled(3, false), "some convert");
        assert!(!apply_enabled(3, true), "read-only mode always refuses");
    }

    /// Every target must have a locale key, or a chooser entry renders as its
    /// own key name. `TargetType::ALL` is the chooser's source of truth, so
    /// walking it is what catches a variant added without a label.
    #[test]
    fn every_target_has_a_label_key() {
        for &target in TargetType::ALL {
            let key = target.i18n_key();
            assert!(
                key.starts_with("retype.type_"),
                "{target:?} has no label key"
            );
        }
    }
}
