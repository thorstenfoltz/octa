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
    }
}

/// Whether the cloud's CLI is on `PATH` (gates the Sign in button).
pub fn cli_available(kind: CloudKind) -> bool {
    which_on_path(cli_binary(kind))
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
mod tests {
    use super::*;

    #[test]
    fn login_command_uses_the_right_program_and_args() {
        let c = login_command(CloudKind::S3, Some("prod"));
        assert_eq!(c.get_program().to_string_lossy(), "aws");
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, ["sso", "login", "--profile", "prod"]);

        let az = login_command(CloudKind::AzureBlob, None);
        assert_eq!(az.get_program().to_string_lossy(), "az");
        let az_args: Vec<String> = az
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(az_args, ["login"]);

        let gs = login_command(CloudKind::Gcs, None);
        let gs_args: Vec<String> = gs
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(gs_args, ["auth", "application-default", "login"]);
    }

    #[test]
    fn cli_binary_names_match_the_cloud() {
        assert_eq!(cli_binary(CloudKind::S3), "aws");
        assert_eq!(cli_binary(CloudKind::AzureBlob), "az");
        assert_eq!(cli_binary(CloudKind::Gcs), "gcloud");
    }
}
