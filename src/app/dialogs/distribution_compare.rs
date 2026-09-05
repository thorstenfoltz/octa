//! Compare-distributions dialog: pick two columns, from one open tab or two,
//! and open the answer as a detached tab.
//!
//! No test picker: the engine chooses Kolmogorov-Smirnov or chi-square from
//! what the columns hold, and asking the user to pick would only let them pick
//! wrong.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::widgets::col_combo;
use crate::app::state::OctaApp;

pub(crate) fn render_dist_compare_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.dist_compare_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut st = app.dist_compare_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let tabs: Vec<String> = app.tabs.iter().map(|t| t.title_display()).collect();
    st.tab_a = st.tab_a.min(tabs.len().saturating_sub(1));
    st.tab_b = st.tab_b.min(tabs.len().saturating_sub(1));
    let cols_a = column_names(app, st.tab_a);
    let cols_b = column_names(app, st.tab_b);

    let dialog_id = egui::Id::new("octa_dist_compare_dialog");
    let window = egui::Window::new("octa_dist_compare")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(240.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("dist_compare_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("distcmp.title"))
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
        let ready = st.col_a.is_some() && st.col_b.is_some();
        egui::Panel::bottom("dist_compare_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ready, egui::Button::new(octa::i18n::t("distcmp.compare")))
                        .on_hover_text(octa::i18n::t("distcmp.compare_hint"))
                        .on_disabled_hover_text(octa::i18n::t("distcmp.need_columns"))
                        .clicked()
                    {
                        run = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("distcmp.intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::Grid::new("dist_compare_grid")
                .num_columns(3)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    // Both rows say the same thing, because the two columns
                    // are interchangeable: the test does not care which is
                    // which.
                    let hint = octa::i18n::t("distcmp.column_hint");
                    ui.label(octa::i18n::t("distcmp.first"))
                        .on_hover_text(&hint);
                    ui.scope(|ui| {
                        tab_combo(ui, "dc_tab_a", &mut st.tab_a, &tabs, &mut st.col_a);
                    })
                    .response
                    .on_hover_text(&hint);
                    ui.scope(|ui| {
                        col_combo(ui, "dc_col_a", &mut st.col_a, &cols_a);
                    })
                    .response
                    .on_hover_text(&hint);
                    ui.end_row();

                    ui.label(octa::i18n::t("distcmp.second"))
                        .on_hover_text(&hint);
                    ui.scope(|ui| {
                        tab_combo(ui, "dc_tab_b", &mut st.tab_b, &tabs, &mut st.col_b);
                    })
                    .response
                    .on_hover_text(&hint);
                    ui.scope(|ui| {
                        col_combo(ui, "dc_col_b", &mut st.col_b, &cols_b);
                    })
                    .response
                    .on_hover_text(&hint);
                    ui.end_row();
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run && let (Some(a), Some(b)) = (st.col_a, st.col_b) {
        app.open_dist_compare_tab(st.tab_a, a, st.tab_b, b);
        return;
    }
    if !close {
        app.dist_compare_dialog = Some(st);
    }
}

/// Tab picker. Changing the tab clears the column beside it: index 3 of one
/// table is not index 3 of another, and silently comparing the wrong column is
/// worse than making the user pick again.
fn tab_combo(
    ui: &mut egui::Ui,
    id: &str,
    sel: &mut usize,
    tabs: &[String],
    col: &mut Option<usize>,
) {
    let text = tabs.get(*sel).cloned().unwrap_or_default();
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, name) in tabs.iter().enumerate() {
                if ui.selectable_label(*sel == i, name).clicked() && *sel != i {
                    *sel = i;
                    *col = None;
                }
            }
        });
}

fn column_names(app: &OctaApp, tab: usize) -> Vec<String> {
    app.tabs
        .get(tab)
        .map(|t| t.table.columns.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default()
}
