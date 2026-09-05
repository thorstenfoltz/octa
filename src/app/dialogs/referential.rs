//! Referential-integrity dialog: pick a parent key and the child column that
//! points at it, and open the orphans as a detached tab.
//!
//! Same two-row shape as the distribution comparison, because the question has
//! the same shape: two columns, each from any open tab.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::widgets::col_combo;
use crate::app::state::OctaApp;

pub(crate) fn render_referential_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.referential_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut st = app.referential_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let tabs: Vec<String> = app.tabs.iter().map(|t| t.title_display()).collect();
    st.parent_tab = st.parent_tab.min(tabs.len().saturating_sub(1));
    st.child_tab = st.child_tab.min(tabs.len().saturating_sub(1));
    let parent_cols = column_names(app, st.parent_tab);
    let child_cols = column_names(app, st.child_tab);

    let dialog_id = egui::Id::new("octa_referential_dialog");
    let window = egui::Window::new("octa_referential")
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
        egui::Panel::top("referential_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("refint.title"))
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
        let ready = st.parent_col.is_some() && st.child_col.is_some();
        egui::Panel::bottom("referential_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ready, egui::Button::new(octa::i18n::t("refint.check")))
                        .on_hover_text(octa::i18n::t("refint.check_hint"))
                        .on_disabled_hover_text(octa::i18n::t("refint.need_columns"))
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
                RichText::new(octa::i18n::t("refint.intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::Grid::new("referential_grid")
                .num_columns(3)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    // The two words this dialog turns on. Hovering either row
                    // says which column goes where, because "parent" and
                    // "child" only mean something once you know which of your
                    // two tables is which.
                    ui.label(octa::i18n::t("refint.parent"))
                        .on_hover_text(octa::i18n::t("refint.parent_hint"));
                    ui.scope(|ui| {
                        tab_combo(
                            ui,
                            "ri_tab_p",
                            &mut st.parent_tab,
                            &tabs,
                            &mut st.parent_col,
                        );
                    })
                    .response
                    .on_hover_text(octa::i18n::t("refint.parent_hint"));
                    ui.scope(|ui| {
                        col_combo(ui, "ri_col_p", &mut st.parent_col, &parent_cols);
                    })
                    .response
                    .on_hover_text(octa::i18n::t("refint.parent_hint"));
                    ui.end_row();

                    ui.label(octa::i18n::t("refint.child"))
                        .on_hover_text(octa::i18n::t("refint.child_hint"));
                    ui.scope(|ui| {
                        tab_combo(ui, "ri_tab_c", &mut st.child_tab, &tabs, &mut st.child_col);
                    })
                    .response
                    .on_hover_text(octa::i18n::t("refint.child_hint"));
                    ui.scope(|ui| {
                        col_combo(ui, "ri_col_c", &mut st.child_col, &child_cols);
                    })
                    .response
                    .on_hover_text(octa::i18n::t("refint.child_hint"));
                    ui.end_row();
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run && let (Some(p), Some(c)) = (st.parent_col, st.child_col) {
        app.open_referential_tab(st.parent_tab, p, st.child_tab, c);
        return;
    }
    if !close {
        app.referential_dialog = Some(st);
    }
}

/// Tab picker. Changing the tab clears the column beside it: index 3 of one
/// table is not index 3 of another.
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
