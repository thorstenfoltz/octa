use super::*;

#[test]
fn auth_options_follow_engine() {
    for &e in DbEngine::ALL {
        assert_eq!(auth_options(e), e.supported_auth(), "{e:?}");
    }
}

#[test]
fn secret_bearing_kinds() {
    assert!(auth_uses_secret(DbAuthKind::Token));
    assert!(auth_uses_secret(DbAuthKind::OAuthClientCredentials));
    assert!(!auth_uses_secret(DbAuthKind::GcpAdc));
    assert!(!auth_uses_secret(DbAuthKind::OAuthBrowser));
}
