use super::*;
use octa::cloud::{CloudConnection, CloudKind};

fn conn(kind: CloudKind, prefix: Option<&str>) -> CloudConnection {
    CloudConnection {
        kind,
        bucket: "b".into(),
        prefix: prefix.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn a_copy_to_another_connection_lands_in_its_start_folder() {
    let sftp = conn(CloudKind::Sftp, Some("home/anna/"));
    // A file keeps its name, a folder its name and the slash.
    assert_eq!(
        suggest_target_key(&sftp, "data/2026/sales.csv", false, false, "x"),
        "home/anna/sales.csv"
    );
    assert_eq!(
        suggest_target_key(&sftp, "data/2026/", true, false, "x"),
        "home/anna/2026/"
    );
    // Several sources go into the folder itself.
    assert_eq!(
        suggest_target_key(&sftp, "data/a.csv", false, true, "x"),
        "home/anna/"
    );
    // No start folder: the top of the bucket or server.
    let s3 = conn(CloudKind::S3, None);
    assert_eq!(
        suggest_target_key(&s3, "upload/x/t.csv", false, false, "x"),
        "t.csv"
    );
    assert_eq!(suggest_target_key(&s3, "a.csv", false, true, "x"), "");
}

#[test]
fn an_account_level_target_keeps_the_source_path() {
    let account = CloudConnection {
        account_level: true,
        ..conn(CloudKind::S3, None)
    };
    assert_eq!(
        suggest_target_key(
            &account,
            "data/sales.csv",
            false,
            false,
            "bkt/data/sales.csv"
        ),
        "bkt/data/sales.csv"
    );
}
