use super::*;
use crate::reference::test_server::{serve, status};
use std::time::{Duration, Instant};

fn wait(jobs: &Jobs) {
    let t = Instant::now();
    while jobs.busy() && t.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_failed_download_is_reported_once_and_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _s) = serve(vec![status(503)]);
    let jobs = Jobs::default();
    assert!(jobs.start_in(
        dir.path().to_path_buf(),
        Some(base),
        Dataset::Inflation,
        vec![]
    ));
    wait(&jobs);
    let errors = jobs.take_errors();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("503"), "{}", errors[0]);
    assert!(jobs.take_errors().is_empty(), "reported once");
    assert!(jobs.last_error(Dataset::Inflation).is_some());
}

#[test]
fn the_same_dataset_does_not_run_twice() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _s) = serve(vec![status(503)]);
    let jobs = Jobs::default();
    {
        jobs.0.lock().unwrap().running.insert(Dataset::Inflation);
    }
    assert!(!jobs.start_in(
        dir.path().to_path_buf(),
        Some(base),
        Dataset::Inflation,
        vec![]
    ));
}
