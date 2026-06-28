//! Confirmation modal for a DB save that changes the table schema. Mirrors the
//! deferred `round_save` flow: re-enters `do_save_tab_inner` with the decision.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_schema_change_save_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_schema_change_save.clone() else {
        return;
    };
    let mut proceed = false;
    let mut cancel = false;
    // No close 'x' (no `.open`): forced-choice prompt dismissed by its own
    // Proceed / Cancel buttons, matching the other confirmation dialogs.
    egui::Window::new(octa::i18n::t("dialog.scs_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(octa::i18n::t("dialog.scs_intro"));
            ui.add_space(4.0);
            for line in &prompt.changes {
                ui.monospace(line);
            }
            ui.add_space(6.0);
            match &prompt.backup_note {
                Some(p) => {
                    ui.label(octa::i18n::t("dialog.scs_backup"));
                    ui.monospace(p);
                }
                None => {
                    ui.label(octa::i18n::t("dialog.scs_no_backup"));
                }
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(octa::i18n::t("dialog.scs_warn"))
                    .color(ui.visuals().warn_fg_color),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("dialog.scs_proceed")).clicked() {
                    proceed = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
        });

    if proceed {
        app.pending_schema_change_save = None;
        app.do_save_tab_inner(
            prompt.tab_idx,
            prompt.path,
            prompt.save_filtered_view,
            None,
            Some(true),
        );
    } else if cancel {
        app.pending_schema_change_save = None;
        app.status_message = Some((
            octa::i18n::t("dialog.scs_cancelled"),
            std::time::Instant::now(),
        ));
    }
}
