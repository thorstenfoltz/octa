//! Update-check and self-install orchestration. This module drives the
//! background threads that hit GitHub, download a new release archive, and
//! (on Linux) escalate via pkexec when the install directory is not
//! user-writable.

pub(crate) mod install_unix;

use std::sync::Arc;

use eframe::egui;

use super::state::{OctaApp, UpdateState};

// ureq 3.x caps response bodies at 10 MB by default. Release archives bundle
// DuckDB + MuPDF + others and exceed that easily on Windows.
const UPDATE_BODY_LIMIT: u64 = 512 * 1024 * 1024;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) enum UpdateOutcome {
    Installed,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    NeedsElevation {
        install_path: std::path::PathBuf,
        tmp_path: std::path::PathBuf,
    },
}

#[cfg(target_os = "linux")]
pub(crate) enum InstallError {
    PermissionDenied,
    Other(String),
}

impl OctaApp {
    pub(crate) fn check_for_updates(&self, ctx: &egui::Context) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        *state.lock().unwrap() = UpdateState::Checking;
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                let body =
                    ureq::get("https://api.github.com/repos/thorstenfoltz/octa/releases/latest")
                        .header("User-Agent", &format!("octa/{}", VERSION))
                        .header("Accept", "application/vnd.github.v3+json")
                        .call()
                        .map_err(|e| format!("Request failed: {}", e))?
                        .body_mut()
                        .with_config()
                        .limit(UPDATE_BODY_LIMIT)
                        .read_to_string()
                        .map_err(|e| format!("Read failed: {}", e))?;

                let resp: serde_json::Value =
                    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;

                resp["tag_name"]
                    .as_str()
                    .map(|s: &str| s.trim_start_matches('v').to_string())
                    .ok_or_else(|| "No tag_name in response".to_string())
            })();

            let mut s = state.lock().unwrap();
            match result {
                Ok(latest) if latest != VERSION => *s = UpdateState::Available(latest),
                Ok(_) => *s = UpdateState::UpToDate,
                Err(e) => *s = UpdateState::Error(e),
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn perform_update(&self, new_version: &str, ctx: &egui::Context) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        let version = new_version.to_string();
        *state.lock().unwrap() = UpdateState::Updating;

        std::thread::spawn(move || {
            let result = Self::download_and_replace(&version);
            let mut s = state.lock().unwrap();
            match result {
                Ok(UpdateOutcome::Installed) => *s = UpdateState::Updated(version),
                Ok(UpdateOutcome::NeedsElevation {
                    install_path,
                    tmp_path,
                }) => {
                    *s = UpdateState::NeedsElevation {
                        version,
                        install_path,
                        tmp_path,
                    }
                }
                Err(e) => *s = UpdateState::Error(e),
            }
            ctx.request_repaint();
        });
    }

    /// Linux only: invoke pkexec to copy the staged binary into a
    /// non-user-writable install path (e.g. /usr/local/bin). pkexec shows a
    /// native graphical password prompt and runs the install as root.
    #[cfg(target_os = "linux")]
    pub(crate) fn install_with_sudo(
        &self,
        tmp_path: std::path::PathBuf,
        install_path: std::path::PathBuf,
        version: String,
        ctx: &egui::Context,
    ) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        *state.lock().unwrap() = UpdateState::Updating;

        std::thread::spawn(move || {
            let result = install_unix::run_pkexec_install(&tmp_path, &install_path);
            let _ = std::fs::remove_file(&tmp_path);
            let mut s = state.lock().unwrap();
            match result {
                Ok(()) => *s = UpdateState::Updated(version),
                Err(e) => *s = UpdateState::Error(e),
            }
            ctx.request_repaint();
        });
    }

    fn download_and_replace(new_version: &str) -> Result<UpdateOutcome, String> {
        let current_exe =
            std::env::current_exe().map_err(|e| format!("Cannot find current exe: {}", e))?;

        #[cfg(target_os = "linux")]
        {
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{0}/octa-{0}-linux-x86_64.tar.gz",
                new_version
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| format!("Download failed: {}", e))?
                .body_mut()
                .with_config()
                .limit(UPDATE_BODY_LIMIT)
                .read_to_vec()
                .map_err(|e| format!("Read failed: {}", e))?;

            let staging_path = std::env::temp_dir().join(format!(
                "octa-update-{}-{}",
                new_version,
                std::process::id()
            ));
            let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
            let mut archive = tar::Archive::new(decoder);
            let binary_name = format!("octa-{}-linux-x86_64/octa", new_version);

            let mut found = false;
            for entry in archive.entries().map_err(|e| format!("Tar error: {}", e))? {
                let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
                let path = entry
                    .path()
                    .map_err(|e| format!("Path error: {}", e))?
                    .to_path_buf();
                if path.to_string_lossy() == binary_name {
                    let mut tmp_file = std::fs::File::create(&staging_path)
                        .map_err(|e| format!("Cannot create staging file: {}", e))?;
                    std::io::copy(&mut entry, &mut tmp_file)
                        .map_err(|e| format!("Extract failed: {}", e))?;

                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))
                        .map_err(|e| format!("chmod failed: {}", e))?;

                    found = true;
                    break;
                }
            }

            if !found {
                let _ = std::fs::remove_file(&staging_path);
                return Err(format!("Binary '{}' not found in archive", binary_name));
            }

            if !install_unix::install_dir_writable(&current_exe) {
                return Ok(UpdateOutcome::NeedsElevation {
                    install_path: current_exe,
                    tmp_path: staging_path,
                });
            }

            match install_unix::install_replace_unix(&staging_path, &current_exe) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&staging_path);
                }
                Err(InstallError::PermissionDenied) => {
                    return Ok(UpdateOutcome::NeedsElevation {
                        install_path: current_exe,
                        tmp_path: staging_path,
                    });
                }
                Err(InstallError::Other(e)) => {
                    let _ = std::fs::remove_file(&staging_path);
                    return Err(e);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{0}/octa-{0}-windows-x86_64.zip",
                new_version
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| format!("Download failed: {}", e))?
                .body_mut()
                .with_config()
                .limit(UPDATE_BODY_LIMIT)
                .read_to_vec()
                .map_err(|e| format!("Read failed: {}", e))?;

            let cursor = std::io::Cursor::new(bytes);
            let mut archive =
                zip::ZipArchive::new(cursor).map_err(|e| format!("Zip error: {}", e))?;

            let binary_name = "octa.exe";
            let mut found = false;
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| format!("Zip entry error: {}", e))?;
                if file.name().ends_with(binary_name) && !file.name().ends_with('/') {
                    let tmp_path = current_exe.with_extension("update.exe");
                    let mut tmp_file = std::fs::File::create(&tmp_path)
                        .map_err(|e| format!("Cannot create temp file: {}", e))?;
                    std::io::copy(&mut file, &mut tmp_file)
                        .map_err(|e| format!("Extract failed: {}", e))?;

                    let old_path = current_exe.with_extension("old.exe");
                    let _ = std::fs::remove_file(&old_path);
                    std::fs::rename(&current_exe, &old_path)
                        .map_err(|e| format!("Backup rename failed: {}", e))?;
                    std::fs::rename(&tmp_path, &current_exe)
                        .map_err(|e| format!("Install rename failed: {}", e))?;

                    found = true;
                    break;
                }
            }

            if !found {
                return Err(format!("'{}' not found in archive", binary_name));
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = current_exe;
            let _ = new_version;
            return Err(
                "Auto-update is not supported on this platform. Please download the latest release from the repository.".to_string(),
            );
        }

        Ok(UpdateOutcome::Installed)
    }
}
