//! The real binary hands its file to a listener started here, with a temp
//! OCTA_CONFIG_DIR, and exits without opening a window.

use std::time::Duration;

#[test]
fn a_second_start_hands_its_file_to_the_running_octa() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let file = dir.path().join("sales.csv");
    std::fs::write(&file, "a\n1\n").unwrap();
    let listener = octa::single_instance::listen(&config, || {}).expect("first instance listens");

    // A relative path, so making it absolute is part of what is tested.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_octa"))
        .arg("sales.csv")
        .current_dir(dir.path())
        .env("OCTA_CONFIG_DIR", &config)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let got = listener
        .rx
        .recv_timeout(Duration::from_secs(5))
        .expect("paths arrive");
    assert_eq!(got.len(), 1);
    assert_eq!(
        std::fs::canonicalize(&got[0]).unwrap(),
        std::fs::canonicalize(&file).unwrap()
    );
    listener.release();
    assert!(
        !config.join("instance").exists(),
        "release removes its own file"
    );
}
