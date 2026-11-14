//! Which reads on this thread stopped at the row cap.
//!
//! A table holding part of its file looks exactly like a whole one, and most of
//! what is computed from one gives no hint: a check says all clear, a conversion
//! writes a short file. `read_table_auto` writes here, and each surface says so its
//! own way once an action is done (stderr on the CLI, a note in an MCP answer).

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::data::DataTable;

/// A GUI session reads for as long as it runs and nobody takes these.
const MAX_REMEMBERED: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct PartialRead {
    pub path: PathBuf,
    pub rows: usize,
}

thread_local! {
    static READS: RefCell<Vec<PartialRead>> = const { RefCell::new(Vec::new()) };
}

/// Remember `table` when it holds only part of `path`.
pub fn note(path: &Path, table: &DataTable) {
    if !table.is_partial() {
        return;
    }
    READS.with_borrow_mut(|reads| {
        if reads.len() < MAX_REMEMBERED && !reads.iter().any(|r| r.path == path) {
            reads.push(PartialRead {
                path: path.to_path_buf(),
                rows: table.row_count(),
            });
        }
    });
}

/// The reads remembered since the last call.
pub fn take() -> Vec<PartialRead> {
    READS.take()
}

#[cfg(test)]
#[path = "partial_reads_tests.rs"]
mod tests;
