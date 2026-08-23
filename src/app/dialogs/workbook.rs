//! "Export workbook": write several open tabs into one `.xlsx`, one sheet per
//! tab.
//!
//! Excel write has always been single-sheet, so exporting five related tables
//! meant five files. The writing itself is
//! `octa::formats::excel_reader::write_workbook`; this dialog only decides
//! which tabs go in and under what sheet names.
//!
//! Sheet names are seeded from the tab labels and stay editable: a tab label
//! can be long, duplicated, or carry characters Excel refuses. Whatever the
//! user leaves is still corrected by `sanitize_sheet_name` before writing, so
//! the dialog cannot produce an invalid workbook.

use eframe::egui;
use egui::RichText;

use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::{OctaApp, WorkbookState};

impl OctaApp {
    /// Open the dialog, seeded with every tab that has columns. Chart tabs and
    /// blank tabs have nothing to write, so they are excluded rather than
    /// offered and then rejected.
    pub(crate) fn open_workbook_dialog(&mut self) {
        let mut selected = Vec::with_capacity(self.tabs.len());
        let mut names = Vec::with_capacity(self.tabs.len());
        for (idx, tab) in self.tabs.iter().enumerate() {
            let exportable = tab.table.col_count() > 0 && !tab.is_chart_tab;
            selected.push(exportable && idx == self.active_tab);
            // The label, not the file name: a renamed tab should keep its name.
            names.push(tab.title_display().trim_end_matches(" *").to_string());
        }
        self.workbook_dialog = Some(WorkbookState {
            selected,
            names,
            error: None,
        });
    }
}

pub(crate) fn render_workbook_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.workbook_dialog.is_none() {
        return;
    }

    let mut close = false;
    let mut export = false;

    let dialog_id = egui::Id::new("octa_workbook_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_workbook")
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(460.0).min_width(380.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("workbook_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("workbook.title")).strong().size(16.0));
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

        let count = app
            .workbook_dialog
            .as_ref()
            .map(|d| d.selected.iter().filter(|&&v| v).count())
            .unwrap_or(0);

        egui::Panel::bottom("workbook_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let btn = ui.add_enabled(
                        count > 0,
                        egui::Button::new(format!("{} ({count})", t("workbook.export"))),
                    );
                    let btn = if count > 0 {
                        btn.on_hover_text(t("workbook.export_hint"))
                    } else {
                        btn.on_disabled_hover_text(t("workbook.need_one"))
                    };
                    if btn.clicked() {
                        export = true;
                    }
                    if ui.button(t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("workbook.hint"));
            ui.add_space(6.0);

            let Some(state) = app.workbook_dialog.as_mut() else {
                return;
            };
            if ui
                .small_button(t("workbook.select_all"))
                .on_hover_text(t("workbook.select_all_hint"))
                .clicked()
            {
                state.selected.fill(true);
            }
            ui.add_space(4.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("workbook_rows")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(t("workbook.tab_column")).strong());
                        ui.label(RichText::new(t("workbook.sheet_name")).strong())
                            .on_hover_text(t("workbook.sheet_name_hint"));
                        ui.end_row();

                        for idx in 0..state.selected.len() {
                            let mut checked = state.selected[idx];
                            let label = state.names[idx].clone();
                            if ui.checkbox(&mut checked, label).changed() {
                                state.selected[idx] = checked;
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut state.names[idx])
                                    .desired_width(180.0),
                            )
                            .on_hover_text(t("workbook.sheet_name_hint"));
                            ui.end_row();
                        }
                    });
            });

            if let Some(err) = state.error.clone() {
                ui.add_space(6.0);
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, &err);
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || export {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if export {
        app.export_workbook();
    }
    if close {
        app.workbook_dialog = None;
    }
}

impl OctaApp {
    /// Snapshot the ticked tabs and write them as one workbook.
    fn export_workbook(&mut self) {
        let Some(state) = self.workbook_dialog.as_ref() else {
            return;
        };
        let picks: Vec<(usize, String)> = state
            .selected
            .iter()
            .enumerate()
            .filter(|(_, v)| **v)
            .map(|(idx, _)| (idx, state.names[idx].clone()))
            .collect();
        if picks.is_empty() {
            return;
        }

        let Some(path) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx"])
            .set_file_name("workbook.xlsx")
            .save_file()
        else {
            return; // cancelled: leave the dialog standing
        };

        // Snapshot first: `apply_edits` must not run on the live tables, and
        // the writer needs owned tables to borrow from.
        let tables: Vec<octa::data::DataTable> = picks
            .iter()
            .map(|(idx, _)| {
                let mut snap = self.tabs[*idx].table.clone();
                snap.apply_edits();
                snap
            })
            .collect();
        let sheets: Vec<(
            String,
            &octa::data::DataTable,
            Option<&octa::formats::write_options::TableStyle>,
        )> = picks
            .iter()
            .zip(tables.iter())
            .map(|((_, name), table)| (name.clone(), table, None))
            .collect();

        match octa::formats::excel_reader::write_workbook(&path, &sheets) {
            Ok(()) => {
                self.status_message = Some((
                    t("workbook.done")
                        .replace("{n}", &sheets.len().to_string())
                        .replace("{path}", &path.display().to_string()),
                    std::time::Instant::now(),
                ));
                self.workbook_dialog = None;
            }
            Err(e) => {
                if let Some(state) = self.workbook_dialog.as_mut() {
                    state.error = Some(format!("{e:#}"));
                }
            }
        }
    }
}
