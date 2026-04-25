//! Linux-specific install primitives: writability probes, atomic replace,
//! pkexec invocation, and a minimal `which`.

#![cfg(target_os = "linux")]

use super::InstallError;

/// Check whether the current user can create files next to `exe` — i.e. can
/// we replace the binary without sudo. Creates a short-lived sibling file;
/// this avoids pulling in libc for an `access(W_OK)` call and sidesteps the
/// fact that `std::fs::metadata` only exposes the read-only bit.
pub(crate) fn install_dir_writable(exe: &std::path::Path) -> bool {
    let Some(parent) = exe.parent() else {
        return false;
    };
    let probe = parent.join(format!(".octa-update-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Atomically replace `target` with the already-staged binary at `src`.
/// Rename-across-filesystems can fail with `EXDEV` (tmp is usually a separate
/// mount on Linux), so we fall through to a copy + rename via a sibling temp
/// path when rename fails for that reason.
pub(crate) fn install_replace_unix(
    src: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), InstallError> {
    let parent = target.parent().ok_or_else(|| {
        InstallError::Other(format!(
            "Target has no parent directory: {}",
            target.display()
        ))
    })?;
    let sibling = parent.join(format!(".octa-update-new-{}", std::process::id()));

    if let Err(e) = std::fs::copy(src, &sibling) {
        return Err(match e.kind() {
            std::io::ErrorKind::PermissionDenied => InstallError::PermissionDenied,
            _ => InstallError::Other(format!("Copy failed: {}", e)),
        });
    }

    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o755)) {
        let _ = std::fs::remove_file(&sibling);
        return Err(InstallError::Other(format!("chmod failed: {}", e)));
    }

    if let Err(e) = std::fs::rename(&sibling, target) {
        let _ = std::fs::remove_file(&sibling);
        return Err(match e.kind() {
            std::io::ErrorKind::PermissionDenied => InstallError::PermissionDenied,
            _ => InstallError::Other(format!("Install rename failed: {}", e)),
        });
    }
    Ok(())
}

/// Invoke pkexec to copy `src` into `dest` as root. pkexec is part of polkit
/// and ships with every modern Linux desktop (GNOME/KDE/XFCE), and it shows
/// its own graphical password prompt.
pub(crate) fn run_pkexec_install(
    src: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    if which_bin("pkexec").is_none() {
        return Err(format!(
            "pkexec is not installed. To update manually, run:\n\n    \
             sudo install -m 755 {} {}",
            src.display(),
            dest.display()
        ));
    }

    let status = std::process::Command::new("pkexec")
        .arg("install")
        .arg("-m")
        .arg("755")
        .arg(src)
        .arg(dest)
        .status()
        .map_err(|e| format!("Failed to run pkexec: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        match status.code() {
            Some(126) | Some(127) => {
                Err("Authorization cancelled or failed. The update was not installed.".to_string())
            }
            Some(code) => Err(format!("pkexec install exited with code {}", code)),
            None => Err("pkexec install was terminated by a signal".to_string()),
        }
    }
}

/// Minimal `which` — walks `$PATH` looking for an executable named `name`.
fn which_bin(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
