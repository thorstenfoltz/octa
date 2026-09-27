//! Searching the whole source when the tab holds only part of it.
//!
//! Octa loads a window of a file (the `initial_load_rows` cap) or a page of a
//! database table, and the search box searches what is in memory. That is the
//! right default - it is instant - but until this existed the result was
//! presented in the same words as a search over a complete table, so a user
//! could conclude a value was absent from a file that contains it.
//!
//! The search bar now states its scope, and offers one button whose behaviour
//! depends on where the rows came from:
//!
//! - a file DuckDB can scan in place (Parquet, CSV/TSV, JSON): the count is a
//!   query against the file and the matches open as their own tab, without
//!   loading the rest of it;
//! - a live database: the count runs on the server, which is the only place
//!   that knows it;
//! - anything else: the load cap is lifted and the file re-read, because
//!   reading it is the only way to see the rest.
//!
//! The scan runs on a worker thread and the update loop drains the slot, the
//! same shape every other long job in the app uses.

use std::sync::{Arc, Mutex};

use octa::data::DataTable;
use octa::ui::toolbar::FullScanKind;

use crate::app::state::{OctaApp, TabState};

/// What a finished scan found.
pub(crate) struct FullScanDone {
    /// Rows in the whole source that match.
    pub(crate) matches: usize,
    /// Rows in the whole source.
    pub(crate) total: usize,
    /// The matching rows, capped like any other load. `None` when the source
    /// could count but not hand them over.
    pub(crate) table: Option<DataTable>,
}

/// One in-flight whole-source search.
pub(crate) struct FullScanJob {
    /// What was searched for, so the result tab can be named after it even if
    /// the search box has moved on. The result is a new tab, so the tab that
    /// asked does not have to still be the active one when it lands.
    pub(crate) needle: String,
    pub(crate) result: Arc<Mutex<Option<Result<FullScanDone, String>>>>,
}

/// Can this search be pushed down to a live database?
///
/// The in-memory search reduces every mode to one regex, and a regex means
/// something different (or nothing at all) on each of the twelve engines -
/// SQL Server has no regex operator at all. A plain substring search is the
/// one shape every engine can express exactly, so anything else is refused
/// rather than answered with a count for a different question.
fn server_can_answer(tab: &TabState) -> bool {
    matches!(tab.search_mode, octa::data::SearchMode::Plain) && !tab.search_whole_word
}

/// The columns a search covers: the whole table, or the one the scope
/// dropdown picked.
fn searched_columns(tab: &TabState) -> Vec<String> {
    match tab.search_scope_col {
        Some(i) => tab
            .table
            .columns
            .get(i)
            .map(|c| vec![c.name.clone()])
            .unwrap_or_default(),
        None => tab.table.columns.iter().map(|c| c.name.clone()).collect(),
    }
}

/// What reaching the rest of `tab`'s source would cost.
pub(crate) fn full_scan_kind(tab: &TabState) -> FullScanKind {
    if tab.db_origin.is_some() {
        return if server_can_answer(tab) {
            FullScanKind::Server
        } else {
            FullScanKind::Unsupported
        };
    }
    let scannable = tab
        .table
        .source_path
        .as_ref()
        .map(std::path::PathBuf::from)
        .is_some_and(|p| octa::formats::large::ScanKind::for_path(&p).is_some());
    if scannable {
        FullScanKind::ScanFile
    } else {
        FullScanKind::LoadAll
    }
}

impl OctaApp {
    /// Start a whole-source search for the active tab's search text.
    pub(crate) fn start_full_scan_search(&mut self, ctx: &eframe::egui::Context) {
        if self.full_scan_job.is_some() {
            return; // one at a time; the spinner is already up
        }
        let tab_idx = self.active_tab;
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        let needle = tab.search_text.trim().to_string();
        if needle.is_empty() {
            return;
        }
        match full_scan_kind(tab) {
            FullScanKind::ScanFile => self.spawn_file_scan(ctx, tab_idx, needle),
            FullScanKind::Server => self.spawn_server_scan(ctx, tab_idx, needle),
            FullScanKind::LoadAll => self.reload_without_row_cap(),
            // The button is disabled for this, so a click cannot arrive.
            FullScanKind::Unsupported => {}
        }
    }

    /// Count and fetch matches straight out of the file, without loading it.
    fn spawn_file_scan(&mut self, ctx: &eframe::egui::Context, tab_idx: usize, needle: String) {
        let tab = &self.tabs[tab_idx];
        let Some(path) = tab.table.source_path.clone().map(std::path::PathBuf::from) else {
            return;
        };
        // The same builder large-file mode uses, so the count answers the
        // question the search box actually asked: same mode, same `Aa`, same
        // whole-word, same column scope.
        let filter = crate::app::large_file::search_filter(tab, &needle);
        if filter.is_empty() {
            return;
        }
        let cap = octa::formats::initial_load_rows();
        let slot: Arc<Mutex<Option<Result<FullScanDone, String>>>> = Arc::new(Mutex::new(None));
        self.full_scan_job = Some(FullScanJob {
            needle,
            result: Arc::clone(&slot),
        });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> Result<FullScanDone, String> {
                let handle = octa::formats::large::open(&path).map_err(|e| format!("{e:#}"))?;
                let total = handle.row_count();
                let matches = handle
                    .filtered_count(Some(&filter))
                    .map_err(|e| format!("{e:#}"))?;
                let table = if matches == 0 {
                    None
                } else {
                    Some(
                        handle
                            .page(0, cap.min(matches), None, Some(&filter))
                            .map_err(|e| format!("{e:#}"))?,
                    )
                };
                Ok(FullScanDone {
                    matches,
                    total,
                    table,
                })
            })();
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Count and fetch matches on the server, which is the only place that
    /// knows what the table holds.
    fn spawn_server_scan(&mut self, ctx: &eframe::egui::Context, tab_idx: usize, needle: String) {
        let tab = &self.tabs[tab_idx];
        let Some(origin) = tab.db_origin.clone() else {
            return;
        };
        let Some(conn) = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == origin.conn_id)
            .cloned()
        else {
            self.status_message = Some((
                octa::i18n::t("sql.server_conn_gone"),
                std::time::Instant::now(),
            ));
            return;
        };
        let columns = searched_columns(tab);
        let case_sensitive = tab.search_case_sensitive;
        let Some(predicate) = conn
            .engine
            .contains_predicate(&columns, &needle, case_sensitive)
        else {
            return;
        };
        let qualified = match &origin.catalog {
            Some(cat) => format!(
                "{}.{}.{}",
                conn.engine.quote_ident(cat),
                conn.engine.quote_ident(&origin.schema),
                conn.engine.quote_ident(&origin.table)
            ),
            None => format!(
                "{}.{}",
                conn.engine.quote_ident(&origin.schema),
                conn.engine.quote_ident(&origin.table)
            ),
        };
        let cap = octa::formats::initial_load_rows();
        let slot: Arc<Mutex<Option<Result<FullScanDone, String>>>> = Arc::new(Mutex::new(None));
        self.full_scan_job = Some(FullScanJob {
            needle,
            result: Arc::clone(&slot),
        });
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> Result<FullScanDone, String> {
                let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                let ssh_secret =
                    octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                let (shared, _) = cache
                    .get_or_connect(&conn, secret.as_deref(), ssh_secret.as_deref())
                    .map_err(|e| format!("{e:#}"))?;
                let mut c = crate::app::db_conn_cache::lock_connector(&shared);
                let counts = c
                    .query(&format!(
                        "SELECT count(*) AS matches FROM {qualified} WHERE {predicate}"
                    ))
                    .map_err(|e| format!("{e:#}"))?;
                let matches = counts
                    .get(0, 0)
                    .map(|v| v.to_string())
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|n| n.max(0.0) as usize)
                    .unwrap_or(0);
                let totals = c
                    .query(&format!("SELECT count(*) AS total FROM {qualified}"))
                    .map_err(|e| format!("{e:#}"))?;
                let total = totals
                    .get(0, 0)
                    .map(|v| v.to_string())
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|n| n.max(0.0) as usize)
                    .unwrap_or(matches);
                let table = if matches == 0 {
                    None
                } else {
                    let sql = octa::db::paged_sql(
                        conn.engine,
                        &format!("SELECT * FROM {qualified} WHERE {predicate}"),
                        cap.min(matches),
                        0,
                    );
                    Some(c.query(&sql).map_err(|e| format!("{e:#}"))?)
                };
                Ok(FullScanDone {
                    matches,
                    total,
                    table,
                })
            })();
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Re-read the active file with the row cap lifted.
    ///
    /// The only honest option for a format DuckDB cannot scan where it lies:
    /// the rest of the file can be reached, it just costs what reading it
    /// costs. The cap is process-wide and the read happens on a worker, so it
    /// is restored once that load lands rather than by a scope guard.
    pub(crate) fn reload_without_row_cap(&mut self) {
        if self.tabs[self.active_tab].table.source_path.is_none() {
            return;
        }
        self.load_cap_to_restore = Some(octa::formats::initial_load_rows());
        octa::formats::set_initial_load_rows(usize::MAX);
        self.status_message = Some((
            octa::i18n::t("search.scan_loading_all"),
            std::time::Instant::now(),
        ));
        self.reload_active_file();
        // A file under the background-load threshold is already read by the
        // time `load_file` returns, so there is no pending load to wait for.
        if self.pending_load.is_none() {
            self.restore_load_cap();
        }
    }

    /// Put the row cap back after a **Load all rows** reload. Lifting it is a
    /// one-file decision, not a session-wide one.
    pub(crate) fn restore_load_cap(&mut self) {
        if let Some(cap) = self.load_cap_to_restore.take() {
            octa::formats::set_initial_load_rows(cap);
        }
    }

    /// Poll the in-flight scan. Called once per frame from the update loop.
    pub(crate) fn drain_full_scan(&mut self) {
        let Some(job) = &self.full_scan_job else {
            return;
        };
        let Some(outcome) = job.result.lock().ok().and_then(|mut g| g.take()) else {
            return;
        };
        let needle = job.needle.clone();
        self.full_scan_job = None;

        let done = match outcome {
            Ok(done) => done,
            Err(e) => {
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("search.scan_failed")),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        self.status_message = Some((
            octa::i18n::t("search.scan_result")
                .replace(
                    "{matches}",
                    &octa::ui::status_bar::format_number(done.matches),
                )
                .replace("{total}", &octa::ui::status_bar::format_number(done.total))
                .replace("{needle}", &needle),
            std::time::Instant::now(),
        ));
        if let Some(mut table) = done.table {
            // A result tab, not a replacement: the original tab keeps its
            // place, its edits and its scroll position.
            table.source_path = None;
            table.total_rows = (table.row_count() < done.matches).then_some(done.matches);
            let mut new_tab = TabState::new(self.settings.default_search_mode);
            new_tab.custom_tab_label =
                Some(octa::i18n::t("search.scan_tab_label").replace("{needle}", &needle));
            new_tab.table = table;
            new_tab.filter_dirty = true;
            self.push_result_tab(new_tab);
        }
    }
}
