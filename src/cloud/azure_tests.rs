use super::*;
use crate::cloud::CloudKind;

fn azure_conn() -> CloudConnection {
    let mut c = CloudConnection::ephemeral_azure("acct", "cont");
    c.kind = CloudKind::AzureBlob;
    c
}

#[test]
fn builds_with_cli_creds_without_network() {
    // `with_use_azure_cli` does not invoke `az` at build time, so this is
    // hermetic.
    let provider = build_azure_provider(&azure_conn(), &AzureCreds::Cli);
    assert!(provider.is_ok());
}

#[test]
fn builds_with_bearer_token() {
    let provider = build_azure_provider(&azure_conn(), &AzureCreds::BearerToken("tok".into()));
    assert!(provider.is_ok());
}

#[test]
fn builds_with_access_key() {
    let provider = build_azure_provider(&azure_conn(), &AzureCreds::AccessKey("a2V5".into()));
    assert!(provider.is_ok());
}

#[test]
fn missing_account_is_an_error() {
    let mut c = azure_conn();
    c.account = None;
    assert!(build_azure_provider(&c, &AzureCreds::Cli).is_err());
}

#[test]
fn builds_anonymous_public_container_without_creds() {
    // A public, read-only container skips signing; creds are ignored.
    let mut c = azure_conn();
    c.anonymous = true;
    assert!(build_azure_provider(&c, &AzureCreds::Cli).is_ok());
}
