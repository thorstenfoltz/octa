//! One guard for the "worker thread owns a boolean flag" pattern.
//!
//! Every background job in the app parks a flag somewhere the UI polls -
//! `bg_loading_done`, a panel's `running` - and sets it back by hand on the
//! last line of the closure. An error path that returns early, or a panic,
//! skips that line and the flag stays wrong forever: the spinner never stops,
//! `request_repaint` fires every frame (constant CPU, flat battery), and the
//! panel refuses to start another scan for the rest of the session.
//!
//! Holding one of these instead means the flag is set when the closure ends,
//! however it ends.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct FlagOnDrop {
    flag: Arc<AtomicBool>,
    value: bool,
}

impl FlagOnDrop {
    /// Store `value` into `flag` when the guard goes out of scope.
    pub(crate) fn new(flag: Arc<AtomicBool>, value: bool) -> Self {
        Self { flag, value }
    }
}

impl Drop for FlagOnDrop {
    fn drop(&mut self) {
        self.flag.store(self.value, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_set_even_when_the_worker_panics() {
        let flag = Arc::new(AtomicBool::new(false));
        let moved = Arc::clone(&flag);
        let handle = std::thread::spawn(move || {
            let _guard = FlagOnDrop::new(moved, true);
            panic!("worker exploded");
        });
        assert!(handle.join().is_err());
        assert!(flag.load(Ordering::Relaxed), "guard did not run");
    }
}
