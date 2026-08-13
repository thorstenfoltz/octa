//! "Include formatting?" prompt. Shown when saving a tab that carries marks,
//! conditional colours, frozen columns or number formats to `.xlsx`, which can
//! hold all four. Set up by `do_save_tab_inner`; resolved here.
//!
//! Modelled on `round_save_prompt.rs`: a forced-choice modal with no close
//! `x`, dismissed by its own buttons, which re-enter the save with the answer.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_xlsx_style_save_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_xlsx_style_save.clone() else {
        return;
    };

    let mut decision: Option<bool> = None;
    let mut remember = app.xlsx_style_remember;

    egui::Window::new(octa::i18n::t("dialog.xstyle_title"))
        .id(egui::Id::new("octa_xlsx_style_save_dialog_v1"))
        .resizable([true, true])
        .collapsible(false)
        .default_width(400.0)
        .default_height(170.0)
        .min_width(320.0)
        .min_height(130.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("dialog.xstyle_body"));
                    ui.add_space(8.0);
                    ui.checkbox(&mut remember, octa::i18n::t("dialog.xstyle_remember"))
                        .on_hover_text(octa::i18n::t("dialog.xstyle_remember_hint"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(octa::i18n::t("dialog.xstyle_include")).clicked() {
                            decision = Some(true);
                        }
                        if ui.button(octa::i18n::t("dialog.xstyle_plain")).clicked() {
                            decision = Some(false);
                        }
                    });
                });
        });

    app.xlsx_style_remember = remember;

    if let Some(include) = decision {
        app.pending_xlsx_style_save = None;
        if remember {
            app.settings.write_options.xlsx.include_formatting = include;
            app.settings.save();
        }
        app.do_save_tab_with_style(
            prompt.tab_idx,
            prompt.path,
            prompt.save_filtered_view,
            prompt.round_decision,
            Some(include),
        );
    }
}
