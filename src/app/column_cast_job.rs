//! An answer to the date or number picker, turned into dates or numbers on a
//! worker: parsing every value of a five-million-row column held the window
//! for about a second.
//!
//! The column is copied (what the user sees, pending edits included, as the
//! picker's own conversion reads it) and the worker runs the very function
//! the picker used to run in place, on a one-column table. While any of these
//! runs the tab is read-only, does not sort and does not stream in more rows
//! (`TabState::holds_rows`), so the answer still lines up with the rows when
//! it comes back by row number.

use eframe::egui;
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::i18n::t;

use crate::app::bg_rows::ChunkCast;
use crate::app::pushdown::{ServerTask, spawn_local_task};
use crate::app::state::OctaApp;

/// One picker answer being applied to one column.
pub(crate) struct ColumnCastJob {
    col_idx: usize,
    /// The column as it was sent, which it must still be on landing.
    column: ColumnInfo,
    rows: usize,
    cast: ChunkCast,
    task: ServerTask<(Vec<CellValue>, String)>,
}

/// The worker: the picker's conversion on a one-column copy. Returns the
/// converted values and the column's new type.
fn convert(
    column: ColumnInfo,
    values: Vec<CellValue>,
    cast: ChunkCast,
) -> (Vec<CellValue>, String) {
    let mut table = DataTable::empty();
    table.columns = vec![column];
    table.rows = values.into_iter().map(|v| vec![v]).collect();
    match cast {
        ChunkCast::Date(crate::app::file_io::passes::Promotion::Date(l)) => {
            octa::data::date_infer::apply_date(&mut table, 0, l)
        }
        ChunkCast::Date(crate::app::file_io::passes::Promotion::DateTime(l)) => {
            octa::data::date_infer::apply_datetime(&mut table, 0, l)
        }
        ChunkCast::Number(style) => octa::data::num_parse::apply_style(&mut table, 0, style),
    }
    let data_type = std::mem::take(&mut table.columns[0].data_type);
    let values = table
        .rows
        .into_iter()
        .map(|mut r| r.pop().unwrap_or(CellValue::Null))
        .collect();
    (values, data_type)
}

impl OctaApp {
    /// Convert column `col_idx` of tab `tab_idx` the way a picker answer
    /// says, in the background.
    pub(crate) fn start_column_cast(&mut self, tab_idx: usize, col_idx: usize, cast: ChunkCast) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        let table = &tab.table;
        let Some(column) = table.columns.get(col_idx).cloned() else {
            return;
        };
        let rows = table.row_count();
        // The date conversion reads through pending edits, the number one
        // reads the stored cells: each copy reads what its conversion read.
        let values: Vec<CellValue> = match cast {
            ChunkCast::Date(_) => (0..rows)
                .map(|r| table.get(r, col_idx).cloned().unwrap_or(CellValue::Null))
                .collect(),
            ChunkCast::Number(_) => table
                .rows
                .iter()
                .map(|r| r.get(col_idx).cloned().unwrap_or(CellValue::Null))
                .collect(),
        };
        let sent = column.clone();
        let task = spawn_local_task(move || convert(sent, values, cast));
        tab.column_casts.push(ColumnCastJob {
            col_idx,
            column,
            rows,
            cast,
            task,
        });
    }

    /// Once per frame: put every finished picker answer into its column.
    pub(crate) fn poll_column_casts(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let mut i = 0;
            while i < self.tabs[idx].column_casts.len() {
                let Some(outcome) = self.tabs[idx].column_casts[i].task.poll_result() else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    i += 1;
                    continue;
                };
                let job = self.tabs[idx].column_casts.remove(i);
                match outcome {
                    Ok((values, data_type)) => self.finish_column_cast(idx, job, values, data_type),
                    Err(e) => {
                        self.status_message = Some((
                            t("column_cast.failed")
                                .replace("{column}", &job.column.name)
                                .replace("{error}", &e),
                            std::time::Instant::now(),
                        ));
                    }
                }
            }
        }
    }

    fn finish_column_cast(
        &mut self,
        idx: usize,
        job: ColumnCastJob,
        values: Vec<CellValue>,
        data_type: String,
    ) {
        let tab = &mut self.tabs[idx];
        let unchanged = tab.table.row_count() == job.rows
            && tab.table.columns.get(job.col_idx) == Some(&job.column);
        if !unchanged {
            self.status_message = Some((
                t("column_cast.changed").replace("{column}", &job.column.name),
                std::time::Instant::now(),
            ));
            return;
        }
        for (row, value) in tab.table.rows.iter_mut().zip(values) {
            if let Some(slot) = row.get_mut(job.col_idx) {
                *slot = value;
            }
        }
        tab.table.columns[job.col_idx].data_type = data_type;
        tab.chunk_casts.push((job.col_idx, job.cast));
        // Answered European: saves write the decimal comma back.
        if matches!(
            job.cast,
            ChunkCast::Number(octa::data::num_parse::NumberStyle::European)
        ) {
            tab.csv_decimal_comma = true;
        }
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }
}

#[cfg(test)]
#[path = "column_cast_job_tests.rs"]
mod tests;
