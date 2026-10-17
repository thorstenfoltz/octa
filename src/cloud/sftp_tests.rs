use super::*;
use crate::ssh::SshAuth;

fn target(auth: SshAuth) -> SshTarget {
    SshTarget {
        host: "files.example.com".into(),
        port: 22,
        username: "anna".into(),
        auth,
        accept_new_host_key: false,
    }
}

#[test]
fn keys_map_to_absolute_server_paths() {
    assert_eq!(remote(""), "/");
    assert_eq!(remote("home/anna/x.csv"), "/home/anna/x.csv");
    assert_eq!(remote("home/anna/"), "/home/anna");
}

#[test]
fn listed_children_get_object_style_keys() {
    assert_eq!(child_key("", "home", true), "home/");
    assert_eq!(child_key("home/anna/", "x.csv", false), "home/anna/x.csv");
}

#[test]
fn a_save_is_written_beside_its_target_first() {
    assert_eq!(temp_sibling("upload/a.csv"), "/upload/.a.csv.octa-part");
    assert_eq!(temp_sibling("a.csv"), "/.a.csv.octa-part");
    assert_eq!(parent_dir("/upload/sub/a.csv"), "/upload/sub");
    assert_eq!(parent_dir("/a.csv"), "");
}

#[test]
fn only_a_broken_connection_triggers_a_reconnect() {
    use russh_sftp::client::error::Error as SftpError;
    use russh_sftp::protocol::{Status, StatusCode};
    let refused = SftpError::Status(Status {
        id: 1,
        status_code: StatusCode::NoSuchFile,
        error_message: "no such file".into(),
        language_tag: "en".into(),
    });
    let refused = anyhow::Error::new(refused).context("downloading /x.csv");
    assert!(server_answered(&refused));
    let dropped = anyhow::Error::new(SftpError::Timeout).context("listing /");
    assert!(!server_answered(&dropped));
}

#[test]
fn a_different_sign_in_is_a_different_session() {
    let agent = SftpProvider::new(target(SshAuth::Agent), None);
    let password = SftpProvider::new(target(SshAuth::Password), None);
    assert_ne!(agent.key, password.key);
}
