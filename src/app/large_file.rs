//! Opening a file in large-file mode: the size check, the two open paths and
//! the conversion worker.
//!
//! The decision of *whether* to ask lives here; the question itself is
//! `dialogs::large_file_notice`. Both paths end in `finish_large_open`, which
//! opens the scan on a worker; `poll_large_open` is the only place a
//! `LargeTable` is put onto a tab. Every later page comes from a worker too
//! (`LargeFetch`), so no query over the file ever holds up the window.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use eframe::egui;
use octa::data::DataTable;
use octa::formats::large::{self, LargeTable, ScanKind};
use octa::i18n::t;

use super::pushdown::{ServerTask, spawn_local_fallible};

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
        // A protected zip's decrypted copy in a format DuckDB cannot scan
        // opens normally. Conversion would write it as a plain Parquet into
        // the shared temp folder and keep it there, and saves nothing: it
        // holds the whole table in memory too, and the entry already was.
        if ScanKind::for_path(path).is_none()
            && super::dialogs::passphrase::is_protected_copy(
                &self.pending_protected_origins,
                &self.tabs,
                path,
            )
        {
            return false;
        }
        let Some(size_bytes) = large_size(path, self.settings.large_file_min_bytes) else {
            return false;
        };
        if self.settings.show_large_file_notice {
            self.pending_large_file_notice = Some(LargeFileNotice {
                path: path.to_path_buf(),
                size_bytes,
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
        let task = self
            .long_tasks
            .start(octa::i18n::t("notify.task_large_convert"));
        std::thread::spawn(move || {
            let _task = task;
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
        // Opening counts every row of the file, and the first page follows:
        // seconds on a big CSV, so both run on a worker.
        let path = scan_path.clone();
        let task = spawn_local_fallible(move || {
            let handle = large::open(&path)?;
            // The first page fills the view; scrolling asks for the rest.
            let first = handle.page(0, LARGE_PAGE_ROWS, None, None)?;
            Ok((handle, first))
        });
        self.large_open_job = Some(LargeOpenJob {
            name: original_name
                .clone()
                .unwrap_or_else(|| display_name(&scan_path)),
            scan_path,
            original_name,
            task,
        });
    }

    /// Once per frame: put a finished large-file open onto a tab.
    pub(crate) fn poll_large_open(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.large_open_job else {
            return;
        };
        let Some(outcome) = job.task.poll_result() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        };
        let Some(job) = self.large_open_job.take() else {
            return;
        };
        match outcome {
            Ok((handle, first)) => {
                self.put_large_on_tab(job.scan_path, job.original_name, handle, first)
            }
            Err(e) => self.status_message = Some((e, std::time::Instant::now())),
        }
    }

    fn put_large_on_tab(
        &mut self,
        scan_path: PathBuf,
        original_name: Option<String>,
        handle: LargeTable,
        first: DataTable,
    ) {
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
        // A decrypted copy of a protected zip: closing the tab removes it.
        self.claim_protected_origin(self.active_tab, &scan_path);
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        tab.table = first;
        tab.table.source_path = Some(scan_path.to_string_lossy().to_string());
        tab.file_stamp = crate::app::file_io::file_stamp(&scan_path);
        if tab.protected_origin.is_none() {
            tab.custom_tab_label = Some(label);
        }
        tab.parse_error_banner = Some(t("largefile.banner"));
        tab.large_page_key = Some(LargePageKey {
            offset: 0,
            len: LARGE_PAGE_ROWS,
            order: None,
            filter: String::new(),
        });
        tab.large = Some(handle);
        tab.large_fetch = None;
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

/// The size that puts `path` into large-file mode, or `None` below `min_bytes`.
///
/// One setting decides, in bytes. A Parquet file is measured by the larger of
/// its length and the uncompressed data size its footer states, so a file
/// small on disk but enormous when expanded is caught too. Its row count is
/// deliberately not a gate: a normal open loads the first `initial_load_rows`
/// and streams the rest in as you scroll, so many rows alone is no reason to
/// overrule the size the user picked (a 35 MB Parquet saved from a 5 GB CSV
/// was asked about under a 10 GB threshold). The footer read is skipped when
/// the length already decided: cheap, but it runs on every Parquet open.
fn large_size(path: &Path, min_bytes: usize) -> Option<u64> {
    let on_disk = std::fs::metadata(path).ok()?.len();
    let size = if on_disk as usize >= min_bytes {
        on_disk
    } else {
        on_disk.max(stated_data_bytes(path)?)
    };
    (size as usize >= min_bytes).then_some(size)
}

/// The uncompressed data size a format states without reading its data. Only
/// Parquet does; everything else answers `None` and is judged on its length.
fn stated_data_bytes(path: &Path) -> Option<u64> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if ext != "parquet" && ext != "pq" {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let reader = parquet::file::serialized_reader::SerializedFileReader::new(file).ok()?;
    use parquet::file::reader::FileReader;
    let bytes: i64 = reader
        .metadata()
        .row_groups()
        .iter()
        .flat_map(|rg| rg.columns())
        .map(|c| c.uncompressed_size())
        .sum();
    Some(bytes.max(0) as u64)
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
        // One fetch at a time: the edge triggers fire every frame, and a step
        // taken from the page still on screen would replace the fetch that is
        // about to move it.
        if tab.large_fetch.is_some() {
            return;
        }
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
    /// `keep_bottom` says the page was reached by scrolling **backwards**.
    /// Either way the viewport lands on the overlap seam rather than on a
    /// scroll edge: parking it at the top or the bottom re-arms the opposite
    /// edge trigger, and the two page steps then ping-pong a DuckDB query per
    /// frame - which is what made the window unusable.
    pub(crate) fn large_fetch_page(&mut self, key: LargePageKey, keep_bottom: bool) {
        let landing = if keep_bottom {
            Landing::Backward
        } else {
            Landing::Forward
        };
        self.request_large_page(self.active_tab, key, landing);
    }

    /// Ask for `key` on tab `idx`. The fetch runs on a worker; while one is
    /// running, the newest request waits for it and runs next, so dragging
    /// the scrollbar costs one query at a time, never one per frame.
    fn request_large_page(&mut self, idx: usize, key: LargePageKey, landing: Landing) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let shown = tab.large_page_key.clone();
        if let Some((key, landing)) =
            queue_request(tab.large_fetch.as_mut(), shown.as_ref(), key, landing)
        {
            self.spawn_large_fetch(idx, key, landing);
        }
    }

    fn spawn_large_fetch(&mut self, idx: usize, key: LargePageKey, landing: Landing) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let Some(handle) = tab.large.as_ref() else {
            return;
        };
        // The filter's row count changes only when the filter does, so it is
        // carried over unless this fetch changed it. A count that genuinely
        // failed fails the fetch rather than being replaced by the whole-file
        // total, which would misreport a filter.
        let carried = match tab.large_page_key.as_ref() {
            Some(prev) if prev.filter == key.filter => tab.table.total_rows,
            _ => None,
        };
        let job = match handle.page_job(
            key.offset,
            key.len,
            key.order,
            key.filter.clone(),
            carried.is_none(),
        ) {
            Ok(job) => job,
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                return;
            }
        };
        tab.large_fetch = Some(LargeFetch {
            key,
            landing,
            carried,
            task: spawn_local_fallible(job),
            next: None,
        });
    }

    /// Once per frame: land every finished page fetch, then start the request
    /// that waited for it.
    pub(crate) fn poll_large_fetches(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let Some(fetch) = &self.tabs[idx].large_fetch else {
                continue;
            };
            let Some(outcome) = fetch.task.poll_result() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                continue;
            };
            let Some(fetch) = self.tabs[idx].large_fetch.take() else {
                continue;
            };
            match outcome {
                Ok((page, total)) => {
                    self.land_large_page(
                        idx,
                        fetch.key,
                        fetch.landing,
                        page,
                        total.or(fetch.carried),
                    );
                }
                Err(e) => self.status_message = Some((e, std::time::Instant::now())),
            }
            if let Some((key, landing)) = fetch.next {
                if self.tabs[idx].large_page_key.as_ref() == Some(&key) {
                    if let Landing::Row(row) = landing {
                        park_on_row(&mut self.tabs[idx], row);
                    }
                } else {
                    self.spawn_large_fetch(idx, key, landing);
                }
            }
        }
    }

    fn land_large_page(
        &mut self,
        idx: usize,
        key: LargePageKey,
        landing: Landing,
        mut page: DataTable,
        total: Option<usize>,
    ) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        // A filtered view reports the filtered total, or the row-number
        // gutter would count against a whole the user cannot see.
        page.total_rows = total;
        let rows = page.row_count();
        tab.table = page;
        tab.table_state.invalidate_row_heights();
        tab.large_page_key = Some(key);
        tab.filter_dirty = true;
        match landing {
            Landing::Forward => tab.table_state.scroll_to_row(PAGE_OVERLAP.min(rows)),
            Landing::Backward => tab
                .table_state
                .scroll_to_row(rows.saturating_sub(PAGE_OVERLAP)),
            Landing::Row(row) => park_on_row(tab, row),
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
        // loaded page would do nothing at all. A far jump parks the viewport
        // when its page lands.
        if offset.abs_diff(key.offset) >= PAGE_OVERLAP {
            self.request_large_page(
                self.active_tab,
                LargePageKey { offset, ..key },
                Landing::Row(row),
            );
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            park_on_row(tab, row);
        }
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

/// Everything that filters a large tab, as one `WHERE` condition over the
/// file: the search box, every column filter (Column Filter, Filter by
/// value) and every comparison chip (the Ask box), ANDed as an ordinary tab
/// ANDs them. Columns are taken in index
/// order and values sorted, so the same filters always spell the same text,
/// which is what tells the view whether to fetch again. Also returns the
/// names of columns whose filter cannot be run as a query, which are left
/// out rather than applied to the page alone.
pub(crate) fn view_filter(tab: &TabState, search: &str) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let searched = search_filter(tab, search);
    if !searched.is_empty() {
        parts.push(searched);
    }
    let mut refused = Vec::new();
    if let Some(large) = &tab.large {
        let mut cols: Vec<(&usize, &std::collections::HashSet<String>)> =
            tab.column_filters.iter().collect();
        cols.sort_by_key(|(c, _)| **c);
        for (&col, allowed) in cols {
            let mut values: Vec<&str> = allowed.iter().map(String::as_str).collect();
            values.sort_unstable();
            match large.values_filter(col, &values) {
                Some(sql) => parts.push(format!("({sql})")),
                None => refused.push(column_name(tab, col)),
            }
        }
        for p in &tab.predicate_filters {
            match large.predicate_filter(p.col, p.op, &p.value, p.case_sensitive) {
                Some(sql) => parts.push(format!("({sql})")),
                None => refused.push(column_name(tab, p.col)),
            }
        }
    }
    refused.dedup();
    (parts.join(" AND "), refused)
}

fn column_name(tab: &TabState, col: usize) -> String {
    tab.table
        .columns
        .get(col)
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

/// What asking for page `key` does, given the fetch running (if any) and the
/// page `shown`: the request to start now, or `None` when it waits behind the
/// running fetch (the newest waiting request wins) or is already on screen.
fn queue_request(
    running: Option<&mut LargeFetch>,
    shown: Option<&LargePageKey>,
    key: LargePageKey,
    landing: Landing,
) -> Option<(LargePageKey, Landing)> {
    if let Some(fetch) = running {
        if fetch.key == key {
            fetch.landing = landing;
            fetch.next = None;
        } else {
            fetch.next = Some((key, landing));
        }
        return None;
    }
    (shown != Some(&key)).then_some((key, landing))
}

/// Park the viewport, and the selection, on `row` of the **file**, measured
/// against the page the tab holds now.
fn park_on_row(tab: &mut TabState, row: usize) {
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

/// A large-file open running on a worker: the scan and its first page.
pub(crate) struct LargeOpenJob {
    /// What the status bar names while it runs.
    pub(crate) name: String,
    scan_path: PathBuf,
    original_name: Option<String>,
    task: ServerTask<(LargeTable, DataTable)>,
}

/// A page of a large tab being fetched on a worker. One per tab; a request
/// made meanwhile waits in `next` (the newest wins) and runs when it lands.
pub(crate) struct LargeFetch {
    key: LargePageKey,
    landing: Landing,
    /// The filtered total to keep when the fetch does not count again.
    carried: Option<usize>,
    task: ServerTask<(DataTable, Option<usize>)>,
    next: Option<(LargePageKey, Landing)>,
}

/// Where the viewport goes when a fetched page lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// Reached by scrolling forwards: the overlap seam near the top.
    Forward,
    /// Reached by scrolling backwards: the overlap seam near the bottom.
    Backward,
    /// A jump to this row of the file (scrollbar, first or last row).
    Row(usize),
}

/// Turn the search box into a SQL fragment matching the same rows the
/// in-memory search would match: same mode, same `Aa`, same whole-word, same
/// column scope.
///
/// It goes through [`octa::data::search::sql_regex_pattern`] rather than
/// building a `LIKE` of its own, because the in-memory search reduces every
/// mode to one regex. A hand-rolled `ILIKE '%needle%'` answered a *plain
/// substring* question no matter what the user had selected, so a regex or
/// whole-word search quietly matched the wrong rows here while the same
/// search in an ordinary tab matched the right ones.
///
/// The pattern goes into a single-quoted literal with its quotes doubled and
/// the column names are quoted from the table's own schema, so nothing the
/// user typed is ever interpreted as SQL.
///
/// Returns an empty string for an empty search (no filter at all) and the
/// constant-false `"1 = 0"` for a pattern the regex engine refuses, which is
/// what the in-memory search shows for one: nothing.
pub(crate) fn search_filter(tab: &TabState, search: &str) -> String {
    if search.trim().is_empty() {
        return String::new();
    }
    let Some(pattern) = octa::data::search::sql_regex_pattern(
        search,
        tab.search_mode,
        tab.search_case_sensitive,
        tab.search_whole_word,
    ) else {
        return "1 = 0".to_string();
    };
    let literal = pattern.replace('\'', "''");
    let columns: Vec<&str> = match tab.search_scope_col {
        Some(i) => tab
            .table
            .columns
            .get(i)
            .map(|c| vec![c.name.as_str()])
            .unwrap_or_default(),
        None => tab.table.columns.iter().map(|c| c.name.as_str()).collect(),
    };
    let clauses: Vec<String> = columns
        .iter()
        .map(|name| {
            let col = name.replace('"', "\"\"");
            format!("regexp_matches(CAST(\"{col}\" AS VARCHAR), '{literal}')")
        })
        .collect();
    if clauses.is_empty() {
        String::new()
    } else {
        format!("({})", clauses.join(" OR "))
    }
}

#[cfg(test)]
#[path = "large_file_tests.rs"]
mod tests;
