//! "Check for Updates" dialog. Matches on [`UpdateState`] and renders the
//! appropriate UI: spinner, "up to date", "new version available" with an
//! update button, pkexec elevation prompt (Linux), "updated, restart", or
//! "error".

use eframe::egui;
use egui::RichText;

use super::super::state::{OctaApp, UpdateState};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn render_update_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_update_dialog {
        return;
    }
    egui::Window::new(octa::i18n::t("dialog.ud_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let state = app.update_state.lock().unwrap().clone();
            match state {
                UpdateState::Idle | UpdateState::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(octa::i18n::t("dialog.ud_checking"));
                    });
                }
                UpdateState::UpToDate => {
                    ui.label(format!(
                        "{} ({}).",
                        octa::i18n::t("dialog.ud_latest"),
                        VERSION
                    ));
                    ui.add_space(8.0);
                    if ui.button(octa::i18n::t("common.close")).clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
                UpdateState::Available(ref new_version) => {
                    ui.label(format!(
                        "{}: {} ({}: {})",
                        octa::i18n::t("dialog.ud_new_avail"),
                        new_version,
                        octa::i18n::t("dialog.ud_current"),
                        VERSION
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let version = new_version.clone();
                        if ui.button(octa::i18n::t("dialog.ud_update_now")).clicked() {
                            app.perform_update(&version, ctx);
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            app.show_update_dialog = false;
                            *app.update_state.lock().unwrap() = UpdateState::Idle;
                        }
                    });
                }
                UpdateState::Updating => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(octa::i18n::t("dialog.ud_downloading"));
                    });
                }
                UpdateState::NeedsElevation {
                    ref version,
                    ref install_path,
                    ref tmp_path,
                } => {
                    ui.label(RichText::new(octa::i18n::t("dialog.ud_admin_required")).strong());
                    ui.add_space(4.0);
                    ui.label(format!(
                        "{}\n    {}",
                        octa::i18n::t("dialog.ud_installed_at"),
                        install_path.display()
                    ));
                    ui.add_space(4.0);
                    ui.label(format!(
                        "{} {} {}",
                        octa::i18n::t("dialog.ud_elev_pre"),
                        version,
                        octa::i18n::t("dialog.ud_elev_post")
                    ));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let version_c = version.clone();
                        let tmp_c = tmp_path.clone();
                        let install_c = install_path.clone();
                        if ui
                            .button(octa::i18n::t("dialog.ud_update_with_admin"))
                            .clicked()
                        {
                            #[cfg(target_os = "linux")]
                            {
                                app.install_with_sudo(tmp_c, install_c, version_c, ctx);
                            }
                            #[cfg(not(target_os = "linux"))]
                            {
                                let _ = (tmp_c, install_c, version_c);
                            }
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            let _ = std::fs::remove_file(tmp_path);
                            app.show_update_dialog = false;
                            *app.update_state.lock().unwrap() = UpdateState::Idle;
                        }
                    });
                }
                UpdateState::Updated(ref version) => {
                    ui.label(format!(
                        "{} {}. {}",
                        octa::i18n::t("dialog.ud_updated_to"),
                        version,
                        octa::i18n::t("dialog.ud_restart")
                    ));
                    ui.add_space(8.0);
                    if ui.button(octa::i18n::t("common.close")).clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
                UpdateState::Error(ref msg) => {
                    ui.label(format!("{}: {}", octa::i18n::t("dialog.ud_failed"), msg));
                    ui.add_space(8.0);
                    if ui.button(octa::i18n::t("common.close")).clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
            }
        });
}
