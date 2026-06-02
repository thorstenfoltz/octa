//! Multi-select Excel sheet picker. Shown when a workbook has more sheets
//! than `excel_max_auto_sheets`. The user ticks which sheets to open; each
//! checked sheet loads into its own tab. The first N are pre-checked but the
//! user may pick any number (including all).

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_sheet_picker_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pending_sheet_picker.is_none() {
        return;
    }

    let mut open = true;
    let mut confirm = false;
    let mut cancel = false;

    egui::Window::new(octa::i18n::t("dialog.sheets_title"))
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .min_width(320.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let picker = app.pending_sheet_picker.as_mut().unwrap();
            let file_label = picker
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            ui.label(format!(
                "{} - {}",
                file_label,
                octa::i18n::t("dialog.sheets_prompt")
            ));
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui
                    .small_button(octa::i18n::t("dialog.select_all"))
                    .clicked()
                {
                    for v in &mut picker.selected {
                        *v = true;
                    }
                }
                if ui
                    .small_button(octa::i18n::t("dialog.select_none"))
                    .clicked()
                {
                    for v in &mut picker.selected {
                        *v = false;
                    }
                }
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (idx, name) in picker.sheet_names.iter().enumerate() {
                        let mut checked = picker.selected[idx];
                        if ui.checkbox(&mut checked, name).changed() {
                            picker.selected[idx] = checked;
                        }
                    }
                });

            let count = picker.selected.iter().filter(|&&v| v).count();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let open_btn = ui.add_enabled(
                    count > 0,
                    egui::Button::new(format!(
                        "{} ({count})",
                        octa::i18n::t("dialog.open_selected_sheets")
                    )),
                );
                if open_btn.clicked() {
                    confirm = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        if let Some(picker) = app.pending_sheet_picker.take() {
            let path = picker.path.clone();
            let chosen: Vec<String> = picker
                .sheet_names
                .iter()
                .zip(picker.selected.iter())
                .filter(|&(_, &sel)| sel)
                .map(|(name, _)| name.clone())
                .collect();
            for name in chosen {
                app.load_table(path.clone(), name);
            }
        }
    } else if cancel || !open {
        app.pending_sheet_picker = None;
    }
}
