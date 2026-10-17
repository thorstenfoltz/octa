//! GUI-triggered browser sign-in by shelling out to the official cloud CLI,
//! plus CLI-presence detection used to enable/disable the Sign in button.

use std::process::Command;

use anyhow::{Context, Result};

use super::CloudKind;

/// The official CLI binary for a cloud.
pub fn cli_binary(kind: CloudKind) -> &'static str {
    match kind {
        CloudKind::S3 => "aws",
        CloudKind::AzureBlob => "az",
        CloudKind::Gcs => "gcloud",
        CloudKind::Sftp | CloudKind::WebDav => "",
    }
}

/// Whether the cloud's CLI is on `PATH` (gates the Sign in button).
pub fn cli_available(kind: CloudKind) -> bool {
    !kind.is_file_server() && which_on_path(cli_binary(kind))
}

/// Build (but do not run) the browser sign-in command for a cloud.
pub fn login_command(kind: CloudKind, profile: Option<&str>) -> Command {
    match kind {
        CloudKind::S3 => {
            let mut c = Command::new("aws");
            c.arg("sso").arg("login");
            if let Some(p) = profile {
                c.arg("--profile").arg(p);
            }
            c
        }
        CloudKind::AzureBlob => {
            let mut c = Command::new("az");
            c.arg("login");
            c
        }
        CloudKind::Gcs => {
            let mut c = Command::new("gcloud");
            c.arg("auth").arg("application-default").arg("login");
            c
        }
        // Never run: the sidebar offers no CLI sign-in for file servers (is_file_server).
        CloudKind::Sftp | CloudKind::WebDav => Command::new(cli_binary(kind)),
    }
}

/// Run the browser sign-in for a cloud, waiting for the CLI to finish. The CLI
/// opens the system browser and performs the SSO/MFA flow.
pub fn interactive_login(kind: CloudKind, profile: Option<&str>) -> Result<()> {
    let status = login_command(kind, profile)
        .status()
        .with_context(|| format!("running {} sign-in", cli_binary(kind)))?;
    if !status.success() {
        anyhow::bail!("{} sign-in exited with {}", cli_binary(kind), status);
    }
    Ok(())
}

/// Minimal `which`: is `bin` runnable from `PATH`? Uses the platform tool so we
/// add no dependency.
fn which_on_path(bin: &str) -> bool {
    let probe = if cfg!(windows) { "where" } else { "which" };
    Command::new(probe)
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
