//! The Tolerance block shared by the compare view (Ordered / Join) and the
//! Compare with database dialog, so both treat "close enough" the same way.
//! The fields are text (never a drag widget): what the user typed stays
//! as typed, and `options` reads it, either decimal mark accepted.

use eframe::egui;
use octa::data::compare::CompareOptions;
use octa::i18n::t;

/// What the user typed, kept between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToleranceDraft {
    pub abs: String,
    pub pct: String,
    pub secs: String,
    pub ignore_case: bool,
    pub ignore_spaces: bool,
    /// Indices into the left table's columns.
    pub ignore_cols: Vec<usize>,
}

impl ToleranceDraft {
    /// The options this draft stands for. A field that is empty, not a
    /// number, or negative means "exact" for that kind of value.
    pub fn options(&self, columns: &[String]) -> CompareOptions {
        let num =
            |s: &str| octa::data::num_parse::parse_number_relaxed(s.trim()).filter(|v| *v >= 0.0);
        CompareOptions {
            number_abs: num(&self.abs),
            number_pct: num(&self.pct),
            seconds: num(&self.secs),
            ignore_case: self.ignore_case,
            ignore_spaces: self.ignore_spaces,
            ignore_columns: self
                .ignore_cols
                .iter()
                .filter_map(|&i| columns.get(i).cloned())
                .collect(),
        }
    }
}

/// The collapsible Tolerance block. `id` keeps two of them apart.
pub fn tolerance_controls(
    ui: &mut egui::Ui,
    id: &str,
    draft: &mut ToleranceDraft,
    columns: &[String],
) {
    let header = egui::CollapsingHeader::new(t("cmptol.title"))
        .id_salt(id)
        .show(ui, |ui| {
            octa::ui::control_row::control_row(ui, |ui| {
                ui.label(t("cmptol.numbers"))
                    .on_hover_text(t("cmptol.numbers_hint"));
                octa::ui::control_row::control_text_edit(
                    ui,
                    70.0,
                    egui::TextEdit::singleline(&mut draft.abs).hint_text("0.01"),
                )
                .on_hover_text(t("cmptol.abs_hint"));
                ui.label(t("cmptol.or"));
                octa::ui::control_row::control_text_edit(
                    ui,
                    50.0,
                    egui::TextEdit::singleline(&mut draft.pct).hint_text("1"),
                )
                .on_hover_text(t("cmptol.pct_hint"));
                ui.label("%");
                ui.add_space(12.0);
                ui.label(t("cmptol.times"))
                    .on_hover_text(t("cmptol.times_hint"));
                octa::ui::control_row::control_text_edit(
                    ui,
                    60.0,
                    egui::TextEdit::singleline(&mut draft.secs).hint_text("60"),
                )
                .on_hover_text(t("cmptol.secs_hint"));
                ui.label(t("cmptol.seconds"));
            });
            octa::ui::control_row::control_row(ui, |ui| {
                ui.checkbox(&mut draft.ignore_case, t("cmptol.ignore_case"))
                    .on_hover_text(t("cmptol.ignore_case_hint"));
                ui.checkbox(&mut draft.ignore_spaces, t("cmptol.ignore_spaces"))
                    .on_hover_text(t("cmptol.ignore_spaces_hint"));
            });
            ui.label(t("cmptol.ignore_columns"))
                .on_hover_text(t("cmptol.ignore_columns_hint"));
            crate::app::dialogs::widgets::multi_col_picker(
                ui,
                &format!("{id}_ignore"),
                &mut draft.ignore_cols,
                columns,
            );
        });
    header.header_response.on_hover_text(t("cmptol.title_hint"));
}

#[cfg(test)]
#[path = "tolerance_tests.rs"]
mod tests;
