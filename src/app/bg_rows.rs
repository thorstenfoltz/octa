//! Drain background-loaded rows into the active table, update the status
//! message, evict front rows when memory grows too large, and request
//! repaint while loading is in-flight.

use eframe::egui;

use octa::data::num_parse::{self, NumberStyle};
use octa::data::{CellValue, ColumnInfo};
use octa::ui;

use super::file_io::passes::Promotion;
use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn drain_background_rows(&mut self, ctx: &egui::Context) {
        let Some(buffer) = self.tabs[self.active_tab].bg_row_buffer.clone() else {
            return;
        };
        // A background job is still reading the rows (a save) or will put
        // an answer back by row number; they wait in the buffer until it is
        // done.
        if self.tabs[self.active_tab].holds_rows() {
            return;
        }
        // Read before draining: the worker hands over its last rows and only
        // then says it is done, so a "done" read after the drain could follow
        // rows that arrived in between, and the buffer below would be dropped
        // with them still in it.
        let loading_done = self.tabs[self.active_tab]
            .bg_loading_done
            .load(std::sync::atomic::Ordering::Relaxed);
        let file_exhausted = self.tabs[self.active_tab]
            .bg_file_exhausted
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut drained = false;
        if let Ok(mut buf) = buffer.try_lock()
            && !buf.is_empty()
        {
            let tab = &mut self.tabs[self.active_tab];
            cast_chunk(
                &mut buf,
                &mut tab.table.columns,
                &tab.chunk_casts,
                tab.chunk_trim,
            );
            let table = &mut tab.table;
            let before = table.rows.len();
            let added = buf.len();
            table.rows.append(&mut *buf);
            // A live-database tab streams with a write-back baseline attached.
            // Rows arriving after the first page need their own identity, or
            // `build_write_back_plan` would read every one of them as a row
            // the user inserted and INSERT the whole page back on Save. The
            // tag is `row_offset + index`, which stays unique across an
            // eviction and is exactly `0..n` on a freshly opened tab, the
            // numbering `db_browser` assigns there.
            if let Some(mut meta) = table.db_meta.take() {
                for i in before..table.rows.len() {
                    let tag = (table.row_offset + i) as i64;
                    meta.row_tags.push(Some(tag));
                    meta.original.insert(tag, table.rows[i].clone());
                }
                table.db_meta = Some(meta);
            }
            // Where the next file chunk starts: rows read, not rows held.
            self.tabs[self.active_tab].file_rows_read += added;
            drained = true;
        }
        if drained {
            self.tabs[self.active_tab].filter_dirty = true;
            if self.tabs[self.active_tab].table.total_rows.is_some() {
                let total_loaded = self.tabs[self.active_tab].table.row_offset
                    + self.tabs[self.active_tab].table.row_count();
                let total_fmt = ui::status_bar::format_number(total_loaded);
                if loading_done && file_exhausted {
                    self.status_message = Some((
                        format!("Loaded all {} rows", total_fmt),
                        std::time::Instant::now(),
                    ));
                    self.tabs[self.active_tab].table.total_rows = None;
                    self.tabs[self.active_tab].bg_can_load_more = false;
                } else if loading_done {
                    self.status_message = Some((
                        format!("Loaded {} rows (scroll down to load more)", total_fmt),
                        std::time::Instant::now(),
                    ));
                    self.tabs[self.active_tab].bg_can_load_more = true;
                } else {
                    self.status_message = Some((
                        format!("Loading... {} rows so far", total_fmt),
                        std::time::Instant::now(),
                    ));
                }
            }
            // Evict front rows if we have too many in memory.
            // Not while Load whole table runs: dropping the start of the table
            // while its end arrives would defeat the point of asking for all of it.
            let evict_count = rows_to_evict(
                self.tabs[self.active_tab].table.rows.len(),
                octa::formats::initial_load_rows(),
            );
            if evict_count > 0 && !self.tabs[self.active_tab].loading_all {
                self.tabs[self.active_tab]
                    .table
                    .evict_front_rows(evict_count);
                self.tabs[self.active_tab].filter_dirty = true;
            }
        }
        if loading_done {
            self.tabs[self.active_tab].bg_row_buffer = None;
            // A whole-table load that was cancelled or failed keeps what
            // arrived, and the tab must still be able to scroll for the rest.
            // The scroll path itself stays off after a failed page (see
            // `central_panel::interaction`); this is the explicit request.
            if self.tabs[self.active_tab].loading_all && !file_exhausted {
                self.tabs[self.active_tab].bg_can_load_more = true;
            }
            self.tabs[self.active_tab].loading_all = false;
            // A chunk that came back empty drains nothing, so this cannot live
            // in the `drained` branch above: without it the tab would keep
            // advertising "scroll for more" over a source that has no more.
            // A database table whose row count divides exactly by the page
            // size ends on precisely such an empty page.
            if file_exhausted {
                self.tabs[self.active_tab].table.total_rows = None;
                self.tabs[self.active_tab].bg_can_load_more = false;
            }
        }
        if !loading_done {
            ctx.request_repaint();
        }
    }
}

/// Front rows to drop once streamed chunks have grown the table past half a
/// chunk over the Settings row limit (`cap`): back down to `cap`. Unlimited
/// (`usize::MAX`) never evicts. With the 2M default this is the old fixed
/// 3M -> 2M rule, which ignored a raised limit.
fn rows_to_evict(len: usize, cap: usize) -> usize {
    if len > cap.saturating_add(cap / 2) {
        len - cap
    } else {
        0
    }
}

/// A load-time conversion repeated on rows that stream in later. The load
/// passes promote the first rows' text to dates or numbers; without this, the
/// rows behind them stayed text in a typed column.
#[derive(Clone, Copy)]
pub(crate) enum ChunkCast {
    Date(Promotion),
    Number(NumberStyle),
}

/// Convert `rows` the way the load converted the rows already held: trim
/// text first (when the load trimmed), then each cast whose column still has
/// the type the cast gave it. A Dismiss, undo or type change turns the column
/// back, and then its cast is left out without anyone having to forget it.
/// A value the cast cannot read stays text, never an empty cell.
// ponytail: casts are keyed by column index, as the chunk readers already
// map file column i to table column i; moving columns breaks both.
pub(crate) fn cast_chunk(
    rows: &mut [Vec<CellValue>],
    columns: &mut [ColumnInfo],
    casts: &[(usize, ChunkCast)],
    trim: bool,
) {
    if trim {
        for cell in rows.iter_mut().flatten() {
            if let CellValue::String(s) = cell
                && s.trim().len() != s.len()
            {
                *s = s.trim().to_string();
            }
        }
    }
    for &(col, cast) in casts {
        let Some(column) = columns.get_mut(col) else {
            continue;
        };
        match cast {
            ChunkCast::Date(promotion) => {
                if column.data_type != promotion.data_type() {
                    continue;
                }
                for cell in rows.iter_mut().filter_map(|r| r.get_mut(col)) {
                    match promotion.cell(cell) {
                        Some(CellValue::Null) if !is_blank(cell) => {}
                        Some(v) => *cell = v,
                        None => {}
                    }
                }
            }
            ChunkCast::Number(style) => {
                if !matches!(column.data_type.as_str(), "Int64" | "Float64") {
                    continue;
                }
                for cell in rows.iter_mut().filter_map(|r| r.get_mut(col)) {
                    let CellValue::String(s) = cell else {
                        continue;
                    };
                    if s.trim().is_empty() {
                        *cell = CellValue::Null;
                        continue;
                    }
                    let Some(v) = num_parse::parse_number(s, style) else {
                        continue;
                    };
                    *cell = if v.fract() == 0.0 && v.abs() < i64::MAX as f64 {
                        CellValue::Int(v as i64)
                    } else {
                        // An Int64 column takes a Float cell the way the
                        // readers type a column holding both.
                        column.data_type = "Float64".to_string();
                        CellValue::Float(v)
                    };
                }
            }
        }
    }
}

fn is_blank(cell: &CellValue) -> bool {
    matches!(cell, CellValue::String(s) if s.trim().is_empty())
}

#[cfg(test)]
#[path = "bg_rows_tests.rs"]
mod tests;
