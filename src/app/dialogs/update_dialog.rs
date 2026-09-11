//! "Check for Updates" dialog. Matches on [`UpdateState`] and renders the
//! appropriate UI: spinner, "up to date", "new version available" with an
//! update button, pkexec elevation prompt (Linux), "updated, restart", or
//! "error".

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::{OctaApp, UpdateState};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn render_update_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_update_dialog {
        return;
    }
    let dialog_id = egui::Id::new("octa_update_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 260.0));
    let window = egui::Window::new("octa_update")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(260.0)
            .min_width(340.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("update_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.ud_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
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
                UpdateState::Available {
                    version: ref new_version,
                    ..
                } => {
                    ui.label(format!(
                        "{}: {} ({}: {})",
                        octa::i18n::t("dialog.ud_new_avail"),
                        new_version,
                        octa::i18n::t("dialog.ud_current"),
                        VERSION
                    ));
                    ui.add_space(8.0);
                    // Unreachable on a Store (MSIX) build - neither the Help
                    // menu nor the startup check offers to look there any
                    // more. Kept as the last gate all the same: an install
                    // under WindowsApps cannot be replaced from inside the
                    // app, so no future caller should ever get the button.
                    let store = octa::platform::is_store_packaged();
                    if store {
                        ui.label(octa::i18n::t("release.store"));
                        ui.add_space(8.0);
                    }
                    ui.horizontal(|ui| {
                        let version = new_version.clone();
                        if !store && ui.button(octa::i18n::t("dialog.ud_update_now")).clicked() {
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
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        app.show_update_dialog = false;
        *app.update_state.lock().unwrap() = UpdateState::Idle;
    }
}
