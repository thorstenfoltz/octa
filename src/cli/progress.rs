//! A one-line progress report on stderr for the CLI actions that loop.
//!
//! `--batch-convert` over 500 files used to print nothing at all until the run
//! finished, so a slow file and a hung one looked identical. This prints the
//! count, what it is on, and an ETA, rewriting the same line.
//!
//! **Silent when stderr is not a terminal.** A pipeline redirecting stderr to a
//! log gets the summary lines it always got and none of the carriage returns.
//! stdout is never touched: it carries the machine-readable listing.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use octa::ui::status_bar::format_number;

/// A live progress line. Construct one, call [`Progress::item`] (or
/// [`Progress::rows`]) as work completes, and [`Progress::finish`] at the end.
pub struct Progress {
    total: Option<usize>,
    started: Instant,
    /// False when stderr is redirected, which makes every method a no-op.
    enabled: bool,
    /// Whether anything has been written, so `finish` only clears a line that
    /// exists.
    dirty: bool,
}

impl Progress {
    /// Start timing. `total` is the number of items when it is known up front;
    /// `None` reports a running count instead (a database copy knows how many
    /// rows it has moved, not how many are coming).
    pub fn start(total: Option<usize>) -> Self {
        Self {
            total,
            started: Instant::now(),
            enabled: std::io::stderr().is_terminal(),
            dirty: false,
        }
    }

    /// Report that `done` items are finished, the last of them `name`.
    pub fn item(&mut self, done: usize, name: &str) {
        let line = item_line(done, self.total, name, self.started.elapsed());
        self.write(&line);
    }

    /// Report a running count with no known total, e.g. rows copied.
    pub fn rows(&mut self, done: usize, unit: &str) {
        let line = format!(
            "{} {unit}  {}",
            format_number(done),
            clock(self.started.elapsed())
        );
        self.write(&line);
    }

    /// Erase the line so the caller's own summary starts on a clean row.
    pub fn finish(&mut self) {
        if self.enabled && self.dirty {
            eprint!("\r\x1b[K");
            let _ = std::io::stderr().flush();
            self.dirty = false;
        }
    }

    fn write(&mut self, line: &str) {
        if !self.enabled {
            return;
        }
        // `\x1b[K` erases the rest of the row, so a short line never leaves the
        // tail of a longer one behind it.
        eprint!("\r\x1b[K{line}");
        let _ = std::io::stderr().flush();
        self.dirty = true;
    }
}

/// A path as it should appear in a progress line: the file name alone, since
/// the directory is the same for every item and would push the count off a
/// narrow terminal. Falls back to the whole path when there is no file name.
pub fn short_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The counted line: `[3/500] products.csv  ETA 0:42`.
///
/// Pure so the arithmetic is testable; the ETA is left out until something has
/// finished, because a prediction from zero samples is a lie.
fn item_line(done: usize, total: Option<usize>, name: &str, elapsed: Duration) -> String {
    match total {
        Some(total) if total > 0 => {
            let head = format!("[{done}/{total}] {name}");
            if done == 0 || done >= total {
                head
            } else {
                let per_item = elapsed.as_secs_f64() / done as f64;
                let left = Duration::from_secs_f64(per_item * (total - done) as f64);
                format!("{head}  ETA {}", clock(left))
            }
        }
        _ => format!("[{done}] {name}"),
    }
}

/// `0:42`, or `1:02:03` once it passes an hour.
fn clock(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eta_waits_for_a_first_sample() {
        // Nothing done yet: no ETA, because there is nothing to divide by.
        let line = item_line(0, Some(500), "a.csv", Duration::from_secs(0));
        assert_eq!(line, "[0/500] a.csv");
        // Last item: the run is over, an ETA would be noise.
        assert_eq!(
            item_line(500, Some(500), "z.csv", Duration::from_secs(10)),
            "[500/500] z.csv"
        );
    }

    #[test]
    fn the_eta_extrapolates_from_what_has_run() {
        // 10 of 100 in 10s means 1s each and 90 to go.
        let line = item_line(10, Some(100), "a.csv", Duration::from_secs(10));
        assert_eq!(line, "[10/100] a.csv  ETA 1:30");
    }

    #[test]
    fn an_unknown_total_just_counts() {
        assert_eq!(
            item_line(7, None, "a.csv", Duration::from_secs(3)),
            "[7] a.csv"
        );
    }

    #[test]
    fn the_clock_grows_an_hours_field_only_when_it_needs_one() {
        assert_eq!(clock(Duration::from_secs(9)), "0:09");
        assert_eq!(clock(Duration::from_secs(600)), "10:00");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }
}
