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
    egui::Window::new("Check for Updates")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let state = app.update_state.lock().unwrap().clone();
            match state {
                UpdateState::Idle | UpdateState::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Checking for updates...");
                    });
                }
                UpdateState::UpToDate => {
                    ui.label(format!("You are running the latest version ({}).", VERSION));
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
                UpdateState::Available(ref new_version) => {
                    ui.label(format!(
                        "A new version is available: {} (current: {})",
                        new_version, VERSION
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let version = new_version.clone();
                        if ui.button("Update Now").clicked() {
                            app.perform_update(&version, ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            app.show_update_dialog = false;
                            *app.update_state.lock().unwrap() = UpdateState::Idle;
                        }
                    });
                }
                UpdateState::Updating => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Downloading and installing update...");
                    });
                }
                UpdateState::NeedsElevation {
                    ref version,
                    ref install_path,
                    ref tmp_path,
                } => {
                    ui.label(RichText::new("Administrator password required").strong());
                    ui.add_space(4.0);
                    ui.label(format!(
                        "Octa is installed at:\n    {}",
                        install_path.display()
                    ));
                    ui.add_space(4.0);
                    ui.label(format!(
                        "This directory is not writable by your user, so \
                         installing version {} requires elevated \
                         permissions. Octa will run `pkexec` to ask for \
                         your password and copy the new binary into \
                         place. The downloaded file is already staged \
                         locally — no further download will happen.",
                        version
                    ));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let version_c = version.clone();
                        let tmp_c = tmp_path.clone();
                        let install_c = install_path.clone();
                        if ui.button("Update with administrator password").clicked() {
                            #[cfg(target_os = "linux")]
                            {
                                app.install_with_sudo(tmp_c, install_c, version_c, ctx);
                            }
                            #[cfg(not(target_os = "linux"))]
                            {
                                let _ = (tmp_c, install_c, version_c);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            let _ = std::fs::remove_file(tmp_path);
                            app.show_update_dialog = false;
                            *app.update_state.lock().unwrap() = UpdateState::Idle;
                        }
                    });
                }
                UpdateState::Updated(ref version) => {
                    ui.label(format!(
                        "Updated to version {}. Please restart Octa to use the new version.",
                        version
                    ));
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
                UpdateState::Error(ref msg) => {
                    ui.label(format!("Update check failed: {}", msg));
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        app.show_update_dialog = false;
                        *app.update_state.lock().unwrap() = UpdateState::Idle;
                    }
                }
            }
        });
}
