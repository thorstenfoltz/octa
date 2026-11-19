use super::run_in_parallel;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The union hands the downloaded paths to the column reconciler in the
/// order the folder listed them, so the pool must return results in INPUT
/// order even though they finish in whatever order the network allows.
/// Reversing the work time makes completion order the opposite of input
/// order, so a pool that returned results as they landed would fail here.
#[test]
fn results_come_back_in_input_order_not_completion_order() {
    let jobs: Vec<usize> = (0..24).collect();
    let out = run_in_parallel(&jobs, 8, |n| {
        std::thread::sleep(Duration::from_millis((24 - *n) as u64));
        n * 2
    });
    let got: Vec<usize> = out
        .into_iter()
        .map(|o| o.expect("no worker panicked"))
        .collect();
    assert_eq!(got, (0..24).map(|n| n * 2).collect::<Vec<_>>());
}

/// Every job runs exactly once. A cursor that used `load` instead of the
/// value `fetch_add` returned would double-run some and skip others.
#[test]
fn every_job_runs_exactly_once() {
    let jobs: Vec<usize> = (0..200).collect();
    let runs: Vec<AtomicUsize> = (0..200).map(|_| AtomicUsize::new(0)).collect();
    run_in_parallel(&jobs, 8, |n| runs[*n].fetch_add(1, Ordering::SeqCst));
    for (i, count) in runs.iter().enumerate() {
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "job {i} ran the wrong number of times"
        );
    }
}

/// The whole point: jobs overlap. And they overlap no more than the limit,
/// because a burst of unbounded requests is what the cloud providers rate
/// limit.
#[test]
fn jobs_overlap_but_never_exceed_the_limit() {
    let jobs: Vec<usize> = (0..32).collect();
    let inflight = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    run_in_parallel(&jobs, 4, |_| {
        let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(5));
        inflight.fetch_sub(1, Ordering::SeqCst);
    });
    let peak = peak.load(Ordering::SeqCst);
    assert!(
        peak > 1,
        "the pool never ran two jobs at once (peak {peak})"
    );
    assert!(
        peak <= 4,
        "the pool exceeded its concurrency limit (peak {peak})"
    );
}

/// Degenerate inputs: no jobs spawns nothing, and fewer jobs than workers
/// must not spawn idle threads or index past the end.
#[test]
fn empty_and_short_job_lists_are_handled() {
    let none: Vec<usize> = Vec::new();
    assert!(run_in_parallel(&none, 8, |n| *n).is_empty());

    let two = vec![7usize, 9];
    let out: Vec<usize> = run_in_parallel(&two, 8, |n| *n)
        .into_iter()
        .map(|o| o.expect("no worker panicked"))
        .collect();
    assert_eq!(out, vec![7, 9]);
}

/// A concurrency of zero would otherwise spawn no workers and hang the
/// download forever; it is clamped to one.
#[test]
fn zero_concurrency_still_runs_the_jobs() {
    let jobs = vec![1usize, 2, 3];
    let out: Vec<usize> = run_in_parallel(&jobs, 0, |n| n * 10)
        .into_iter()
        .map(|o| o.expect("no worker panicked"))
        .collect();
    assert_eq!(out, vec![10, 20, 30]);
}
