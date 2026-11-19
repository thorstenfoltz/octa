//! Update-check and self-install orchestration. This module drives the
//! background threads that hit GitHub, download a new release archive, and
//! (on Linux) escalate via pkexec when the install directory is not
//! user-writable.

pub(crate) mod install_unix;

use std::sync::Arc;

use eframe::egui;

use super::state::{OctaApp, UpdateState};

// ureq 3.x caps response bodies at 10 MB by default. Release archives bundle
// DuckDB + others and exceed that easily on Windows.
#[cfg(any(target_os = "linux", target_os = "windows"))]
const UPDATE_BODY_LIMIT: u64 = 512 * 1024 * 1024;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const GET_OCTA: &str =
    "curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash";

/// The command a Mac user runs to update. Octa cannot update itself there:
/// the release is a signed `Octa.app`, and swapping the binary inside it the
/// way Linux and Windows do breaks the seal, so the update dialog shows the
/// one-line installer instead. It names the folder the running bundle sits in,
/// unless that is get-octa.sh's default `~/Applications`, so the update
/// replaces this copy rather than installing a second one beside it.
pub(crate) fn mac_update_command(exe: &std::path::Path, home: &std::path::Path) -> String {
    // .../Octa.app/Contents/MacOS/octa
    let dir = exe
        .ancestors()
        .nth(3)
        .filter(|bundle| bundle.extension().is_some_and(|e| e == "app"))
        .and_then(std::path::Path::parent);
    match dir {
        Some(dir) if dir != home.join("Applications") => {
            let quoted = dir.to_string_lossy().replace('\'', r"'\''");
            format!("{GET_OCTA} -s -- '{quoted}'")
        }
        _ => GET_OCTA.to_string(),
    }
}

/// Hex SHA-256 of a byte slice.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Parse a `sha256sum`-style SHA256SUMS file: one `<hex>  <name>` line per
/// artifact. Tolerates the `*name` binary-mode marker and skips junk lines.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn parse_sha256sums(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hex), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let name = name.trim_start_matches('*');
        out.insert(name.to_string(), hex.to_ascii_lowercase());
    }
    out
}

/// Fetch the SHA256SUMS file attached to a release. `None` on any failure:
/// releases published before checksums shipped do not have one.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn fetch_sha256sums(new_version: &str) -> Option<String> {
    let url =
        format!("https://github.com/thorstenfoltz/octa/releases/download/{new_version}/SHA256SUMS");
    ureq::get(&url)
        .header("User-Agent", &format!("octa/{}", VERSION))
        .call()
        .ok()?
        .body_mut()
        .with_config()
        // Checksum files are tiny; anything bigger is wrong.
        .limit(1024 * 1024)
        .read_to_string()
        .ok()
}

/// Verify a downloaded release archive against the release's SHA256SUMS.
/// An unreachable checksum file, a missing entry or a hash mismatch all abort
/// the update.
///
/// Absence used to be tolerated with a logged warning, for releases published
/// before checksums shipped. That made the check opt-out by accident: the one
/// state an attacker can force is "no checksum file", so the weakest release on
/// the server decided whether verification happened at all. Every supported
/// release carries SHA256SUMS, so failing here costs nothing and closes that.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn verify_archive_checksum(
    new_version: &str,
    archive_name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let Some(text) = fetch_sha256sums(new_version) else {
        return Err(octa::i18n::t("errs.upd_sums_missing").replace("{version}", new_version));
    };
    let sums = parse_sha256sums(&text);
    let Some(expected) = sums.get(archive_name) else {
        return Err(octa::i18n::t("errs.upd_sums_unlisted").replace("{file}", archive_name));
    };
    let got = sha256_hex(bytes);
    if &got != expected {
        return Err(octa::i18n::t("errs.upd_sums_mismatch")
            .replace("{file}", archive_name)
            .replace("{expected}", expected)
            .replace("{got}", &got));
    }
    Ok(())
}

/// A failure message in the user's language: `key`'s text with the cause.
fn failed(key: &str, e: impl std::fmt::Display) -> String {
    octa::i18n::t(key).replace("{error}", &e.to_string())
}

/// Pull the version out of the address `releases/latest` redirects to
/// (`.../releases/tag/v0.17.0`). The tag is normalised (`v0.17.0` ->
/// `0.17.0`) so it can be compared with `CARGO_PKG_VERSION`. The notes window
/// reads the copy baked into the binary, so this request only answers "is
/// there a newer version".
fn parse_release(location: &str) -> Result<String, String> {
    location
        .rsplit_once("/releases/tag/")
        .map(|(_, tag)| tag.trim_start_matches('v').to_string())
        .filter(|version| !version.is_empty())
        .ok_or_else(|| octa::i18n::t("errs.upd_no_release").replace("{detail}", location))
}

/// Ask GitHub which release is the newest. The release page, not
/// api.github.com: the API answers 60 anonymous requests an hour per IP
/// address, shared by every program behind the same router, and past that
/// every check failed with 403. The redirect from `releases/latest` to the
/// newest tag carries the same answer without that limit.
fn fetch_latest_version() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .build()
        .into();
    let resp = agent
        .get("https://github.com/thorstenfoltz/octa/releases/latest")
        .header("User-Agent", &format!("octa/{}", VERSION))
        .call()
        .map_err(|e| failed("errs.upd_request_failed", e))?;
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            octa::i18n::t("errs.upd_no_release")
                .replace("{detail}", &format!("HTTP {}", resp.status().as_u16()))
        })?;
    parse_release(location)
}

/// True when `latest` is a higher version than `current`, compared number by
/// number so `0.21.10` beats `0.21.9`. Equal or lower is never an update: a
/// copy already on the newest release, or ahead of it, hears nothing.
fn is_newer(latest: &str, current: &str) -> bool {
    let numbers = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    };
    numbers(latest) > numbers(current)
}

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
    /// Ask GitHub for the latest release. On a Store (MSIX) copy only the
    /// start-up check calls it, and only to announce a version, since the
    /// Store does the installing. The Help entry that opens the update
    /// dialog is hidden there (`ui::toolbar::help_menu`). Any gate belongs
    /// to the callers, never to this function: returning early here would
    /// leave a caller's dialog spinning on "Checking..." forever.
    pub(crate) fn check_for_updates(&self, ctx: &egui::Context) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        *state.lock().unwrap() = UpdateState::Checking;
        std::thread::spawn(move || {
            let result = fetch_latest_version();

            let mut s = state.lock().unwrap();
            match result {
                Ok(version) if is_newer(&version, VERSION) => {
                    *s = UpdateState::Available { version }
                }
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
        let current_exe = std::env::current_exe().map_err(|e| failed("errs.upd_current_exe", e))?;

        #[cfg(target_os = "linux")]
        {
            let archive_name = format!("octa-{new_version}-linux-x86_64.tar.gz");
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{new_version}/{archive_name}"
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| failed("errs.upd_download_failed", e))?
                .body_mut()
                .with_config()
                .limit(UPDATE_BODY_LIMIT)
                .read_to_vec()
                .map_err(|e| failed("errs.upd_read_failed", e))?;

            verify_archive_checksum(new_version, &archive_name, &bytes)?;

            let staging_path = std::env::temp_dir().join(format!(
                "octa-update-{}-{}",
                new_version,
                std::process::id()
            ));
            let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
            let mut archive = tar::Archive::new(decoder);
            let binary_name = format!("octa-{}-linux-x86_64/octa", new_version);

            let mut found = false;
            for entry in archive
                .entries()
                .map_err(|e| failed("errs.upd_archive_failed", e))?
            {
                let mut entry = entry.map_err(|e| failed("errs.upd_archive_failed", e))?;
                let path = entry
                    .path()
                    .map_err(|e| failed("errs.upd_archive_failed", e))?
                    .to_path_buf();
                if path.to_string_lossy() == binary_name {
                    let mut tmp_file = std::fs::File::create(&staging_path)
                        .map_err(|e| failed("errs.upd_staging_failed", e))?;
                    std::io::copy(&mut entry, &mut tmp_file)
                        .map_err(|e| failed("errs.upd_extract_failed", e))?;

                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))
                        .map_err(|e| failed("errs.upd_chmod_failed", e))?;

                    found = true;
                    break;
                }
            }

            if !found {
                let _ = std::fs::remove_file(&staging_path);
                return Err(
                    octa::i18n::t("errs.upd_not_in_archive").replace("{name}", &binary_name)
                );
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
            let archive_name = format!("octa-{new_version}-windows-x86_64.zip");
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{new_version}/{archive_name}"
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| failed("errs.upd_download_failed", e))?
                .body_mut()
                .with_config()
                .limit(UPDATE_BODY_LIMIT)
                .read_to_vec()
                .map_err(|e| failed("errs.upd_read_failed", e))?;

            verify_archive_checksum(new_version, &archive_name, &bytes)?;

            let cursor = std::io::Cursor::new(bytes);
            let mut archive =
                zip::ZipArchive::new(cursor).map_err(|e| failed("errs.upd_archive_failed", e))?;

            let binary_name = "octa.exe";
            let mut found = false;
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| failed("errs.upd_archive_failed", e))?;
                let name = file
                    .name()
                    .map_err(|e| failed("errs.upd_archive_failed", e))?
                    .into_owned();
                if name.ends_with(binary_name) && !name.ends_with('/') {
                    let tmp_path = current_exe.with_extension("update.exe");
                    let mut tmp_file = std::fs::File::create(&tmp_path)
                        .map_err(|e| failed("errs.upd_staging_failed", e))?;
                    std::io::copy(&mut file, &mut tmp_file)
                        .map_err(|e| failed("errs.upd_extract_failed", e))?;

                    let old_path = current_exe.with_extension("old.exe");
                    if old_path.exists()
                        && let Err(e) = std::fs::remove_file(&old_path)
                    {
                        let _ = std::fs::remove_file(&tmp_path);
                        return Err(octa::i18n::t("errs.upd_old_leftover")
                            .replace("{path}", &old_path.display().to_string())
                            .replace("{error}", &e.to_string()));
                    }
                    std::fs::rename(&current_exe, &old_path)
                        .map_err(|e| failed("errs.upd_backup_failed", e))?;
                    std::fs::rename(&tmp_path, &current_exe)
                        .map_err(|e| failed("errs.upd_install_failed", e))?;

                    found = true;
                    break;
                }
            }

            if !found {
                return Err(octa::i18n::t("errs.upd_not_in_archive").replace("{name}", binary_name));
            }
        }

        // `cfg!`, not a `#[cfg]` block: on macOS the two blocks above are
        // compiled out, and a block that always returns would leave the
        // `Ok` below unreachable, which CI's `-D warnings` refuses.
        if cfg!(not(any(target_os = "linux", target_os = "windows"))) {
            let _ = current_exe;
            let _ = new_version;
            return Err(
                "Auto-update is not supported on this platform. Please download the latest release from the repository.".to_string(),
            );
        }

        Ok(UpdateOutcome::Installed)
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
