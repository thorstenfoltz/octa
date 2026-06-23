//! A size-capped, rotating log file writer. Appends to the target path and,
//! when a write crosses the cap, renames it to `<name>.log.1` (overwriting any
//! previous one) and starts a fresh file. At most two files exist, bounding the
//! on-disk log at about twice the cap. `tracing-appender` is not used: its
//! rolling is time-based, not size-based, so it would not bound the size.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Default cap: 5 MB. With one rotated file, at most ~10 MB on disk.
pub const LOG_SIZE_CAP: u64 = 5 * 1024 * 1024;

struct Inner {
    file: File,
    path: PathBuf,
    written: u64,
    cap: u64,
}

/// Clone-able handle to the rotating file. Cloning shares the same file behind a
/// mutex, so it doubles as a `tracing` `MakeWriter` via `move || writer.clone()`.
#[derive(Clone)]
pub struct RotatingWriter(Arc<Mutex<Inner>>);

impl RotatingWriter {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::with_cap(path, LOG_SIZE_CAP)
    }

    pub fn with_cap(path: &Path, cap: u64) -> io::Result<Self> {
        // If the existing log is already at/over the cap, rotate before opening
        // so a restart cannot keep appending past the bound.
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) >= cap {
            rotate_file(path)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let written = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Ok(Self(Arc::new(Mutex::new(Inner {
            file,
            path: path.to_path_buf(),
            written,
            cap,
        }))))
    }
}

fn rotate_file(path: &Path) -> io::Result<()> {
    let bak = path.with_extension("log.1");
    // Windows `rename` fails if the destination exists, so remove it first.
    let _ = std::fs::remove_file(&bak);
    std::fs::rename(path, &bak)
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut g = self.0.lock().unwrap();
        let n = g.file.write(buf)?;
        g.written += n as u64;
        if g.written >= g.cap {
            g.file.flush()?;
            rotate_file(&g.path)?;
            g.file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&g.path)?;
            g.written = 0;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_when_cap_exceeded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("octa.log");
        let mut w = RotatingWriter::with_cap(&path, 100).unwrap();

        w.write_all(&[b'x'; 60]).unwrap();
        assert!(
            !path.with_extension("log.1").exists(),
            "should not rotate before the cap"
        );

        w.write_all(&[b'y'; 60]).unwrap(); // crosses 100 -> rotate
        assert!(
            path.with_extension("log.1").exists(),
            "rotated file should exist after crossing the cap"
        );
        let live = std::fs::metadata(&path).unwrap().len();
        assert!(live < 100, "live file should restart small, got {live}");
    }

    #[test]
    fn total_size_stays_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("octa.log");
        let mut w = RotatingWriter::with_cap(&path, 100).unwrap();
        for _ in 0..50 {
            w.write_all(&[b'z'; 40]).unwrap();
        }
        let live = std::fs::metadata(&path).unwrap().len();
        let bak = std::fs::metadata(path.with_extension("log.1"))
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(
            live + bak <= 2 * 100 + 40,
            "total {} exceeded bound",
            live + bak
        );
    }
}
