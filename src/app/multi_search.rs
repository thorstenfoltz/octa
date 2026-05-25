//! Cross-tab and directory grep. Drives the search panel that lives
//! between the SQL panel and the central table panel — the actual
//! cell-walking lives in `octa::data::multi_search` so the algorithm
//! stays unit-testable.
//!
//! Scopes:
//!
//! * **All Open Tabs** — runs synchronously on the UI thread. Every loaded
//!   tab is a few hundred MB of data at most, and a `RowMatcher` over the
//!   table is faster than the cost of cloning the data into a worker.
//! * **Directory** — runs on a background thread. The thread walks one
//!   directory level, opens each readable file via the format registry,
//!   feeds cells to the matcher, and streams hits back through an
//!   `Arc<Mutex<…>>` so the UI can paint them as they arrive.
//!
//! The active-tab case keeps using the existing per-tab search bar — see
//! `OctaApp::recompute_filter` and the `FocusSearch` shortcut.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use eframe::egui;

use octa::data::multi_search::{self, MultiSearchHit, MultiSearchScope};
use octa::data::{self, SearchMode};
use octa::formats::FormatRegistry;
use octa::ui;

use super::state::OctaApp;

const SNIPPET_CHARS: usize = 80;
const MAX_HITS_PER_FILE: usize = 1_000;
const MAX_HITS_TOTAL: usize = 10_000;

/// Why a file was skipped during a directory scan. The panel surfaces
/// these as a non-fatal warning chip so the user sees which files
/// weren't searched without the scan looking like it failed outright.
#[derive(Debug, Clone)]
pub(crate) enum SkipReason {
    /// File size exceeded `AppSettings.grep_max_file_size_mb`.
    Oversized { size_bytes: u64, cap_bytes: u64 },
    /// The format reader returned an error. Stores the message for the
    /// expandable details view.
    ParseError(String),
}

#[derive(Debug, Clone)]
pub(crate) struct SkippedFile {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// Per-app state for the multi-search panel.
pub(crate) struct MultiSearchState {
    pub(crate) visible: bool,
    pub(crate) query: String,
    pub(crate) mode: SearchMode,
    pub(crate) scope: MultiSearchScope,
    pub(crate) directory: Option<PathBuf>,
    /// Live hits — written by either the UI thread (AllOpenTabs scope) or
    /// the background worker (Directory scope), read every frame by the
    /// panel renderer. Capped at `MAX_HITS_TOTAL` so a runaway regex on a
    /// huge dataset doesn't pin the UI thread shoving labels.
    pub(crate) results: Arc<Mutex<Vec<MultiSearchHit>>>,
    /// Set while a background scan is in flight. Used to render the
    /// "Cancel" button vs the "Search" button.
    pub(crate) running: Arc<AtomicBool>,
    /// Set by the Cancel button. The worker polls this between files and
    /// bails out at the next safe point.
    pub(crate) cancel: Arc<AtomicBool>,
    /// Counter the directory worker updates after each file. Drives the
    /// "Scanned X / Y files" progress label.
    pub(crate) scanned: Arc<AtomicUsize>,
    pub(crate) total: Arc<AtomicUsize>,
    /// Truly fatal worker error (invalid regex, unreadable directory).
    /// Rendered as a red banner above the result list and aborts the
    /// scan. Per-file parse failures go into [`Self::skipped`] instead
    /// so they don't visually overwhelm the (still useful) result set.
    pub(crate) last_error: Arc<Mutex<Option<String>>>,
    /// Files the directory worker couldn't search but kept going past.
    /// Surfaced as an expandable warning section in the panel so the
    /// user sees *which* files were skipped without each one looking
    /// like a fatal error.
    pub(crate) skipped: Arc<Mutex<Vec<SkippedFile>>>,
    /// Handle on the directory worker so the app can join it cleanly when
    /// the user kicks off a second search before the first finishes.
    pub(crate) handle: Option<JoinHandle<()>>,
    /// One-frame focus request so opening the panel via shortcut /
    /// toolbar puts the cursor in the query field.
    pub(crate) focus_query: bool,
    /// True once a scan has been kicked off this session. Lets the panel
    /// show "Type a query and press Search" the first time it opens but
    /// keep showing the (possibly empty) result set after the user
    /// finishes a scan.
    pub(crate) scan_completed: bool,
}

impl MultiSearchState {
    pub(crate) fn new(default_mode: SearchMode) -> Self {
        Self {
            visible: false,
            query: String::new(),
            mode: default_mode,
            scope: MultiSearchScope::default(),
            directory: None,
            results: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            scanned: Arc::new(AtomicUsize::new(0)),
            total: Arc::new(AtomicUsize::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            skipped: Arc::new(Mutex::new(Vec::new())),
            handle: None,
            focus_query: false,
            scan_completed: false,
        }
    }

    fn reset_run_state(&self) {
        self.results.lock().map(|mut r| r.clear()).ok();
        self.skipped.lock().map(|mut s| s.clear()).ok();
        if let Ok(mut e) = self.last_error.lock() {
            *e = None;
        }
        self.scanned.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
    }
}

impl OctaApp {
    /// Toggle the multi-search panel and arm the focus-on-open flag.
    pub(crate) fn toggle_multi_search(&mut self) {
        if self.multi_search.visible {
            // Cancel any in-flight scan as we hide the panel — running it
            // unattended while users can't see results just chews CPU.
            self.cancel_multi_search();
            self.multi_search.visible = false;
        } else {
            self.multi_search.visible = true;
            self.multi_search.focus_query = true;
        }
    }

    /// Cancel and join any running directory-scope worker. Safe to call
    /// when nothing is running.
    pub(crate) fn cancel_multi_search(&mut self) {
        self.multi_search.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.multi_search.handle.take() {
            // Don't block forever — the worker checks `cancel` between
            // files and should exit promptly.
            let _ = handle.join();
        }
        self.multi_search.running.store(false, Ordering::Relaxed);
    }

    /// Kick off a search using the current panel state. AllOpenTabs runs
    /// synchronously; Directory spawns a worker.
    pub(crate) fn run_multi_search(&mut self) {
        // Always cancel any prior run before starting a new one.
        self.cancel_multi_search();
        self.multi_search.reset_run_state();
        if self.multi_search.query.is_empty() {
            return;
        }
        let matcher_proto =
            data::search::RowMatcher::new(&self.multi_search.query, self.multi_search.mode);
        if matches!(matcher_proto, data::search::RowMatcher::Invalid) {
            if let Ok(mut e) = self.multi_search.last_error.lock() {
                *e = Some("Invalid pattern (regex did not compile)".to_string());
            }
            self.multi_search.scan_completed = true;
            return;
        }

        match self.multi_search.scope {
            MultiSearchScope::AllOpenTabs => {
                let matcher = matcher_proto;
                let mut all = Vec::new();
                for (idx, tab) in self.tabs.iter().enumerate() {
                    if tab.table.col_count() == 0 {
                        continue;
                    }
                    let label = tab.title_display();
                    let source_path = tab.table.source_path.as_ref().map(PathBuf::from);
                    let mut hits = multi_search::search_table(
                        &tab.table,
                        &matcher,
                        &label,
                        source_path,
                        Some(idx),
                        SNIPPET_CHARS,
                    );
                    if hits.len() > MAX_HITS_PER_FILE {
                        hits.truncate(MAX_HITS_PER_FILE);
                    }
                    all.extend(hits);
                    if all.len() >= MAX_HITS_TOTAL {
                        all.truncate(MAX_HITS_TOTAL);
                        break;
                    }
                }
                if let Ok(mut r) = self.multi_search.results.lock() {
                    *r = all;
                }
                self.multi_search.scan_completed = true;
            }
            MultiSearchScope::Directory => {
                let Some(dir) = self.multi_search.directory.clone() else {
                    if let Ok(mut e) = self.multi_search.last_error.lock() {
                        *e = Some("Pick a directory first".to_string());
                    }
                    self.multi_search.scan_completed = true;
                    return;
                };
                let files = match collect_directory_files(&dir) {
                    Ok(f) => f,
                    Err(e) => {
                        if let Ok(mut slot) = self.multi_search.last_error.lock() {
                            *slot = Some(format!("Could not read directory: {e}"));
                        }
                        self.multi_search.scan_completed = true;
                        return;
                    }
                };
                self.multi_search
                    .total
                    .store(files.len(), Ordering::Relaxed);
                self.multi_search.running.store(true, Ordering::Relaxed);

                let results = self.multi_search.results.clone();
                let running = self.multi_search.running.clone();
                let cancel = self.multi_search.cancel.clone();
                let scanned = self.multi_search.scanned.clone();
                let last_error = self.multi_search.last_error.clone();
                let skipped = self.multi_search.skipped.clone();
                let query = self.multi_search.query.clone();
                let mode = self.multi_search.mode;
                let max_file_bytes =
                    (self.settings.grep_max_file_size_mb as u64).saturating_mul(1024 * 1024);

                let handle = std::thread::spawn(move || {
                    directory_worker(
                        files,
                        query,
                        mode,
                        max_file_bytes,
                        results,
                        scanned,
                        cancel,
                        last_error,
                        skipped,
                    );
                    running.store(false, Ordering::Relaxed);
                });
                self.multi_search.handle = Some(handle);
                self.multi_search.scan_completed = true;
            }
        }
    }

    /// Drain the worker handle once it's reported `running = false`. Keeps
    /// `handle` from holding a dead `JoinHandle` across frames.
    pub(crate) fn drain_finished_multi_search(&mut self) {
        if self.multi_search.handle.is_some()
            && !self.multi_search.running.load(Ordering::Relaxed)
            && let Some(h) = self.multi_search.handle.take()
        {
            let _ = h.join();
        }
    }

    /// Switch to the tab + cell pointed to by a result. For directory-scope
    /// hits, opens the file (or switches to the already-open tab) before
    /// jumping. Cancels any in-flight scan first so the panel doesn't keep
    /// fighting for the result list.
    pub(crate) fn jump_to_multi_search_hit(&mut self, hit: MultiSearchHit) {
        if let Some(tab_idx) = hit.tab_idx
            && tab_idx < self.tabs.len()
        {
            self.active_tab = tab_idx;
            self.focus_cell_in_active_tab(hit.row, hit.col);
            return;
        }
        if let Some(path) = hit.source_path.as_ref() {
            if let Some(existing) = self.find_tab_by_path(path) {
                self.active_tab = existing;
                self.focus_cell_in_active_tab(hit.row, hit.col);
                return;
            }
            // Load the file. `load_file` opens a new tab and makes it
            // active; we can then place the cursor on the hit cell.
            self.load_file(path.clone());
            self.focus_cell_in_active_tab(hit.row, hit.col);
        }
    }

    fn focus_cell_in_active_tab(&mut self, row: usize, col: usize) {
        let row_height =
            (self.settings.font_size * self.zoom_percent as f32 / 100.0 * 2.0).max(26.0);
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if row >= tab.table.row_count() || col >= tab.table.col_count() {
            return;
        }
        tab.view_mode = data::ViewMode::Table;
        tab.table_state.selected_cell = Some((row, col));
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cols.clear();
        tab.table_state.set_scroll_y(row as f32 * row_height);
        if col < tab.table_state.col_widths.len() {
            let col_left: f32 = tab.table_state.col_widths[..col].iter().sum();
            tab.table_state.set_scroll_x(col_left);
        }
    }

    fn find_tab_by_path(&self, path: &Path) -> Option<usize> {
        let target = path.to_string_lossy();
        self.tabs.iter().position(|t| {
            t.table
                .source_path
                .as_deref()
                .map(|p| p == target)
                .unwrap_or(false)
        })
    }

    /// Render the docked multi-search panel. Returns early when not
    /// visible so calling it every frame is cheap.
    pub(crate) fn render_multi_search_panel(&mut self, parent_ui: &mut egui::Ui) {
        if !self.multi_search.visible {
            return;
        }
        // Pop the worker handle if the scan finished since last frame.
        self.drain_finished_multi_search();

        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        egui::Panel::bottom("multi_search_panel")
            .resizable(true)
            .default_size(220.0)
            .min_size(140.0)
            .show_inside(parent_ui, |ui| {
                let mut run_clicked = false;
                let mut cancel_clicked = false;
                let mut close_clicked = false;
                let mut pick_dir_clicked = false;
                let running = self.multi_search.running.load(Ordering::Relaxed);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Multi-search")
                            .strong()
                            .color(colors.text_primary),
                    );
                    ui.separator();
                    ui.label("Scope:");
                    let scope = &mut self.multi_search.scope;
                    egui::ComboBox::from_id_salt("multi_search_scope")
                        .selected_text(scope.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                scope,
                                MultiSearchScope::AllOpenTabs,
                                MultiSearchScope::AllOpenTabs.label(),
                            );
                            ui.selectable_value(
                                scope,
                                MultiSearchScope::Directory,
                                MultiSearchScope::Directory.label(),
                            );
                        });
                    ui.label("Mode:");
                    let mode = &mut self.multi_search.mode;
                    egui::ComboBox::from_id_salt("multi_search_mode")
                        .selected_text(mode.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(mode, SearchMode::Plain, "Plain");
                            ui.selectable_value(mode, SearchMode::Wildcard, "Wildcard");
                            ui.selectable_value(mode, SearchMode::Regex, "Regex");
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("×")
                            .on_hover_text("Close multi-search panel")
                            .clicked()
                        {
                            close_clicked = true;
                        }
                    });
                });

                ui.horizontal(|ui| {
                    let query_edit = egui::TextEdit::singleline(&mut self.multi_search.query)
                        .desired_width(ui.available_width() - 220.0)
                        .hint_text("Search across all open tabs or a directory…");
                    let resp = ui.add(query_edit);
                    if std::mem::take(&mut self.multi_search.focus_query) {
                        resp.request_focus();
                    }
                    if resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !running
                    {
                        run_clicked = true;
                    }
                    if running {
                        if ui.button("Cancel").clicked() {
                            cancel_clicked = true;
                        }
                    } else if ui.button("Search").clicked() {
                        run_clicked = true;
                    }
                });

                if self.multi_search.scope == MultiSearchScope::Directory {
                    ui.horizontal(|ui| {
                        ui.label("Directory:");
                        let label = self
                            .multi_search
                            .directory
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "(none picked)".to_string());
                        ui.label(egui::RichText::new(label).color(colors.text_secondary));
                        if ui.button("Pick directory…").clicked() {
                            pick_dir_clicked = true;
                        }
                    });
                }

                if let Ok(err) = self.multi_search.last_error.lock()
                    && let Some(msg) = err.as_ref()
                {
                    ui.colored_label(colors.error, msg);
                }

                let scanned = self.multi_search.scanned.load(Ordering::Relaxed);
                let total = self.multi_search.total.load(Ordering::Relaxed);
                let hit_count = self
                    .multi_search
                    .results
                    .lock()
                    .map(|r| r.len())
                    .unwrap_or(0);
                ui.horizontal(|ui| {
                    if running && self.multi_search.scope == MultiSearchScope::Directory {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.label(format!(
                            "Scanning {} / {} files — {} hit(s)",
                            scanned, total, hit_count
                        ));
                    } else if self.multi_search.scan_completed {
                        ui.label(format!("{} hit(s)", hit_count));
                        if hit_count >= MAX_HITS_TOTAL {
                            ui.colored_label(
                                colors.warning,
                                format!("(capped at {})", MAX_HITS_TOTAL),
                            );
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Type a query and press Search.")
                                .color(colors.text_secondary),
                        );
                    }
                });

                self.render_skipped_files(ui);

                ui.separator();
                self.render_multi_search_results(ui);

                if pick_dir_clicked && let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    self.multi_search.directory = Some(picked);
                }
                if cancel_clicked {
                    self.cancel_multi_search();
                }
                if close_clicked {
                    self.cancel_multi_search();
                    self.multi_search.visible = false;
                }
                if run_clicked {
                    self.run_multi_search();
                }
            });
    }

    /// Render the "files skipped during the scan" warning chip. The
    /// chip itself is one line; clicking expands a scroll area with one
    /// row per skipped file (filename + reason). Warning color so the
    /// scan doesn't visually read as failed when results are present.
    fn render_skipped_files(&mut self, ui: &mut egui::Ui) {
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        // Snapshot to drop the lock before any nested egui calls.
        let skipped: Vec<SkippedFile> = self
            .multi_search
            .skipped
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        if skipped.is_empty() {
            return;
        }
        let id = ui.make_persistent_id("multi_search_skipped_files");
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                ui.colored_label(
                    colors.warning,
                    format!("{} file(s) skipped — click to expand", skipped.len()),
                );
            })
            .body(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .auto_shrink([false, true])
                    .id_salt("multi_search_skipped_scroll")
                    .show(ui, |ui| {
                        for sk in &skipped {
                            let name = sk
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| sk.path.to_string_lossy().to_string());
                            let detail = match &sk.reason {
                                SkipReason::Oversized {
                                    size_bytes,
                                    cap_bytes,
                                } => format!(
                                    "{} — {} MB exceeds {} MB cap",
                                    name,
                                    size_bytes / (1024 * 1024),
                                    cap_bytes / (1024 * 1024),
                                ),
                                SkipReason::ParseError(msg) => {
                                    // Truncate verbose reader errors so one
                                    // line stays readable; the user can
                                    // hover for the full message.
                                    let trimmed = msg.lines().next().unwrap_or(msg);
                                    let short = if trimmed.chars().count() > 120 {
                                        let mut s: String = trimmed.chars().take(120).collect();
                                        s.push('…');
                                        s
                                    } else {
                                        trimmed.to_string()
                                    };
                                    format!("{} — {}", name, short)
                                }
                            };
                            ui.label(detail)
                                .on_hover_text(sk.path.to_string_lossy().to_string());
                        }
                    });
            });
    }

    fn render_multi_search_results(&mut self, ui: &mut egui::Ui) {
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        // Snapshot hits so we drop the lock before any UI interaction
        // re-enters borrowing the app. Cheap clone — hits are small structs.
        let hits: Vec<MultiSearchHit> = self
            .multi_search
            .results
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default();

        let mut clicked: Option<MultiSearchHit> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for hit in &hits {
                    let label = format!(
                        "{} · row {} · {} — {}",
                        hit.source_label,
                        hit.row + 1,
                        hit.column_name,
                        hit.snippet,
                    );
                    let resp = ui.add(
                        egui::Label::new(egui::RichText::new(label).color(colors.text_primary))
                            .sense(egui::Sense::click()),
                    );
                    if resp.clicked() {
                        clicked = Some(hit.clone());
                    }
                    resp.on_hover_text(format!(
                        "Jump to row {}, column {}",
                        hit.row + 1,
                        hit.col + 1
                    ));
                }
            });
        if let Some(hit) = clicked {
            self.jump_to_multi_search_hit(hit);
        }
    }
}

/// One directory level, sorted (directories first, files next). Hidden
/// entries (leading `.`) and entries that aren't files are skipped at
/// scan time, not here, so the count we surface in the panel matches
/// what we actually try to read.
fn collect_directory_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = ui::directory_tree::read_sorted_dir(dir)?;
    Ok(entries
        .into_iter()
        .filter(|p| p.is_file())
        .filter(|p| {
            // Hidden files: skip. They're rarely the target of a grep
            // and frequently editor swap files.
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| !s.starts_with('.'))
                .unwrap_or(true)
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn directory_worker(
    files: Vec<PathBuf>,
    query: String,
    mode: SearchMode,
    max_file_bytes: u64,
    results: Arc<Mutex<Vec<MultiSearchHit>>>,
    scanned: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    skipped: Arc<Mutex<Vec<SkippedFile>>>,
) {
    let matcher = data::search::RowMatcher::new(&query, mode);
    if matches!(matcher, data::search::RowMatcher::Invalid) {
        if let Ok(mut e) = last_error.lock() {
            *e = Some("Invalid pattern (regex did not compile)".to_string());
        }
        return;
    }
    let registry = FormatRegistry::new();
    for path in files {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        scanned.fetch_add(1, Ordering::Relaxed);

        // Per-file size cap (matching the `grep_max_file_size_mb`
        // setting). The user sees the file name in the skipped list
        // so they can either bump the cap or pick a smaller scope.
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if max_file_bytes > 0 && size > max_file_bytes {
            push_skip(
                &skipped,
                &path,
                SkipReason::Oversized {
                    size_bytes: size,
                    cap_bytes: max_file_bytes,
                },
            );
            continue;
        }

        let Some(reader) = registry.reader_for_path(&path) else {
            push_skip(
                &skipped,
                &path,
                SkipReason::ParseError("no reader for this file".to_string()),
            );
            continue;
        };
        let table = match reader.read_file(&path) {
            Ok(t) => t,
            Err(e) => {
                push_skip(&skipped, &path, SkipReason::ParseError(e.to_string()));
                continue;
            }
        };
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let mut hits = multi_search::search_table(
            &table,
            &matcher,
            &label,
            Some(path.clone()),
            None,
            SNIPPET_CHARS,
        );
        if hits.is_empty() {
            continue;
        }
        if hits.len() > MAX_HITS_PER_FILE {
            hits.truncate(MAX_HITS_PER_FILE);
        }
        let mut stop = false;
        if let Ok(mut all) = results.lock() {
            let room = MAX_HITS_TOTAL.saturating_sub(all.len());
            if hits.len() > room {
                hits.truncate(room);
                stop = true;
            }
            all.extend(hits);
            if all.len() >= MAX_HITS_TOTAL {
                stop = true;
            }
        }
        if stop {
            break;
        }
    }
}

fn push_skip(skipped: &Arc<Mutex<Vec<SkippedFile>>>, path: &Path, reason: SkipReason) {
    if let Ok(mut s) = skipped.lock() {
        s.push(SkippedFile {
            path: path.to_path_buf(),
            reason,
        });
    }
}
