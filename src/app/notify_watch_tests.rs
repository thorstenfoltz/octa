//! Unit tests for [`notify_watch`](super).

use super::*;

#[test]
fn a_task_counts_while_its_token_lives() {
    let tasks = LongTasks::default();
    assert_eq!(tasks.running(), None);
    let first = tasks.start("Split into files".into());
    let second = tasks.start("Export".into());
    assert_eq!(tasks.running().as_deref(), Some("Split into files"));
    drop(first);
    assert_eq!(tasks.running().as_deref(), Some("Export"));
    // Moved into a thread like a real job: done when the thread ends.
    std::thread::spawn(move || {
        let _task = second;
    })
    .join()
    .unwrap();
    assert_eq!(tasks.running(), None);
}
