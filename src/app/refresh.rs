//! Refresh a tab: read its source again (file, database table, cloud object
//! or API endpoint) into the same tab, or into a new one.
//!
//! Every source opens asynchronously on its own path, so "the same tab" is a
//! note left for later: `reload_target` names the tab and the source it waits
//! for, and the path that places a finished table calls `take_reload_slot`
//! with that table's source. Only a match empties the tab, and the ordinary
//! "reuse a blank tab" rule on every placement path then fills it. A read that
//! fails never consumes the note, so it cannot blank the tab either.

use eframe::egui;

use octa::ui::settings::RefreshBehaviour;

use std::sync::atomic::Ordering::Relaxed;

use super::state::{OctaApp, TabState};

/// The tab a refresh should land in, and the source it is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReloadTarget {
    pub(crate) tab: usize,
    pub(crate) source: String,
    /// The user explicitly chose to throw this tab's unsaved edits away
    /// (the Refresh dialog's "discard" answer). Without it a read landing on
    /// an edited tab is dropped.
    pub(crate) discard_edits: bool,
}

impl ReloadTarget {
    /// Whether a freshly read table from `source` is the refresh this target
    /// waits for, with the tab still there to put it in.
    fn claims(&self, source: &str, tab_count: usize) -> bool {
        self.source == source && self.tab < tab_count
    }
}

/// What `take_reload_slot` did with a freshly read table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReloadSlot {
    /// The target tab was emptied: place the table there.
    Replaced,
    /// Not a refresh: place the table as usual.
    NotClaimed,
    /// A refresh, but the tab has unsaved edits: discard the table.
    Dropped,
}

/// Where tab index `tab` ends up once tab `removed` is closed: `None` when it
/// was the closed tab itself.
fn shifted(tab: usize, removed: usize) -> Option<usize> {
    match tab.cmp(&removed) {
        std::cmp::Ordering::Less => Some(tab),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(tab - 1),
    }
}

/// Keep `slot` while `follow` can re-point it; drop it once a tab it needs
/// is closed.
fn keep<T>(slot: &mut Option<T>, follow: impl FnOnce(&mut T) -> Option<()>) {
    *slot = slot.take().and_then(|mut x| follow(&mut x).map(|()| x));
}

/// `kept`, after running `stop` when it is `None`: a dialog dropped with a
/// worker still running stops it, as its own Close button does.
fn or_stop(kept: Option<()>, stop: impl FnOnce()) -> Option<()> {
    if kept.is_none() {
        stop();
    }
    kept
}

/// Keep a list of one entry per tab in tab order after `map`: entry `i`
/// moves to `map(i)`, a closed tab's entry goes, and a slot nothing moved to
/// (a tab opened after the list was made) gets `fill`. `n` is the tab count.
pub(crate) fn follow_tabs<T>(
    list: &mut Vec<T>,
    map: &dyn Fn(usize) -> Option<usize>,
    n: usize,
    fill: impl Fn(usize) -> T,
) {
    let mut out: Vec<Option<T>> = std::iter::repeat_with(|| None).take(n).collect();
    for (i, x) in std::mem::take(list).into_iter().enumerate() {
        if let Some(j) = map(i).filter(|&j| j < n) {
            out[j] = Some(x);
        }
    }
    *list = out
        .into_iter()
        .enumerate()
        .map(|(j, x)| x.unwrap_or_else(|| fill(j)))
        .collect();
}

/// A refresh waiting on the "this tab or a new tab" dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingRefresh {
    pub(crate) tab: usize,
    /// The dialog's "Don't ask again" tick box, kept here between frames.
    pub(crate) dont_ask: bool,
}

pub(crate) fn api_source(conn_id: &str, path: Option<&str>) -> String {
    format!("api:{conn_id}:{}", path.unwrap_or_default())
}

pub(crate) fn db_source(conn_id: &str, catalog: Option<&str>, schema: &str, table: &str) -> String {
    format!(
        "db:{conn_id}:{}:{schema}:{table}",
        catalog.unwrap_or_default()
    )
}

pub(crate) fn cloud_source(conn_id: &str, key: &str) -> String {
    format!("cloud:{conn_id}:{key}")
}

impl TabState {
    /// What reading this tab again means, as the key `take_reload_slot`
    /// matches on. `None` for a tab computed inside Octa (a SQL result, a
    /// summary, a scratch buffer): there is nothing to read again.
    ///
    /// The origins are checked before `source_path` because a cloud tab's
    /// path is only its downloaded temp copy.
    pub(crate) fn refresh_source(&self) -> Option<String> {
        if let Some(o) = &self.api_origin {
            return Some(api_source(&o.conn_id, o.path.as_deref()));
        }
        if let Some(o) = &self.db_origin {
            return Some(db_source(
                &o.conn_id,
                o.catalog.as_deref(),
                &o.schema,
                &o.table,
            ));
        }
        if let Some(o) = &self.cloud_origin {
            return Some(cloud_source(&o.conn_id, &o.key));
        }
        self.table.source_path.clone()
    }
}

impl OctaApp {
    /// Ctrl+R and the tab menu's Refresh. Asks first when the setting says so
    /// and always when the tab holds unsaved changes, because refreshing in
    /// place throws those away.
    pub(crate) fn request_refresh(&mut self, idx: usize, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        if tab.refresh_source().is_none() {
            self.status_message =
                Some((octa::i18n::t("refresh.nothing"), std::time::Instant::now()));
            return;
        }
        match self.settings.refresh_behaviour {
            _ if tab.is_modified() => self.ask_refresh(idx),
            RefreshBehaviour::Ask => self.ask_refresh(idx),
            RefreshBehaviour::InPlace => self.refresh_tab(idx, true, ctx),
            RefreshBehaviour::NewTab => self.refresh_tab(idx, false, ctx),
        }
    }

    fn ask_refresh(&mut self, idx: usize) {
        self.pending_refresh = Some(PendingRefresh {
            tab: idx,
            dont_ask: false,
        });
    }

    /// Read tab `idx`'s source again, into that tab (`in_place`) or a new one.
    pub(crate) fn refresh_tab(&mut self, idx: usize, in_place: bool, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        let Some(source) = tab.refresh_source() else {
            return;
        };
        self.reload_target = in_place.then_some(ReloadTarget {
            tab: idx,
            source,
            discard_edits: false,
        });
        let api = tab.api_origin.clone();
        let db = tab.db_origin.clone();
        let cloud = tab.cloud_origin.clone();
        let path = tab.table.source_path.clone();
        if let Some(origin) = api {
            let Some(conn) = self
                .settings
                .api_connections
                .iter()
                .find(|c| c.id == origin.conn_id)
                .cloned()
            else {
                self.reload_target = None;
                self.status_message = Some((
                    octa::i18n::t("api.connection_gone"),
                    std::time::Instant::now(),
                ));
                return;
            };
            self.start_api_fetch(conn, origin.path, ctx);
        } else if let Some(o) = db {
            self.open_db_table(ctx, o.conn_id, o.catalog, o.schema, o.table);
        } else if let Some(o) = cloud {
            let name = o
                .key
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(&o.key)
                .to_string();
            self.open_cloud_object(ctx, o.conn_id, o.key, name);
        } else if let Some(path) = path {
            if in_place {
                self.active_tab = idx;
                self.reload_active_file();
            } else if self.tabs[idx].sheet_name.is_some() {
                // One sheet or table only. The active tab holds data (or is
                // the blank start tab), so the placement opens a new tab.
                self.reread_file(idx, std::path::PathBuf::from(path));
            } else {
                self.load_file_in_new_tab(std::path::PathBuf::from(path));
            }
        }
    }

    /// Called by every path that places a freshly read table, with that
    /// table's source. When it is the refresh `reload_target` waits for, the
    /// target tab is emptied and made active, so the placement's "reuse a
    /// blank tab" rule lands the table there. The tab keeps its pin, its name,
    /// its cloud origin and its SQL editors; everything else is read fresh, as
    /// a new tab would be.
    pub(crate) fn take_reload_slot(&mut self, source: &str) -> ReloadSlot {
        let tab_count = self.tabs.len();
        let Some(target) = self.reload_target.take_if(|t| t.claims(source, tab_count)) else {
            return ReloadSlot::NotClaimed;
        };
        // Edits made since the read began must survive: the new read is
        // dropped, not placed, unless the user chose to discard them.
        if !target.discard_edits && self.tabs[target.tab].is_modified() {
            if let Some(a) = self.tabs[target.tab].auto_refresh.as_mut() {
                a.next_at = std::time::Instant::now() + a.every;
                a.return_to = None;
            }
            self.status_message = Some((
                octa::i18n::t("refresh.dropped_edits"),
                std::time::Instant::now(),
            ));
            return ReloadSlot::Dropped;
        }
        // The edits question was about the rows this replaces.
        if self
            .view_edit_prompt
            .as_ref()
            .is_some_and(|p| p.tab == target.tab)
        {
            self.view_edit_prompt = None;
        }
        let fresh = TabState::new(self.settings.default_search_mode);
        let mut old = std::mem::replace(&mut self.tabs[target.tab], fresh);
        let tab = &mut self.tabs[target.tab];
        // A refresh is not the SQL editors' business: they stay as they are,
        // and a query running in one still lands there.
        old.move_sql_to(tab);
        tab.sql_maximised = old.sql_maximised;
        tab.is_reload_slot = true;
        tab.id = old.id;
        tab.group = old.group;
        tab.tile = old.tile;
        tab.pinned = old.pinned;
        // A live tab stays live across its own reload. Auto-refresh also
        // hands over the rows it is about to replace, to highlight the change.
        tab.follow = old.follow;
        // The reload read the whole file: the old offset is stale, follow
        // starts again from the end of it (see `drive_live_tabs`).
        if let Some(f) = tab.follow.as_mut() {
            f.state = None;
        }
        tab.auto_refresh = old.auto_refresh;
        if let Some(a) = tab.auto_refresh.as_mut() {
            a.before = Some(std::mem::replace(
                &mut old.table,
                octa::data::DataTable::empty(),
            ));
        }
        tab.custom_tab_label = old.custom_tab_label;
        // A cloud tab re-read from its downloaded copy (a Tab memory reload)
        // is still that object.
        tab.cloud_origin = old.cloud_origin;
        // Likewise a protected tab re-read from its plain copy: moving the
        // handle keeps the folder that copy lives in.
        tab.protected_origin = old.protected_origin;
        self.active_tab = target.tab;
        ReloadSlot::Replaced
    }

    /// A read failed: forget the refresh it would have answered, so the tab
    /// can be refreshed (or auto-refreshed) again. `source_prefix` is the
    /// kind of source that failed (`db:`, `api:`, `cloud:`) or a file path.
    pub(crate) fn fail_reload(&mut self, source_prefix: &str) {
        self.reload_target.take_if(|t| {
            if source_prefix.ends_with(':') {
                t.source.starts_with(source_prefix)
            } else {
                t.source == source_prefix
            }
        });
    }

    /// Keep the tab indices the refresh bookkeeping holds pointing at the same
    /// tabs after tab `removed` is closed.
    pub(crate) fn shift_refresh_indices(&mut self, removed: usize) {
        self.remap_tab_indices(&|i| shifted(i, removed));
    }

    /// Re-point every held tab index through `map` (`None` = the tab is gone).
    /// `active_tab` is not touched: close and move each handle it their way.
    ///
    /// The one place a tab position is kept up: every field that names a tab
    /// by index belongs here. What belongs to a closed tab goes with it (its
    /// prompts, banners, jobs and the dialogs bound to it); a dialog that
    /// lists tabs to pick from only loses that pick.
    pub(crate) fn remap_tab_indices(&mut self, map: &dyn Fn(usize) -> Option<usize>) {
        // Point `i` at where its tab went; `None` once that tab is closed.
        let re = |i: &mut usize| -> Option<()> {
            *i = map(*i)?;
            Some(())
        };
        // Refresh, follow, auto-refresh and the SQL copies of other tabs.
        keep(&mut self.reload_target, |t| re(&mut t.tab));
        keep(&mut self.pending_refresh, |p| re(&mut p.tab));
        keep(&mut self.auto_refresh_dialog, |d| re(&mut d.tab));
        keep(&mut self.view_edit_prompt, |p| re(&mut p.tab));
        for t in &mut self.tabs {
            if let Some(f) = t.follow.as_mut() {
                f.return_to = f.return_to.and_then(map);
            }
            if let Some(a) = t.auto_refresh.as_mut() {
                a.return_to = a.return_to.and_then(map);
            }
            if let Some(c) = t.sql_other_tabs.as_mut() {
                c.remap_tabs(map);
            }
        }
        // Load banners and pickers: Dismiss and a picked layout write into
        // their tab.
        keep(&mut self.pending_date_warning, |w| re(&mut w.tab_idx));
        keep(&mut self.pending_date_parse_warning, |w| re(&mut w.tab_idx));
        keep(&mut self.pending_trim_warning, |w| re(&mut w.tab_idx));
        keep(&mut self.pending_number_warning, |w| re(&mut w.tab_idx));
        keep(&mut self.pending_raw_perf_prompt, |p| re(&mut p.tab_idx));
        self.pending_date_pickers
            .retain_mut(|q| re(&mut q.tab_idx).is_some());
        self.pending_number_pickers
            .retain_mut(|q| re(&mut q.tab_idx).is_some());
        keep(&mut self.load_all, |d| re(&mut d.tab_idx));
        // Save and close questions, each resumed on its tab.
        keep(&mut self.pending_save_as, |p| re(&mut p.tab_idx));
        keep(&mut self.pending_round_save, |p| re(&mut p.tab_idx));
        keep(&mut self.pending_xlsx_style_save, |p| re(&mut p.tab_idx));
        keep(&mut self.pending_schema_change_save, |p| re(&mut p.tab_idx));
        keep(&mut self.pending_partial_save_confirm, |p| re(&mut p.0));
        keep(&mut self.pending_db_write_back, |p| re(&mut p.tab_idx));
        self.pending_close_tab = self.pending_close_tab.and_then(map);
        self.pending_overwrite_confirm = self.pending_overwrite_confirm.and_then(map);
        // The password and conflict dialogs are not modal: closing their own
        // tab drops them, another tab moving re-points them.
        keep(&mut self.password_save, |p| re(&mut p.tab_idx));
        keep(&mut self.db_conflicts, |c| re(&mut c.tab_idx));
        // A write-back in flight re-baselines its tab when it lands: follow
        // the tab, and land nowhere once it is closed.
        if let Some(job) = self.db_write_back_job.as_mut() {
            job.tab_idx = map(job.tab_idx).unwrap_or(usize::MAX);
        }
        // Ask answers land in the tab that asked.
        keep(&mut self.ask_filter_job, |j| re(&mut j.tab_idx));
        keep(&mut self.ask_sql_job, |j| re(&mut j.tab_idx));
        // Dialogs bound to one tab close with it.
        keep(&mut self.tab_rename_draft, |d| re(&mut d.tab_index));
        keep(&mut self.cloud_upload_dialog, |d| re(&mut d.tab_idx));
        keep(&mut self.cell_history_dialog, |d| re(&mut d.tab));
        keep(&mut self.recipe_key_dialog, |d| re(&mut d.tab));
        keep(&mut self.file_versions_dialog, |d| {
            or_stop(re(&mut d.tab_idx), || {
                if let Some(lake) = d.lake.as_mut() {
                    lake.cancel();
                }
            })
        });
        keep(&mut self.lookups_dialog, |d| {
            or_stop(re(&mut d.tab), || d.cancel.store(true, Relaxed))
        });
        // Two-sided pickers close once either side does.
        keep(&mut self.dist_compare_dialog, |d| {
            re(&mut d.tab_a)?;
            re(&mut d.tab_b)
        });
        keep(&mut self.referential_dialog, |d| {
            re(&mut d.parent_tab)?;
            re(&mut d.child_tab)
        });
        keep(&mut self.join_diag_dialog, |d| {
            re(&mut d.left_tab)?;
            re(&mut d.right_tab)
        });
        keep(&mut self.join_dialog, |d| {
            if let Some(s) = d.spatial.as_mut() {
                s.layers = s.layers.iter().filter_map(|&i| map(i)).collect();
            }
            re(&mut d.left_tab)?;
            re(&mut d.right_tab)
        });
        // A fuzzy-join step whose tab closed goes, as its x button would
        // remove it; the left table or the last step going closes the dialog.
        keep(&mut self.fuzzy_join_dialog, |d| {
            d.steps.retain_mut(|s| re(&mut s.right_tab).is_some());
            let kept = re(&mut d.left_tab).filter(|_| !d.steps.is_empty());
            or_stop(kept, || d.cancel.store(true, Relaxed))
        });
        // Lists of tab picks lose a closed tab and keep the rest.
        self.tab_multi_selection = self
            .tab_multi_selection
            .iter()
            .filter_map(|&i| map(i))
            .collect();
        if let Some(d) = self.tab_group_dialog.as_mut() {
            d.members = d.members.iter().filter_map(|&i| map(i)).collect();
        }
        if let Some(d) = self.join_keys_dialog.as_mut() {
            d.remap_tabs(map);
        }
        if let Some(d) = self.rel_map_dialog.as_mut() {
            d.remap_tabs(map);
        }
        if let Some(d) = self.test_data_dialog.as_mut() {
            d.remap_tabs(map);
        }
        if let Some(d) = self.merge_versions_dialog.as_mut() {
            d.remap_tabs(map);
        }
        if let Some(d) = self.drift_dialog.as_mut() {
            d.side_a.remap_tab(map);
            d.side_b.remap_tab(map);
        }
        // One entry per tab, in tab order.
        if let Some(d) = self.workbook_dialog.as_mut() {
            d.remap_tabs(map, &self.tabs);
        }
        self.remap_union_dialog(map);
    }

    /// Move the tab at `from` so it ends up at index `to`; every held index
    /// and the active tab follow their tabs.
    pub(crate) fn move_tab(&mut self, from: usize, to: usize) {
        let n = self.tabs.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let t = self.tabs.remove(from);
        self.tabs.insert(to, t);
        self.remap_tab_indices(&|i| Some(super::tabs::order::moved(i, from, to)));
        self.active_tab = super::tabs::order::moved(self.active_tab, from, to);
    }

    /// Put the tabs in a new order: position `k` takes old tab `order[k]`.
    /// Ignored unless `order` is a permutation of `0..tabs.len()`.
    pub(crate) fn reorder_tabs(&mut self, order: &[usize]) {
        let Some(inv) = super::tabs::order::inverse(order, self.tabs.len()) else {
            return;
        };
        let mut old: Vec<Option<TabState>> = std::mem::take(&mut self.tabs)
            .into_iter()
            .map(Some)
            .collect();
        self.tabs = order.iter().filter_map(|&o| old[o].take()).collect();
        self.remap_tab_indices(&|i| inv.get(i).copied());
        self.active_tab = inv.get(self.active_tab).copied().unwrap_or(self.active_tab);
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
