use super::*;

#[test]
fn detects_unclean_then_marks() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!mark_running_in(dir.path()), "first run is clean");
    assert!(mark_running_in(dir.path()), "lingering lock => unclean");
}

#[test]
fn panic_message_handles_str_and_string() {
    let s: &str = "boom";
    assert_eq!(panic_message(&s), "boom");
    let owned: String = "kaboom".to_string();
    assert_eq!(panic_message(&owned), "kaboom");
}
