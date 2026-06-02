//! Column picker for Value Frequency when launched without a column context
//! (the **Analyse -> Value frequency...** menu entry, or the shortcut with no
//! cell selected). On confirm it sets `value_frequency_col`, which opens the
//! main value-frequency dialog.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_value_frequency_picker_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].value_frequency_pick {
        return;
    }
    // Nothing to pick from - close silently.
    if app.tabs[app.active_tab].table.col_count() == 0 {
        app.tabs[app.active_tab].value_frequency_pick = false;
        return;
    }

    let mut open = true;
    let mut chosen: Option<usize> = None;
    let mut cancel = false;

    egui::Window::new(octa::i18n::t("dialog.vfpick_title"))
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .min_width(280.0)
        .default_width(320.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            ui.label(octa::i18n::t("dialog.vfpick_prompt"));
            ui.add_space(6.0);

            let tab = &app.tabs[app.active_tab];
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (idx, col) in tab.table.columns.iter().enumerate() {
                        let label = format!("{} [{}]", col.name, col.data_type);
                        if ui.selectable_label(false, label).clicked() {
                            chosen = Some(idx);
                        }
                    }
                });

            ui.add_space(8.0);
            if ui.button(octa::i18n::t("common.cancel")).clicked() {
                cancel = true;
            }
        });

    if let Some(col_idx) = chosen {
        let tab = &mut app.tabs[app.active_tab];
        tab.value_frequency_pick = false;
        tab.value_frequency_col = Some(col_idx);
        tab.value_frequency_size = octa::ui::settings::DialogSize::default();
    } else if cancel || !open {
        app.tabs[app.active_tab].value_frequency_pick = false;
    }
}
