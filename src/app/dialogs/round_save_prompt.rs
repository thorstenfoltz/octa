//! "Round on save?" prompt. Shown when the user saves a tab that has
//! per-column rounding formats (which are otherwise display-only). Lets them
//! choose whether the written file carries the rounded values or full
//! precision. Set up by `do_save_tab`; resolved here.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_round_save_prompt_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_round_save.clone() else {
        return;
    };

    let mut decision: Option<bool> = None;
    let mut cancel = false;
    let mut open = true;

    // Explicit, stable window id (not the title-derived default). The dialog
    // shipped for a while as a fixed-size, non-resizable window, and egui
    // persisted that locked size under the old key; bumping the suffix discards
    // it so the new resizable defaults take effect.
    egui::Window::new(octa::i18n::t("dialog.round_title"))
        .id(egui::Id::new("octa_round_save_dialog_v2"))
        .open(&mut open)
        .resizable([true, true])
        .collapsible(false)
        .default_width(380.0)
        .default_height(160.0)
        .min_width(300.0)
        .min_height(120.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            // Fill the window in both axes so the resize handles drag freely
            // instead of the window snapping back to content size.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("dialog.round_body"));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(octa::i18n::t("dialog.round_save_rounded"))
                            .on_hover_text(octa::i18n::t("dialog.round_save_rounded_hint"))
                            .clicked()
                        {
                            decision = Some(true);
                        }
                        if ui
                            .button(octa::i18n::t("dialog.round_save_full"))
                            .on_hover_text(octa::i18n::t("dialog.round_save_full_hint"))
                            .clicked()
                        {
                            decision = Some(false);
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
        });

    if let Some(round) = decision {
        app.pending_round_save = None;
        app.do_save_tab_inner(
            prompt.tab_idx,
            prompt.path,
            prompt.save_filtered_view,
            Some(round),
        );
    } else if cancel || !open {
        app.pending_round_save = None;
    }
}
