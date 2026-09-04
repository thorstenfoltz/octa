//! Opening a file in large-file mode: the size check, the two open paths and
//! the conversion worker.
//!
//! The decision of *whether* to ask lives here; the question itself is
//! `dialogs::large_file_notice`. Both paths end in `finish_large_open`, which
//! is the only place a `LargeTable` is put onto a tab.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use octa::formats::large::{self, ScanKind};
use octa::i18n::t;

use super::dialogs::large_file_notice::{LargeConvertJob, LargeFileNotice};
use super::state::{OctaApp, TabState};

impl OctaApp {
    /// Does this file want large-file mode, and has the user already answered?
    ///
    /// Returns true when `load_file` should stop: either the question is now
    /// on screen, or the file is already opening in large-file mode.
    pub(crate) fn intercept_large_file(&mut self, path: &Path) -> bool {
        // One bypass per path: the "Open normally" answer comes straight back
        // through `load_file`, and without this it would ask again forever.
        if self.large_check_bypass.as_deref() == Some(path) {
            self.large_check_bypass = None;
            return false;
        }
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        let big_bytes = meta.len() as usize >= self.settings.large_file_min_bytes;
        // A Parquet footer states its row count without reading the data, so
        // a file that is small on disk but enormous when expanded is caught
        // too. Formats that cannot say stay on the byte check alone. The
        // threshold IS the streaming cap: a file with more rows than a normal
        // open would ever load is exactly the file large-file mode is for.
        // With the cap set to unlimited that comparison never fires (the cap
        // is `usize::MAX`), so size alone decides, which is what "unlimited"
        // should mean. Skipped when the size already decided: the footer read
        // is cheap but not free, and it runs on every Parquet open.
        let big_rows = !big_bytes
            && stated_row_count(path)
                .is_some_and(|rows| rows >= octa::formats::initial_load_rows());
        if !big_bytes && !big_rows {
            return false;
        }
        if self.settings.show_large_file_notice {
            self.pending_large_file_notice = Some(LargeFileNotice {
                path: path.to_path_buf(),
                size_bytes: meta.len(),
                needs_conversion: ScanKind::for_path(path).is_none(),
                suppress_future: false,
            });
        } else {
            self.open_large_file(path.to_path_buf());
        }
        true
    }

    /// Open `path` read-only from disk, converting first when the format
    /// cannot be scanned in place.
    pub(crate) fn open_large_file(&mut self, path: PathBuf) {
        if ScanKind::for_path(&path).is_some() {
            self.finish_large_open(path, None);
            return;
        }
        let name = display_name(&path);
        let cancel = Arc::new(AtomicBool::new(false));
        let slot: Arc<Mutex<Option<Result<PathBuf, String>>>> = Arc::new(Mutex::new(None));
        self.large_convert_job = Some(LargeConvertJob {
            name,
            cancel: cancel.clone(),
            slot: slot.clone(),
        });

        // No repaint request needed: the progress window's spinner keeps
        // frames coming, and the slot is drained on each of them.
        std::thread::spawn(move || {
            let noop = |_: usize| {};
            let out =
                large::convert_to_temp_parquet(&path, &noop, &cancel).map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(out);
            }
        });
    }

    /// Put an opened scan onto a tab. `original_name` is set when `scan_path`
    /// is a converted temp file, so the tab is still labelled by the file the
    /// user picked.
    pub(crate) fn finish_large_open(&mut self, scan_path: PathBuf, original_name: Option<String>) {
        let handle = match large::open(&scan_path) {
            Ok(h) => h,
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                return;
            }
        };
        // The first page fills the view; scrolling asks for the rest.
        let first = match handle.page(0, LARGE_PAGE_ROWS, None, None) {
            Ok(t) => t,
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                return;
            }
        };

        let blank = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.table.col_count() == 0 && t.raw_content.is_none() && !t.is_modified())
            .unwrap_or(false);
        if !blank {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
            self.active_tab = self.tabs.len() - 1;
        }
        let label = original_name.unwrap_or_else(|| display_name(&scan_path));
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        tab.table = first;
        tab.table.source_path = Some(scan_path.to_string_lossy().to_string());
        tab.file_stamp = crate::app::file_io::file_stamp(&scan_path);
        tab.custom_tab_label = Some(label);
        tab.parse_error_banner = Some(t("largefile.banner"));
        tab.large_page_key = Some(LargePageKey {
            offset: 0,
            len: LARGE_PAGE_ROWS,
            order: None,
            filter: String::new(),
        });
        tab.large = Some(handle);
        // The view renders from `filtered_rows`, which `recompute_filter` only
        // rebuilds when the tab is dirty. A reused blank tab may already be
        // clean, and would then paint the headers over nothing.
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }
}

/// What the tab's current page was fetched for. Compared before paging again,
/// so scrolling within the same window costs no query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LargePageKey {
    pub(crate) offset: usize,
    pub(crate) len: usize,
    /// Sort column index and direction, as passed to `LargeTable::page`.
    pub(crate) order: Option<(usize, bool)>,
    /// The SQL fragment the view's filters translated to; empty is no filter.
    pub(crate) filter: String,
}

/// Rows fetched per page. Comfortably more than a screen so ordinary scrolling
/// does not re-query on every frame.
pub(crate) const LARGE_PAGE_ROWS: usize = 2_000;

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// The row count a format states without reading its data. Only Parquet does;
/// everything else answers `None` and is judged on bytes alone.
fn stated_row_count(path: &Path) -> Option<usize> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if ext != "parquet" && ext != "pq" {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let reader = parquet::file::serialized_reader::SerializedFileReader::new(file).ok()?;
    use parquet::file::reader::FileReader;
    Some(reader.metadata().file_metadata().num_rows().max(0) as usize)
}

/// Rows the next page keeps from the previous one, so a continuous scroll does
/// not land on a hard seam.
const PAGE_OVERLAP: usize = 100;

// Row -> pixel conversion deliberately does not happen here. The row height is
// `(font_size * 2.0).max(26.0)`, taller again for rows with line breaks, and is
// only known inside `draw_table`; guessing it at 24px left the last row of a
// 2,000-row page 130 rows below the bottom of the viewport. Every scroll below
// goes through `TableViewState::scroll_to_row` instead.

impl OctaApp {
    /// Move the loaded window of a large tab. `direction` is +1 forward, -1
    /// back. Does nothing when the window cannot move that way.
    pub(crate) fn large_page_step(&mut self, direction: i32) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(key) = tab.large_page_key.clone() else {
            return;
        };
        let Some(handle) = tab.large.as_ref() else {
            return;
        };
        let stride = LARGE_PAGE_ROWS.saturating_sub(PAGE_OVERLAP).max(1);
        // The filtered total is already on the page (`total_rows`), so stepping
        // costs no `count(*)`. Re-querying it here ran once per frame, because
        // the edge triggers below are evaluated every frame.
        let total = tab.table.total_rows.unwrap_or_else(|| handle.row_count());
        let next = if direction >= 0 {
            let candidate = key.offset + stride;
            if candidate >= total.saturating_sub(LARGE_PAGE_ROWS) {
                // Last window: land exactly on the end so the final row is
                // reachable rather than one stride short of it.
                total.saturating_sub(LARGE_PAGE_ROWS)
            } else {
                candidate
            }
        } else {
            key.offset.saturating_sub(stride)
        };
        if next == key.offset {
            return;
        }
        self.large_fetch_page(
            LargePageKey {
                offset: next,
                ..key
            },
            direction < 0,
        );
    }

    /// Re-fetch a large tab's page for `key`, replacing what the view shows.
    ///
    /// ponytail: the fetch is synchronous, on the UI thread. That is fine for
    /// Parquet, where a LIMIT/OFFSET skips row groups, and it is the wrong
    /// answer for a multi-gigabyte CSV, where a deep OFFSET is a scan. Move it
    /// to a worker with a polled slot (like `DriftState`) if that bites.
    ///
    /// `keep_bottom` says the page was reached by scrolling **backwards**.
    /// Either way the viewport lands on the overlap seam rather than on a
    /// scroll edge: parking it at the top or the bottom re-arms the opposite
    /// edge trigger, and the two page steps then ping-pong a DuckDB query per
    /// frame - which is what made the window unusable.
    pub(crate) fn large_fetch_page(&mut self, key: LargePageKey, keep_bottom: bool) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if tab.large_page_key.as_ref() == Some(&key) {
            return;
        }
        let Some(handle) = tab.large.as_ref() else {
            return;
        };
        // The filter's row count changes only when the filter does, so it is
        // carried over unless this fetch changed it. An unfiltered
        // `filtered_count` costs no query, so the common case is free either
        // way; a count that genuinely failed stays `None` rather than being
        // replaced by the whole-file total, which would misreport a filter.
        let total = match tab.large_page_key.as_ref() {
            Some(prev) if prev.filter == key.filter => tab.table.total_rows,
            _ => None,
        }
        .or_else(|| handle.filtered_count(non_empty(&key.filter)).ok());
        let page = handle.page(key.offset, key.len, key.order, non_empty(&key.filter));
        match page {
            Ok(mut t) => {
                // A filtered view reports the filtered total, or the row-number
                // gutter would count against a whole the user cannot see.
                t.total_rows = total;
                let rows = t.row_count();
                tab.table = t;
                tab.table_state.invalidate_row_heights();
                let seam = if keep_bottom {
                    rows.saturating_sub(PAGE_OVERLAP)
                } else {
                    PAGE_OVERLAP.min(rows)
                };
                tab.table_state.scroll_to_row(seam);
                tab.large_page_key = Some(key);
                tab.filter_dirty = true;
            }
            Err(e) => self.status_message = Some((format!("{e:#}"), std::time::Instant::now())),
        }
    }

    /// Put the loaded window over `row` of the **file** and park the viewport
    /// (and the selection) on it. Reached from the virtual scrollbar and from
    /// the jump-to-first/last-row shortcuts, which in this mode mean the file's
    /// first and last row, not the loaded page's.
    ///
    /// The window is centred on `row` so scrolling either way from the landing
    /// point works without an immediate re-fetch, except at the two ends, where
    /// it is clamped and the row sits at the corresponding end of the page.
    pub(crate) fn large_jump_to_row(&mut self, row: usize) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(key) = tab.large_page_key.clone() else {
            return;
        };
        let total = tab.table.total_rows.unwrap_or(0);
        let max_offset = total.saturating_sub(LARGE_PAGE_ROWS);
        let offset = row.saturating_sub(LARGE_PAGE_ROWS / 2).min(max_offset);
        // Re-fetching for a move of a few rows is a query for no visible
        // change - but the viewport still has to move, or a jump within the
        // loaded page would do nothing at all.
        if offset.abs_diff(key.offset) >= PAGE_OVERLAP {
            self.large_fetch_page(LargePageKey { offset, ..key }, false);
        }
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        // Measured against the page actually loaded, which is the one just
        // fetched or the one that was already there.
        let loaded = tab.table.row_offset;
        let within = row
            .saturating_sub(loaded)
            .min(tab.table.row_count().saturating_sub(1));
        tab.table_state.scroll_to_row(within);
        // Move the cursor too, so a keyboard jump lands somewhere visible
        // instead of leaving the selection on the row it started from.
        let col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
        tab.table_state.selected_cell = Some((within, col));
    }

    /// Re-page a large tab after its sort or filter changed.
    pub(crate) fn large_apply_view(&mut self, order: Option<(usize, bool)>, filter: String) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(key) = tab.large_page_key.clone() else {
            return;
        };
        if key.order == order && key.filter == filter {
            return;
        }
        // Any change to what is shown starts again at the top: the old offset
        // pointed into a different result.
        self.large_fetch_page(
            LargePageKey {
                offset: 0,
                len: LARGE_PAGE_ROWS,
                order,
                filter,
            },
            false,
        );
    }
}

fn non_empty(s: &str) -> Option<&str> {
    (!s.trim().is_empty()).then_some(s)
}

/// Turn the search box's text into a SQL fragment matching any column.
///
/// The needle goes into a single-quoted literal with its quotes doubled, and
/// the column names are quoted from the table's own schema, so nothing the
/// user typed is ever interpreted as SQL.
pub(crate) fn search_filter(tab: &TabState, search: &str) -> String {
    let needle = search.trim();
    if needle.is_empty() {
        return String::new();
    }
    let literal = needle.replace('\'', "''");
    let clauses: Vec<String> = tab
        .table
        .columns
        .iter()
        .map(|c| {
            let name = c.name.replace('"', "\"\"");
            format!("CAST(\"{name}\" AS VARCHAR) ILIKE '%{literal}%'")
        })
        .collect();
    if clauses.is_empty() {
        String::new()
    } else {
        format!("({})", clauses.join(" OR "))
    }
}
