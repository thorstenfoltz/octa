use super::*;

#[test]
fn a_tunnel_saved_before_the_move_still_loads() {
    // The struct moved but its serialised shape must not change: DB
    // connections in existing settings.toml files carry it.
    let t: SshTarget = serde_json::from_str(r#"{"host":"bastion","username":"anna"}"#).unwrap();
    assert_eq!(t.port, 22);
    assert_eq!(t.auth, SshAuth::Agent);
    assert!(!t.accept_new_host_key);
    let back: SshTarget = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
    assert_eq!(back, t);
}

#[test]
fn a_home_relative_key_path_is_expanded() {
    let home = home_dir().expect("tests run with a home directory");
    assert_eq!(
        expand_home("~/.ssh/id_ed25519"),
        home.join(".ssh/id_ed25519")
    );
    assert_eq!(
        expand_home("/etc/key"),
        std::path::PathBuf::from("/etc/key")
    );
    assert_eq!(
        expand_home("relative/key"),
        std::path::PathBuf::from("relative/key")
    );
}
