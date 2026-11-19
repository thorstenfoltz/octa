//! Live tabs: Follow file (a growing CSV, TSV, JSON Lines or log file gets
//! its new rows every second) and Auto-refresh (a database or API tab is
//! read again at an interval). Neither is saved in the session.

use std::path::Path;
use std::time::{Duration, Instant};

use octa::data::{CellValue, DataTable, MarkColor, MarkKey};
use octa::db::DbEngine;
use octa::formats::follow::{FollowState, FollowStep, first_line};
use octa::ui::status_bar::LiveState;

use super::state::OctaApp;

/// Bytes read per check. ponytail: a chunk size, not a limit; a burst
/// arrives over several seconds instead of stalling one frame.
const FOLLOW_SLICE: u64 = 8 * 1024 * 1024;

/// How often a followed file is looked at.
const FOLLOW_EVERY: Duration = Duration::from_secs(1);

pub(crate) struct Follow {
    /// `None` = start again once the reload in flight has landed.
    pub(crate) state: Option<FollowState>,
    pub(crate) next_check: Instant,
    /// The delimited header put in front of new lines; `None` for JSON
    /// Lines and logs.
    pub(crate) header: Option<String>,
    /// The tab the user was on when a reload started from another tab; the
    /// reload makes the followed tab active, so following switches back once
    /// the read has landed.
    pub(crate) return_to: Option<usize>,
}

pub(crate) struct AutoRefresh {
    pub(crate) every: Duration,
    pub(crate) next_at: Instant,
    /// The rows before the read in flight, to highlight what changed.
    pub(crate) before: Option<DataTable>,
    /// The tab the user was on when this tab's read started.
    pub(crate) return_to: Option<usize>,
}

pub(crate) fn is_billed(e: DbEngine) -> bool {
    matches!(
        e,
        DbEngine::BigQuery | DbEngine::Athena | DbEngine::Snowflake
    )
}

/// `Ok(secs)` or `Err(minimum)`: the minimums are settings.
pub(crate) fn check_interval(
    secs: u64,
    billed: bool,
    min: u64,
    min_billed: u64,
) -> Result<u64, u64> {
    let floor = if billed { min_billed } else { min };
    if secs < floor { Err(floor) } else { Ok(secs) }
}

/// `new`'s rows in `tab`'s column order, each cell converted to the tab's
/// column type; `None` when the two share no column name (the file changed
/// shape: reload).
pub(crate) fn rows_for(tab: &DataTable, new: &DataTable) -> Option<Vec<Vec<CellValue>>> {
    let map: Vec<Option<usize>> = tab
        .columns
        .iter()
        .map(|c| new.columns.iter().position(|n| n.name == c.name))
        .collect();
    if map.iter().all(Option::is_none) {
        return None;
    }
    Some(
        (0..new.row_count())
            .map(|r| {
                map.iter()
                    .zip(&tab.columns)
                    .map(|(m, col)| match m.and_then(|c| new.get(r, c)) {
                        Some(v) => octa::data::convert_value(v, &col.data_type),
                        None => CellValue::Null,
                    })
                    .collect()
            })
            .collect(),
    )
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
}

fn is_delimited(ext: &str) -> bool {
    matches!(ext, "csv" | "tsv")
}

/// The new lines as rows of `path`'s format: a temp file the registry reader
/// parses exactly as it parses the whole file.
fn parse_appended(
    app: &OctaApp,
    ext: &str,
    header: Option<&str>,
    text: &str,
) -> anyhow::Result<DataTable> {
    let dir = tempfile::TempDir::new()?;
    let tmp = dir.path().join(format!("follow.{ext}"));
    let body = match header {
        Some(h) => format!("{h}\n{text}"),
        None => text.to_string(),
    };
    std::fs::write(&tmp, body)?;
    let reader = app.registry.reader_for_path(&tmp).ok_or_else(|| {
        anyhow::anyhow!(octa::i18n::t("msg.no_reader").replace("{file}", &format!(".{ext}")))
    })?;
    reader.read_file(&tmp)
}

impl OctaApp {
    /// Why Follow file is greyed for tab `idx`, `None` when it can be on.
    pub(crate) fn follow_block_reason(&self, idx: usize) -> Option<String> {
        use octa::i18n::t;
        let tab = self.tabs.get(idx)?;
        let path = tab.table.source_path.as_deref();
        let plain_file = path.is_some_and(|p| Path::new(p).is_file())
            && tab.cloud_origin.is_none()
            && tab.db_origin.is_none()
            && tab.api_origin.is_none()
            && tab.compressed_origin.is_none()
            && tab.protected_origin.is_none();
        let Some(path) = path.filter(|_| plain_file) else {
            return Some(t("live.follow_not_local"));
        };
        if tab.large.is_some() {
            return Some(t("live.follow_large"));
        }
        let ext = extension(path);
        let log = tab
            .table
            .format_name
            .as_deref()
            .is_some_and(|f| f.starts_with("Log"));
        if !(is_delimited(&ext) || matches!(ext.as_str(), "jsonl" | "ndjson" | "log") || log) {
            return Some(t("live.follow_format"));
        }
        if tab.source_has_more() {
            return Some(t("live.follow_partial"));
        }
        None
    }

    /// Why Auto-refresh is greyed for tab `idx`.
    pub(crate) fn auto_refresh_block_reason(&self, idx: usize) -> Option<String> {
        let tab = self.tabs.get(idx)?;
        (tab.db_origin.is_none() && tab.api_origin.is_none())
            .then(|| octa::i18n::t("live.auto_refresh_needs_server"))
    }

    pub(crate) fn toggle_follow(&mut self, idx: usize) {
        if self.tabs.get(idx).is_none() {
            return;
        }
        if self.tabs[idx].follow.is_some() {
            self.tabs[idx].follow = None;
            return;
        }
        if let Some(why) = self.follow_block_reason(idx) {
            self.status_message = Some((why, Instant::now()));
            return;
        }
        let Some(path) = self.tabs[idx].table.source_path.clone() else {
            return;
        };
        match start_follow(&path) {
            Ok(f) => self.tabs[idx].follow = Some(f),
            Err(e) => self.status_message = Some((format!("{e:#}"), Instant::now())),
        }
    }

    /// The Auto-refresh entry: opens the dialog for tab `idx`, or says why
    /// the tab cannot be refreshed.
    pub(crate) fn open_auto_refresh(&mut self, idx: usize) {
        if let Some(why) = self.auto_refresh_block_reason(idx) {
            self.status_message = Some((why, Instant::now()));
            return;
        }
        let min = self.settings.auto_refresh_min_secs;
        self.auto_refresh_dialog = Some(super::dialogs::auto_refresh::AutoRefreshDialog {
            tab: idx,
            buf: self.tabs[idx]
                .auto_refresh
                .as_ref()
                .map_or(min, |a| a.every.as_secs())
                .to_string(),
            size: octa::ui::settings::DialogSize::Normal,
        });
    }

    /// The engine of tab `idx` when it was read from a saved database
    /// connection; `None` for an API tab.
    pub(crate) fn tab_db_engine(&self, idx: usize) -> Option<DbEngine> {
        let id = &self.tabs.get(idx)?.db_origin.as_ref()?.conn_id;
        self.settings
            .db_connections
            .iter()
            .find(|c| &c.id == id)
            .map(|c| c.engine)
    }

    /// The status-bar pill (text, hint, state) of tab `idx`. It is always
    /// there, so whether a tab refreshes by itself never needs a dialog to tell.
    pub(crate) fn live_pill(&self, idx: usize) -> (String, String, LiveState) {
        use octa::i18n::t;
        let tab = &self.tabs[idx];
        if tab.follow.is_some() {
            return (t("live.following"), t("live.following_hint"), LiveState::On);
        }
        if let Some(a) = &tab.auto_refresh {
            let hint = t("live.auto_refresh_pill_hint");
            return if tab.table.is_modified() {
                (t("live.auto_refresh_paused"), hint, LiveState::Paused)
            } else {
                let secs = a.every.as_secs().to_string();
                let text = t("live.auto_refresh_pill").replace("{secs}", &secs);
                (text, hint, LiveState::On)
            };
        }
        match self.auto_refresh_block_reason(idx) {
            Some(why) => (t("live.auto_refresh_off"), why, LiveState::Unavailable),
            None => (
                t("live.auto_refresh_off"),
                t("live.auto_refresh_off_hint"),
                LiveState::Off,
            ),
        }
    }

    /// Once per frame: look at every followed file that is due, and keep the
    /// frame loop ticking while any tab is live.
    pub(crate) fn drive_live_tabs(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let mut soonest: Option<Instant> = None;
        for idx in 0..self.tabs.len() {
            if let Some(next) = self.tabs[idx].follow.as_ref().map(|f| f.next_check)
                && now >= next
            {
                self.follow_step(idx, ctx);
            }
            if let Some(f) = self.tabs.get(idx).and_then(|t| t.follow.as_ref()) {
                soonest = Some(soonest.map_or(f.next_check, |s| s.min(f.next_check)));
            }
            if let Some(at) = self.auto_refresh_step(idx, ctx) {
                soonest = Some(soonest.map_or(at, |s| s.min(at)));
            }
        }
        if let Some(at) = soonest {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
    }

    /// One look at tab `idx`'s Auto-refresh; returns when to look again.
    /// States: idle (waiting for `next_at`) -> read in flight (`reload_target`
    /// names the tab) -> landed (`before` holds the old rows and the tab holds
    /// columns again) -> idle. `next_at` is set when a read starts (so a failed
    /// read waits one interval) and again when it lands (so reads never
    /// overlap and the interval counts from the end of a read).
    fn auto_refresh_step(&mut self, idx: usize, ctx: &egui::Context) -> Option<Instant> {
        let now = Instant::now();
        let a = self.tabs[idx].auto_refresh.as_ref()?;
        let (every, waiting) = (a.every, a.before.is_some());
        let in_flight = self.reload_target.as_ref().is_some_and(|t| t.tab == idx);
        if waiting && !in_flight && !self.tabs[idx].table.columns.is_empty() {
            let keys = match self.tabs[idx]
                .db_origin
                .as_ref()
                .and_then(|o| o.identity.as_ref())
            {
                Some(octa::db::write_back::RowIdentity::Key(cols)) => cols.clone(),
                _ => Vec::new(),
            };
            let (on, secs) = (
                self.settings.sql_row_diff_highlight_enabled,
                self.settings.sql_row_diff_highlight_secs,
            );
            let tab = &mut self.tabs[idx];
            let (before, back) = tab.auto_refresh.as_mut().map(|a| {
                a.next_at = now + every;
                (a.before.take(), a.return_to.take())
            })?;
            if let Some(before) = before {
                super::sql_panel::apply_change_highlight(tab, &before, &keys, on, secs);
            }
            if let Some(b) = back.filter(|&b| b < self.tabs.len())
                && self.active_tab == idx
            {
                self.active_tab = b;
            }
            return Some(now + every);
        }
        if waiting || in_flight {
            return Some(now + Duration::from_millis(250));
        }
        let next_at = self.tabs[idx].auto_refresh.as_ref()?.next_at;
        if self.tabs[idx].is_modified() {
            return Some(now + Duration::from_secs(1));
        }
        if now >= next_at && self.reload_target.is_none() {
            let back = (self.active_tab != idx).then_some(self.active_tab);
            if let Some(a) = self.tabs[idx].auto_refresh.as_mut() {
                a.return_to = back;
                a.next_at = now + every;
            }
            self.refresh_tab(idx, true, ctx);
            return Some(now + Duration::from_millis(250));
        }
        Some(next_at.max(now + Duration::from_millis(50)))
    }

    fn follow_stop(&mut self, idx: usize, msg: String) {
        self.tabs[idx].follow = None;
        self.status_message = Some((msg, Instant::now()));
    }

    /// The file was replaced: read it in full again and follow from its new
    /// end. A tab with unsaved edits is not reloaded, that would drop them.
    fn follow_reload(&mut self, idx: usize, ctx: &egui::Context) {
        if self.tabs[idx].is_modified() {
            return self.follow_stop(idx, octa::i18n::t("live.follow_stopped_edits"));
        }
        let previous = self.active_tab;
        if let Some(f) = self.tabs[idx].follow.as_mut() {
            f.state = None;
            f.return_to = (previous != idx).then_some(previous);
        }
        self.refresh_tab(idx, true, ctx);
    }

    fn follow_step(&mut self, idx: usize, ctx: &egui::Context) {
        let Some(path) = self.tabs[idx].table.source_path.clone() else {
            self.tabs[idx].follow = None;
            return;
        };
        let reloading = self.reload_target.as_ref().is_some_and(|t| t.tab == idx);
        let Some(f) = self.tabs[idx].follow.as_mut() else {
            return;
        };
        f.next_check = Instant::now() + FOLLOW_EVERY;
        // Restart after a reload: the table now holds the whole file.
        let Some(state) = f.state.as_mut() else {
            if !reloading {
                let back = f.return_to.take();
                match start_follow(&path) {
                    Ok(fresh) => {
                        self.tabs[idx].follow = Some(fresh);
                        if let Some(b) = back.filter(|&b| b < self.tabs.len())
                            && self.active_tab == idx
                        {
                            self.active_tab = b;
                        }
                    }
                    Err(e) => self.follow_stop(idx, format!("{e:#}")),
                }
            }
            return;
        };
        let step = match state.step(Path::new(&path), FOLLOW_SLICE) {
            Ok(s) => s,
            Err(e) => return self.follow_stop(idx, format!("{e:#}")),
        };
        match step {
            FollowStep::Unchanged => {}
            FollowStep::Reload => self.follow_reload(idx, ctx),
            FollowStep::Appended(text) => {
                let header = f.header.clone();
                self.follow_append(idx, &path, header.as_deref(), &text, ctx);
            }
        }
    }

    fn follow_append(
        &mut self,
        idx: usize,
        path: &str,
        header: Option<&str>,
        text: &str,
        ctx: &egui::Context,
    ) {
        let parsed = match parse_appended(self, &extension(path), header, text) {
            Ok(t) => t,
            Err(e) => return self.follow_stop(idx, format!("{e:#}")),
        };
        let Some(mut rows) = rows_for(&self.tabs[idx].table, &parsed) else {
            return self.follow_reload(idx, ctx);
        };
        let room =
            octa::formats::initial_load_rows().saturating_sub(self.tabs[idx].table.row_count());
        let cut = rows.len() > room;
        rows.truncate(room);
        let (highlight, secs) = (
            self.settings.sql_row_diff_highlight_enabled,
            self.settings.sql_row_diff_highlight_secs,
        );
        let tab = &mut self.tabs[idx];
        let at_bottom = !tab.table_state.scrolled_up;
        let first = tab.table.rows.len();
        tab.table.rows.append(&mut rows);
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
        if highlight && tab.table.rows.len() > first {
            for r in first..tab.table.rows.len() {
                tab.table.set_mark(MarkKey::Row(r), MarkColor::Green);
                tab.sql_diff_marks.push(MarkKey::Row(r));
            }
            tab.sql_diff_highlight_until =
                Some(Instant::now() + Duration::from_secs(secs.max(1) as u64));
        }
        if at_bottom {
            // Clamped to the last display row when the view draws it.
            tab.table_state.scroll_to_row(usize::MAX);
        }
        if cut {
            self.follow_stop(idx, octa::i18n::t("live.follow_limit"));
        }
    }
}

fn start_follow(path: &str) -> std::io::Result<Follow> {
    let p = Path::new(path);
    Ok(Follow {
        state: Some(FollowState::start(p)?),
        next_check: Instant::now() + FOLLOW_EVERY,
        header: if is_delimited(&extension(path)) {
            Some(first_line(p)?)
        } else {
            None
        },
        return_to: None,
    })
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
