//! The worker pool behind every cloud job that works through many objects:
//! a union's downloads, a folder copy, move or delete, a folder comparison.
//! How many at once is the connection's own number, else the Settings one
//! ([`super::CloudConnection::parallel_or`]).

/// Run `work` over every job, at most `concurrency` at a time, returning the
/// results in **input order** rather than completion order.
///
/// The pool shares one cursor instead of giving each worker a fixed slice:
/// cloud objects are of wildly different sizes, so handing every worker an
/// equal COUNT would leave most of them idle while one finished the big
/// files.
///
/// A job whose worker panicked comes back as `None` rather than shortening
/// the vec, so the caller can always line the results up with its inputs.
pub fn run_in_parallel<T, R>(
    jobs: &[T],
    concurrency: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<Option<R>>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::Ordering;

    if jobs.is_empty() {
        return Vec::new();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let collected: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..concurrency.clamp(1, jobs.len()))
            .map(|_| {
                // Move the REFERENCES into each worker, not the values: the
                // cursor and the closure are shared by the whole pool.
                let (next, work) = (&next, &work);
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        // Take the index from `fetch_add` itself. Reading
                        // `next` back afterwards would race: another worker
                        // can claim the next job in between, and the result
                        // would be filed under someone else's index.
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break;
                        };
                        mine.push((index, work(job)));
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });

    let mut slots: Vec<Option<R>> = (0..jobs.len()).map(|_| None).collect();
    for (index, result) in collected {
        if let Some(slot) = slots.get_mut(index) {
            *slot = Some(result);
        }
    }
    slots
}

#[cfg(test)]
#[path = "parallel_tests.rs"]
mod tests;
