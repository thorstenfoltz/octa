use super::*;

#[test]
fn spec_parses_pairs_and_rejects_junk() {
    let s = parse_spec("kind=s3, name=Prod , bucket=my-bucket").unwrap();
    assert_eq!(s["kind"], "s3");
    assert_eq!(s["name"], "Prod");
    assert_eq!(s["bucket"], "my-bucket");
    // Keys are case-insensitive, values are not.
    assert_eq!(parse_spec("KIND=s3,name=X").unwrap()["kind"], "s3");

    assert!(parse_spec("").is_err());
    assert!(parse_spec("just-a-word").is_err());
    assert!(parse_spec("=novalue").is_err());
    assert!(parse_spec("kind=s3,kind=gcs").is_err(), "duplicate key");
}

#[test]
fn unknown_keys_are_reported_not_ignored() {
    // A typo must not produce a connection pointing nowhere.
    let s = parse_spec("kind=s3,name=x,buckett=y").unwrap();
    let err = reject_unknown(&s, CLOUD_KEYS).unwrap_err().to_string();
    assert!(err.contains("buckett"), "{err}");
    assert!(err.contains("bucket"), "the message lists the real keys");
}

#[test]
fn flags_take_the_usual_spellings() {
    let s = parse_spec("a=true,b=no,c=1,d=off").unwrap();
    assert!(flag(&s, "a", false).unwrap());
    assert!(!flag(&s, "b", true).unwrap());
    assert!(flag(&s, "c", false).unwrap());
    assert!(!flag(&s, "d", true).unwrap());
    assert!(flag(&s, "missing", true).unwrap(), "absent = default");
    let bad = parse_spec("a=maybe").unwrap();
    assert!(flag(&bad, "a", false).is_err());
}

#[test]
fn s3_secret_accepts_the_shorthand_and_the_stored_json() {
    let s = parse_cloud_secret(CloudKind::S3, "AKIA123:supersecret").unwrap();
    assert_eq!(
        s,
        CloudSecret::S3 {
            access_key_id: "AKIA123".into(),
            secret_access_key: "supersecret".into(),
            token: None,
        }
    );
    // With a session token.
    let s = parse_cloud_secret(CloudKind::S3, "AKIA123:supersecret:tok").unwrap();
    assert!(matches!(s, CloudSecret::S3 { token: Some(t), .. } if t == "tok"));
    // Missing half is an error, not a silent empty key.
    assert!(parse_cloud_secret(CloudKind::S3, "AKIA123").is_err());
    assert!(parse_cloud_secret(CloudKind::S3, "  ").is_err());

    // The JSON Octa itself stores round-trips, for anything exotic.
    let json = serde_json::to_string(&CloudSecret::AzureSas("sv=x&sig=y".into())).unwrap();
    assert_eq!(
        parse_cloud_secret(CloudKind::AzureBlob, &json).unwrap(),
        CloudSecret::AzureSas("sv=x&sig=y".into())
    );
}

#[test]
fn azure_tells_a_sas_from_an_account_key() {
    assert!(matches!(
        parse_cloud_secret(CloudKind::AzureBlob, "?sv=2021&sig=abc").unwrap(),
        CloudSecret::AzureSas(_)
    ));
    assert!(matches!(
        parse_cloud_secret(CloudKind::AzureBlob, "base64accountkey==").unwrap(),
        CloudSecret::AzureKey(_)
    ));
}

#[test]
fn sftp_and_webdav_secrets_are_the_raw_password() {
    assert_eq!(
        parse_cloud_secret(CloudKind::Sftp, " s3cret ").unwrap(),
        CloudSecret::Password("s3cret".into())
    );
    assert_eq!(
        parse_cloud_secret(CloudKind::WebDav, "app-pw").unwrap(),
        CloudSecret::Password("app-pw".into())
    );
}

#[test]
fn gcs_says_why_it_takes_no_secret() {
    let err = parse_cloud_secret(CloudKind::Gcs, "anything")
        .unwrap_err()
        .to_string();
    assert!(err.contains("application-default"), "{err}");
}

#[test]
fn ssl_keys_take_either_engines_words_and_only_where_read() {
    let tls = db_tls(
        DbEngine::Postgres,
        &parse_spec("kind=postgres,sslmode=verify-full,sslrootcert=/ca.pem,hostaddr=127.0.0.1")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(tls.mode, TlsMode::VerifyFull);
    assert_eq!(tls.ca_file.as_deref(), Some("/ca.pem"));
    assert_eq!(tls.connect_via.as_deref(), Some("127.0.0.1"));

    // MySQL's word, and the default when the key is left out.
    let spec = parse_spec("kind=mysql,sslmode=VERIFY_IDENTITY").unwrap();
    assert_eq!(
        db_tls(DbEngine::MySql, &spec).unwrap().mode,
        TlsMode::VerifyFull
    );
    let spec = parse_spec("kind=redshift").unwrap();
    assert_eq!(db_tls(DbEngine::Redshift, &spec).unwrap(), DbTls::default());

    // MySQL cannot do verify-ca; a name in hostaddr is not an IP; other
    // engines do not read these keys at all.
    let spec = parse_spec("kind=mysql,sslmode=verify-ca").unwrap();
    assert!(db_tls(DbEngine::MySql, &spec).is_err());
    let spec = parse_spec("kind=postgres,hostaddr=localhost").unwrap();
    assert!(db_tls(DbEngine::Postgres, &spec).is_err());
    let spec = parse_spec("kind=mssql,sslmode=require").unwrap();
    let err = db_tls(DbEngine::Mssql, &spec).unwrap_err();
    assert!(err.to_string().contains("only applies"), "{err}");
}

#[test]
fn file_database_kinds_are_recognised() {
    assert_eq!(db_engine("SQLite3"), Some(DbEngine::Sqlite));
    assert_eq!(db_engine("duckdb"), Some(DbEngine::DuckDb));
}
