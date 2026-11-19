use super::*;

#[test]
fn detects_unclean_then_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (unclean, held) = mark_running_in(dir.path());
    assert!(!unclean, "first run is clean");
    // A crash: the process dies holding the sentinel, the system frees the
    // lock and the text stays.
    drop(held.unwrap());
    let (unclean, held) = mark_running_in(dir.path());
    assert!(unclean, "lingering sentinel => unclean");
    assert!(held.is_some());
}

/// A second Octa window started while the first runs used to report a crash,
/// and the first to close deleted the other's sentinel.
#[test]
fn a_second_live_window_is_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (_, first) = mark_running_in(dir.path());
    let first = first.unwrap();
    let (unclean, second) = mark_running_in(dir.path());
    assert!(!unclean, "a live owner is not a crash");
    assert!(second.is_none(), "the second window holds no sentinel");
    // Read through the holder: Windows locks are enforced, so a fresh
    // handle may not read the file while it is held.
    use std::io::{Read, Seek};
    let mut text = String::new();
    (&first).rewind().unwrap();
    (&first).read_to_string(&mut text).unwrap();
    assert_eq!(text, format!("pid {}", std::process::id()));
    clear_held(&first);
    drop(first);
}

#[test]
fn a_clean_exit_makes_the_next_start_clean() {
    let dir = tempfile::tempdir().unwrap();
    let (_, held) = mark_running_in(dir.path());
    let held = held.unwrap();
    clear_held(&held);
    drop(held);
    assert!(
        dir.path().join("running.lock").exists(),
        "kept, only emptied"
    );
    let (unclean, _) = mark_running_in(dir.path());
    assert!(!unclean);
}

/// A sentinel written by a version before the lock (plain `pid N`, nobody
/// holding it) still counts as an unclean exit.
#[test]
fn an_old_sentinel_still_counts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("running.lock"), "pid 1").unwrap();
    let (unclean, _) = mark_running_in(dir.path());
    assert!(unclean);
}

#[test]
fn panic_message_handles_str_and_string() {
    let s: &str = "boom";
    assert_eq!(panic_message(&s), "boom");
    let owned: String = "kaboom".to_string();
    assert_eq!(panic_message(&owned), "kaboom");
}

/// A panic inside `caught` is an error the caller handles, and the hook
/// sees it as caught, so no crash report waits at the next start. Outside,
/// and after it, a panic is a crash again.
#[test]
fn a_caught_panic_is_an_error_and_no_crash() {
    assert!(!panic_is_caught());
    assert_eq!(caught(|| 7), Ok(7));
    assert_eq!(caught(panic_is_caught), Ok(true));
    assert_eq!(
        caught(|| -> () { panic!("bad buffer") }),
        Err("bad buffer".to_string())
    );
    assert!(!panic_is_caught(), "the flag outlived the panic");
    // Nested: the inner one leaves the outer one caught.
    assert_eq!(
        caught(|| {
            let _ = caught(|| -> () { panic!("inner") });
            panic_is_caught()
        }),
        Ok(true)
    );
}
