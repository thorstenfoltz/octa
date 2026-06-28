//! Column picker for Value Frequency when launched without a column context
//! (the **Analyse -> Value frequency...** menu entry, or the shortcut with no
//! cell selected). On confirm it sets `value_frequency_col`, which opens the
//! main value-frequency dialog.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

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

    let mut chosen: Option<usize> = None;
    let mut close = false;

    let dialog_id = egui::Id::new("octa_value_frequency_picker_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_value_frequency_picker")
        .title_bar(false)
        .collapsible(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(320.0).min_width(280.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("vfpick_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.vfpick_title"))
                            .strong()
                            .size(16.0),
                    );
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

        egui::Panel::bottom("vfpick_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    close = true;
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(octa::i18n::t("dialog.vfpick_prompt"));
            ui.add_space(6.0);

            let tab = &app.tabs[app.active_tab];
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, col) in tab.table.columns.iter().enumerate() {
                    let label = format!("{} [{}]", col.name, col.data_type);
                    if ui.selectable_label(false, label).clicked() {
                        chosen = Some(idx);
                    }
                }
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || chosen.is_some() {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if let Some(col_idx) = chosen {
        let tab = &mut app.tabs[app.active_tab];
        tab.value_frequency_pick = false;
        tab.value_frequency_col = Some(col_idx);
        tab.value_frequency_size = octa::ui::settings::DialogSize::default();
    } else if close {
        app.tabs[app.active_tab].value_frequency_pick = false;
    }
}
