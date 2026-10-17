use super::*;

#[test]
fn nothing_recorded_or_a_dead_port_is_no_hand_over() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!try_hand_over(dir.path(), &[]));
    let dead = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);
    std::fs::write(dir.path().join(FILE_NAME), format!("{port} abc\n")).unwrap();
    assert!(!try_hand_over(dir.path(), &[PathBuf::from("x.csv")]));
}

#[test]
fn a_wrong_token_is_refused_and_delivers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let l = listen(dir.path(), || {}).unwrap();
    let (port, _) = read_file(&dir.path().join(FILE_NAME)).unwrap();
    std::fs::write(
        dir.path().join(FILE_NAME),
        format!("{port} not-the-token\n"),
    )
    .unwrap();
    assert!(!try_hand_over(dir.path(), &[PathBuf::from("/tmp/x.csv")]));
    assert!(
        l.rx.recv_timeout(std::time::Duration::from_millis(300))
            .is_err()
    );
}

#[test]
fn a_second_listener_defers_to_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let first = listen(dir.path(), || {}).unwrap();
    assert!(listen(dir.path(), || {}).is_none());
    assert!(try_hand_over(dir.path(), &[]), "a ping reaches the first");
    first.release();
}

#[cfg(unix)]
#[test]
fn the_instance_file_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let _l = listen(dir.path(), || {}).unwrap();
    let mode = std::fs::metadata(dir.path().join(FILE_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
