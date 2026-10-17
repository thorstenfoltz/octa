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
