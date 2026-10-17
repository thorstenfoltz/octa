use super::*;

#[test]
fn a_folder_is_stored_without_slashes_at_either_end() {
    assert_eq!(form_prefix("/home/anna/"), Some("home/anna/".to_string()));
    assert_eq!(form_prefix("team-a"), Some("team-a/".to_string()));
    assert_eq!(form_prefix("  /  "), None);
}

#[test]
fn an_sftp_form_saves_host_and_port_as_the_bucket() {
    let mut d = SettingsDialog {
        cloud_form_kind: CloudKind::Sftp,
        cloud_form_bucket: "files.example.com".into(),
        cloud_form_port: "2222".into(),
        cloud_form_prefix: "/upload".into(),
        ..Default::default()
    };
    assert_eq!(
        d.sftp_form_location(),
        Ok((
            "files.example.com:2222".to_string(),
            Some("upload/".to_string())
        ))
    );
    d.cloud_form_port = String::new();
    assert_eq!(d.sftp_form_location().unwrap().0, "files.example.com");
    d.cloud_form_port = "0".into();
    assert!(d.sftp_form_location().is_err());
    d.cloud_form_port = "22".into();
    d.cloud_form_bucket = " ".into();
    assert!(d.sftp_form_location().is_err());
}
