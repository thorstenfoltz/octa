//! Load-time banners above the table (promoted dates and numbers, trimmed
//! whitespace, unparsed dates, the per-tab notice) and the reverts their
//! Dismiss buttons run.

use eframe::egui;

use octa::data::ViewMode;
use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    pub(super) fn render_load_banners(&mut self, ui: &mut egui::Ui) {
        // A result computed from a table that held only part of its
        // source says so, above the result. Not dismissable: it is not an
        // event that happened once, it is what this tab *is*.
        if let Some((loaded, known_total)) = self.tabs[self.active_tab].partial_source_note {
            octa::ui::message::partial_note(ui, loaded, known_total);
            ui.add_space(4.0);
        }

        // Date format-change banner. Stays visible until the user
        // dismisses it; the inference pass only sets it when the source
        // layout differs from the canonical ISO display.
        let mut dismiss_warning = false;
        let mut keep_dates = false;
        if let Some(warning) = self
            .pending_date_warning
            .as_ref()
            .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
        {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let names: Vec<String> = warning
                .entries
                .iter()
                .map(|e| format!("{} ({})", e.column_name, e.source_label))
                .collect();
            // Bounded, and the row wraps: a wide file can promote two
            // hundred columns, and spelled out in one non-wrapping row
            // that label pushes Okay and Dismiss off the screen edge.
            let summary = ui::message::elide_list(&names, ui::message::BANNER_LIST_MAX);
            let full = names.join(", ");
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        octa::i18n::t("banner.dates_detected").replace("{cols}", &summary),
                    )
                    .color(colors.warning)
                    .size(12.0),
                )
                .on_hover_text(&full);
                // "Okay" accepts the date display and closes the banner;
                // "Dismiss" reverts the promoted columns back to text.
                if ui
                    .small_button(octa::i18n::t("banner.okay"))
                    .on_hover_text(octa::i18n::t("banner.dates_keep_tip"))
                    .clicked()
                {
                    keep_dates = true;
                }
                if ui
                    .small_button(octa::i18n::t("banner.dismiss"))
                    .on_hover_text(octa::i18n::t("banner.dates_revert_tip"))
                    .clicked()
                {
                    dismiss_warning = true;
                }
                ui.label(
                    egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                        .color(colors.text_muted)
                        .size(11.0),
                );
            });
            ui.add_space(4.0);
        }
        if dismiss_warning {
            self.revert_promoted_date_columns();
        } else if keep_dates {
            self.pending_date_warning = None;
        }

        // Near-miss date banner. Lists columns that looked date-shaped but
        // had values that could not be parsed, so they stayed text. This
        // explains *why* a column the user expected to be a date is still
        // text. Dismiss-only - nothing was changed.
        let mut dismiss_date_parse = false;
        if let Some(warning) = self
            .pending_date_parse_warning
            .as_ref()
            .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
        {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let names: Vec<String> = warning
                .entries
                .iter()
                .map(|e| {
                    let samples = e.samples.join(", ");
                    format!(
                        "{} (looks like {}, {} of {} values parsed; e.g. {})",
                        e.column_name, e.source_label, e.parsed, e.total, samples
                    )
                })
                .collect();
            // These entries are long sentences, so the cap is lower.
            let summary = ui::message::elide_list(&names, 2);
            let full = names.join("; ");
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        octa::i18n::t("banner.dates_unparsed").replace("{cols}", &summary),
                    )
                    .color(colors.warning)
                    .size(12.0),
                )
                .on_hover_text(&full);
                if ui
                    .small_button(octa::i18n::t("banner.dismiss"))
                    .on_hover_text(octa::i18n::t("banner.close_tip"))
                    .clicked()
                {
                    dismiss_date_parse = true;
                }
                ui.label(
                    egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                        .color(colors.text_muted)
                        .size(11.0),
                );
            });
            ui.add_space(4.0);
        }
        if dismiss_date_parse {
            self.pending_date_parse_warning = None;
        }

        // Whitespace-trim banner. Lists the columns whose string cells had
        // leading/trailing whitespace stripped on load. Dismiss-only - the
        // trimming itself already happened.
        let mut dismiss_trim = false;
        let mut undo_trim = false;
        if let Some(warning) = self
            .pending_trim_warning
            .as_ref()
            .filter(|w| w.tab_idx == self.active_tab && !w.columns.is_empty())
        {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let summary = ui::message::elide_list(&warning.columns, ui::message::BANNER_LIST_MAX);
            let full = warning.columns.join(", ");
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        octa::i18n::t("banner.trimmed")
                            .replace("{n}", &warning.columns.len().to_string())
                            .replace("{cols}", &summary),
                    )
                    .color(colors.warning)
                    .size(12.0),
                )
                .on_hover_text(&full);
                // "Okay" accepts the trim and closes the banner; "Dismiss"
                // undoes it, restoring the original leading/trailing
                // whitespace.
                if ui
                    .small_button(octa::i18n::t("banner.okay"))
                    .on_hover_text(octa::i18n::t("banner.trim_keep_tip"))
                    .clicked()
                {
                    dismiss_trim = true;
                }
                if ui
                    .small_button(octa::i18n::t("banner.dismiss"))
                    .on_hover_text(octa::i18n::t("banner.trim_undo_tip"))
                    .clicked()
                {
                    undo_trim = true;
                }
                ui.label(
                    egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                        .color(colors.text_muted)
                        .size(11.0),
                );
            });
            ui.add_space(4.0);
        }
        if undo_trim {
            self.revert_trimmed_columns();
        } else if dismiss_trim {
            self.pending_trim_warning = None;
        }

        // Number-promotion banner. Lists the text columns that were read as
        // European- or English-formatted numbers. Okay accepts, Dismiss
        // puts the original strings back.
        let mut dismiss_numbers = false;
        let mut undo_numbers = false;
        if let Some(warning) = self
            .pending_number_warning
            .as_ref()
            .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
        {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let names: Vec<String> = warning
                .entries
                .iter()
                .map(|e| e.column_name.clone())
                .collect();
            let summary = ui::message::elide_list(&names, ui::message::BANNER_LIST_MAX);
            let full = names.join(", ");
            let style_label = warning.entries[0].style_label;
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        octa::i18n::t("banner.numbers_promoted")
                            .replace("{n}", &warning.entries.len().to_string())
                            .replace("{cols}", &summary)
                            .replace("{style}", style_label),
                    )
                    .color(colors.warning)
                    .size(12.0),
                )
                .on_hover_text(&full);
                if ui
                    .small_button(octa::i18n::t("banner.okay"))
                    .on_hover_text(octa::i18n::t("banner.numbers_keep_tip"))
                    .clicked()
                {
                    dismiss_numbers = true;
                }
                if ui
                    .small_button(octa::i18n::t("banner.dismiss"))
                    .on_hover_text(octa::i18n::t("banner.numbers_undo_tip"))
                    .clicked()
                {
                    undo_numbers = true;
                }
            });
            ui.add_space(4.0);
        }
        if undo_numbers {
            self.revert_promoted_number_columns();
        } else if dismiss_numbers {
            self.pending_number_warning = None;
        }

        // Per-tab notice banner (parse fallback, inventory truncation, the
        // file-internals facts). The Raw view renders this itself inside
        // its scroll area, so only the other views need it here - without
        // this, a banner set on a table-shaped tab was simply invisible.
        if self.tabs[self.active_tab].view_mode != ViewMode::Raw
            && let Some(text) = self.tabs[self.active_tab].parse_error_banner.clone()
            && crate::view_modes::raw_text::render_parse_error_banner(ui, &text, self.theme_mode)
        {
            self.tabs[self.active_tab].parse_error_banner = None;
        }
    }

    /// Revert every column that the date inference pass promoted under a
    /// non-canonical layout, restoring the source strings the user saw on
    /// disk and switching the column type back to `Utf8`. Called from the
    /// "Dismiss" button on the date-format-change banner.
    fn revert_promoted_date_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_date_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for entry in &warning.entries {
            if entry.col_idx >= tab.table.col_count() {
                continue;
            }
            for (row, original) in entry.original_values.iter().enumerate() {
                if row >= tab.table.row_count() {
                    break;
                }
                let new_cell = match original {
                    Some(s) => CellValue::String(s.clone()),
                    None => CellValue::Null,
                };
                tab.table.rows[row][entry.col_idx] = new_cell;
            }
            if let Some(col) = tab.table.columns.get_mut(entry.col_idx) {
                col.data_type = "Utf8".to_string();
            }
        }
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }

    /// Undo the load-time number promotion, putting the original strings back
    /// and returning the columns to text. Called from the "Dismiss" button on
    /// the number banner; the sibling of `revert_promoted_date_columns`.
    fn revert_promoted_number_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_number_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for entry in &warning.entries {
            if entry.col_idx >= tab.table.col_count() {
                continue;
            }
            for (row, original) in entry.original_values.iter().enumerate() {
                if row >= tab.table.row_count() {
                    break;
                }
                tab.table.rows[row][entry.col_idx] = match original {
                    Some(s) => CellValue::String(s.clone()),
                    None => CellValue::Null,
                };
            }
            if let Some(col) = tab.table.columns.get_mut(entry.col_idx) {
                col.data_type = "Utf8".to_string();
            }
        }
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }

    /// Undo the load-time whitespace trim, restoring the original column titles
    /// and cell whitespace recorded in the trim banner's undo log. Called from
    /// the "Dismiss" button on the whitespace-trim banner.
    fn revert_trimmed_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_trim_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for (col_idx, title) in &warning.undo.titles {
            if let Some(col) = tab.table.columns.get_mut(*col_idx) {
                col.name = title.clone();
            }
        }
        for (col_idx, cells) in &warning.undo.cells {
            for (row_idx, value) in cells {
                if *row_idx < tab.table.row_count() && *col_idx < tab.table.col_count() {
                    tab.table.rows[*row_idx][*col_idx] = CellValue::String(value.clone());
                }
            }
        }
        // Restoring whitespace is an in-place change; re-sync the DB diff-save
        // baseline so it isn't later seen as an edit / schema change.
        crate::app::file_io::resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }
}
