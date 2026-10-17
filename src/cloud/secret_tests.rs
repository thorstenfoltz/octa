use super::*;

#[test]
fn s3_secret_roundtrips_through_json() {
    let s = CloudSecret::S3 {
        access_key_id: "AKIA".into(),
        secret_access_key: "sk".into(),
        token: Some("tok".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: CloudSecret = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn azure_variants_roundtrip() {
    for s in [
        CloudSecret::AzureKey("k".into()),
        CloudSecret::AzureSas("sv=1".into()),
    ] {
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<CloudSecret>(&json).unwrap(), s);
    }
}

#[test]
fn maps_to_provider_creds() {
    let s3 = CloudSecret::S3 {
        access_key_id: "AKIA".into(),
        secret_access_key: "sk".into(),
        token: None,
    };
    assert!(matches!(s3.to_provider_creds(), ProviderCreds::S3(_)));
    assert!(matches!(
        CloudSecret::AzureKey("k".into()).to_provider_creds(),
        ProviderCreds::Azure(AzureCreds::AccessKey(_))
    ));
    assert!(matches!(
        CloudSecret::AzureSas("sv=1".into()).to_provider_creds(),
        ProviderCreds::Azure(AzureCreds::Sas(_))
    ));
}

#[test]
fn a_password_secret_maps_to_password_creds() {
    let s = CloudSecret::Password("pw".into());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<CloudSecret>(&json).unwrap(), s);
    assert!(matches!(s.to_provider_creds(), ProviderCreds::Password(p) if p == "pw"));
}
