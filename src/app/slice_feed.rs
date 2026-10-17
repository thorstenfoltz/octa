//! Hand a worker a table a slice at a time from the window thread, so no
//! whole copy of the table is ever made there. Used by background saves
//! (`file_io::save_job`) and the SQL panel's copy of `data`
//! (`sql_panel::job`).

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use octa::data::num_format::NumberFormat;
use octa::data::{ColumnInfo, DataTable};

use crate::app::file_io::save::round_table_in_place;

/// Cells per slice: about 25 ms of copying on the window thread.
const SLICE_CELLS: usize = 500_000;

/// Rows per slice for a table `cols` wide. A save of no more rows than this
/// is written at once, while the window waits.
pub(crate) fn slice_rows(cols: usize) -> usize {
    (SLICE_CELLS / cols.max(1)).max(1)
}

/// The rows as a feed found them: a replaced table is a new allocation, so
/// the address catches every `tab.table = ..` site; streaming can append a
/// chunk and evict as many rows off the front in one frame (same count,
/// moved offset); Detect dates, the pickers and a banner's Dismiss change
/// values in place and a column type with them. The tab is read-only while it is fed, but
/// anything that still replaces or changes them (another file loaded into
/// the tab, a background pass) stops the job rather than hand over a mix of
/// two tables. Undo counts are left out: colour marks are allowed on a
/// read-only tab and push undo entries.
#[derive(Clone, PartialEq)]
pub(crate) struct RowsStamp {
    at: usize,
    rows: usize,
    offset: usize,
    edits: usize,
    columns: Vec<ColumnInfo>,
}

impl RowsStamp {
    pub(crate) fn of(table: &DataTable) -> Self {
        Self {
            at: table.rows.as_ptr() as usize,
            rows: table.row_count(),
            offset: table.row_offset,
            edits: table.edits.len(),
            columns: table.columns.clone(),
        }
    }
}

/// The window thread's side of handing a worker a table a slice at a time:
/// each [`SliceFeed::feed`] copies what the channel has room for.
pub(crate) struct SliceFeed {
    /// Rows to hand over, by index (a copy of the rows on screen); `None` =
    /// all.
    rows: Option<Vec<usize>>,
    total: usize,
    next: usize,
    slice_rows: usize,
    round: Option<HashMap<usize, NumberFormat>>,
    /// A slice, or the end (`None`), the worker had no room for yet.
    held: Option<Option<DataTable>>,
    /// `None` once the end was handed over, or to stop the worker.
    pub(crate) tx: Option<SyncSender<Option<DataTable>>>,
}

impl SliceFeed {
    /// A feed of `rows` of `table` (all when `None`), rounded by `round`,
    /// and the worker's end of it. One slice queued beside the one being
    /// written and the one held.
    pub(crate) fn new(
        table: &DataTable,
        rows: Option<Vec<usize>>,
        round: Option<HashMap<usize, NumberFormat>>,
        slice_rows: usize,
    ) -> (Self, Receiver<Option<DataTable>>) {
        let (tx, rx) = sync_channel::<Option<DataTable>>(1);
        let feed = Self {
            total: rows.as_ref().map_or(table.row_count(), Vec::len),
            rows,
            next: 0,
            slice_rows: slice_rows.max(1),
            round,
            held: None,
            tx: Some(tx),
        };
        (feed, rx)
    }

    /// Hand the worker what it has room for.
    pub(crate) fn feed(&mut self, table: &DataTable) {
        loop {
            let Some(tx) = self.tx.as_ref() else {
                return;
            };
            let item = match self.held.take() {
                Some(item) => item,
                None if self.next < self.total => {
                    let end = (self.next + self.slice_rows).min(self.total);
                    let picked: Vec<usize> = match &self.rows {
                        Some(rows) => rows[self.next..end].to_vec(),
                        None => (self.next..end).collect(),
                    };
                    // Pending edits are read into the slice, as every save
                    // writes them.
                    let mut slice = table.clone_with_rows(&picked);
                    if let Some(formats) = &self.round {
                        round_table_in_place(&mut slice, formats);
                    }
                    self.next = end;
                    Some(slice)
                }
                None => None,
            };
            let end = item.is_none();
            match tx.try_send(item) {
                Ok(()) if end => self.tx = None,
                Ok(()) => continue,
                Err(TrySendError::Full(item)) => self.held = Some(item),
                // The worker stopped; its error arrives through the task.
                Err(TrySendError::Disconnected(_)) => self.tx = None,
            }
            return;
        }
    }
}
