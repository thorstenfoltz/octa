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

use super::state::{OctaApp, TabState};

/// The tab a refresh should land in, and the source it is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReloadTarget {
    pub(crate) tab: usize,
    pub(crate) source: String,
}

impl ReloadTarget {
    /// Whether a freshly read table from `source` is the refresh this target
    /// waits for, with the tab still there to put it in.
    fn claims(&self, source: &str, tab_count: usize) -> bool {
        self.source == source && self.tab < tab_count
    }
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
        self.reload_target = in_place.then_some(ReloadTarget { tab: idx, source });
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
    /// blank tab" rule lands the table there. The tab keeps its pin, its name
    /// and its cloud origin; everything else is read fresh, as a new tab would
    /// be.
    pub(crate) fn take_reload_slot(&mut self, source: &str) -> bool {
        let tab_count = self.tabs.len();
        let Some(target) = self.reload_target.take_if(|t| t.claims(source, tab_count)) else {
            return false;
        };
        let fresh = TabState::new(self.settings.default_search_mode);
        let old = std::mem::replace(&mut self.tabs[target.tab], fresh);
        let tab = &mut self.tabs[target.tab];
        tab.pinned = old.pinned;
        tab.custom_tab_label = old.custom_tab_label;
        // A cloud tab re-read from its downloaded copy (a Tab memory reload)
        // is still that object.
        tab.cloud_origin = old.cloud_origin;
        self.active_tab = target.tab;
        true
    }

    /// Keep the tab indices the refresh bookkeeping holds pointing at the same
    /// tabs after tab `removed` is closed.
    pub(crate) fn shift_refresh_indices(&mut self, removed: usize) {
        self.reload_target = self.reload_target.take().and_then(|mut t| {
            t.tab = shifted(t.tab, removed)?;
            Some(t)
        });
        self.pending_refresh = self.pending_refresh.and_then(|mut p| {
            p.tab = shifted(p.tab, removed)?;
            Some(p)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_tab_refreshes_the_object_not_its_downloaded_copy() {
        let mut tab = TabState::new(octa::data::SearchMode::default());
        tab.table.source_path = Some("/tmp/octa-download-123.csv".into());
        tab.cloud_origin = Some(crate::app::state::CloudOrigin {
            conn_id: "s3".into(),
            key: "data/sales.csv".into(),
        });
        assert_eq!(
            tab.refresh_source().as_deref(),
            Some("cloud:s3:data/sales.csv")
        );
    }

    #[test]
    fn a_file_tab_refreshes_its_path_and_a_computed_tab_nothing() {
        let mut tab = TabState::new(octa::data::SearchMode::default());
        assert_eq!(tab.refresh_source(), None);
        tab.table.source_path = Some("/data/a.csv".into());
        assert_eq!(tab.refresh_source().as_deref(), Some("/data/a.csv"));
    }

    #[test]
    fn only_the_awaited_source_claims_the_tab() {
        let target = ReloadTarget {
            tab: 2,
            source: "/data/a.csv".into(),
        };
        assert!(target.claims("/data/a.csv", 3));
        // Another file opened meanwhile gets a tab of its own.
        assert!(!target.claims("/data/b.csv", 3));
        // The tab was closed in between: nothing to land in.
        assert!(!target.claims("/data/a.csv", 2));
    }

    #[test]
    fn closing_a_tab_keeps_the_indices_on_the_same_tabs() {
        assert_eq!(shifted(1, 3), Some(1));
        assert_eq!(shifted(3, 3), None);
        assert_eq!(shifted(4, 3), Some(3));
    }
}
