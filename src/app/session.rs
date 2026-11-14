//! **Reopen last session**: remember which tabs were open when Octa closed and
//! open them again at the next start. Files come back when
//! `restore_session` is on; cloud objects and database / API tabs each need
//! their own switch on top, because reopening them downloads or queries at
//! start-up. SQL editors come back under `restore_session` alone, whatever
//! their tab did: only their text and file link, so nothing connects.

use eframe::egui;
use serde::{Deserialize, Serialize};

use octa::ui::settings::AppSettings;

use octa::data::MarkColor;
use octa::ui::tiles::TileLayout;

use super::state::{OctaApp, TabState};

/// One tab worth reopening. Result, chart and scratch tabs have no source to
/// reopen from, so they are not recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SavedTab {
    File {
        path: String,
    },
    Cloud {
        conn_id: String,
        key: String,
    },
    Db {
        conn_id: String,
        catalog: Option<String>,
        schema: String,
        table: String,
    },
    Api {
        conn_id: String,
        path: Option<String>,
    },
}

/// Tab groups and tile sets, their members named by what they reopen from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionLayout {
    #[serde(default)]
    pub groups: Vec<SavedGroup>,
    #[serde(default)]
    pub tiles: Vec<SavedTiles>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedGroup {
    pub name: String,
    pub colour: MarkColor,
    pub folded: bool,
    pub members: Vec<SavedTab>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedTiles {
    pub layout: TileLayout,
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub members: Vec<SavedTab>,
}

/// One SQL editor: what it holds and the `.sql` file it belongs to. Results,
/// attached databases and the Run on choice stay behind, so an editor that
/// comes back reaches for no server, neither at start nor on its next Run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedEditor {
    pub query: String,
    #[serde(default)]
    pub file: Option<String>,
}

/// A tab's editors, and the tab they go back to if it reopens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedEditors {
    #[serde(default)]
    pub tab: Option<SavedTab>,
    pub editors: Vec<SavedEditor>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionFile {
    #[serde(default)]
    tabs: Vec<SavedTab>,
    #[serde(default)]
    layout: SessionLayout,
    #[serde(default)]
    sql_editors: Vec<SavedEditors>,
}

fn session_path() -> Option<std::path::PathBuf> {
    AppSettings::config_dir().map(|d| d.join("session.toml"))
}

/// Written whole or not at all, and for the owner alone: the file holds SQL
/// text now, and a query can hold a password.
fn write_session_file(path: &std::path::Path, text: &str) {
    if octa::formats::write_atomically(path, |tmp| Ok(std::fs::write(tmp, text)?)).is_ok() {
        octa::ui::settings::restrict_file_to_owner(path);
    }
}

/// The editors of `tab` worth bringing back, by the rule closing a tab follows
/// (`sql_survives_close`): a panel the user closed first has been dealt with.
pub(crate) fn saved_editors(tab: &TabState) -> Option<SavedEditors> {
    if !tab.sql_survives_close() {
        return None;
    }
    let editors: Vec<SavedEditor> = tab
        .sql_panes_in_order()
        .filter(|pane| pane.holds_text())
        .map(|pane| SavedEditor {
            query: pane.query.clone(),
            file: pane
                .file
                .as_ref()
                .map(|f| f.path.to_string_lossy().into_owned()),
        })
        .collect();
    (!editors.is_empty()).then(|| SavedEditors {
        tab: saved_tab(tab),
        editors,
    })
}

/// Where `tab` came from, if it can be opened again. A cloud tab's
/// `source_path` is a temp download and a compressed or protected file's is
/// the plain temp copy, so those are recorded by their real origin instead
/// (a protected zip asks for its passphrase again); any other path inside
/// the temp folder (an archive member) is left out.
pub(crate) fn saved_tab(tab: &TabState) -> Option<SavedTab> {
    if let Some(o) = &tab.db_origin {
        return Some(SavedTab::Db {
            conn_id: o.conn_id.clone(),
            catalog: o.catalog.clone(),
            schema: o.schema.clone(),
            table: o.table.clone(),
        });
    }
    if let Some(o) = &tab.api_origin {
        return Some(SavedTab::Api {
            conn_id: o.conn_id.clone(),
            path: o.path.clone(),
        });
    }
    if let Some(o) = &tab.cloud_origin {
        return Some(SavedTab::Cloud {
            conn_id: o.conn_id.clone(),
            key: o.key.clone(),
        });
    }
    if let Some(o) = &tab.protected_origin {
        return Some(SavedTab::File {
            path: o.zip.to_string_lossy().into_owned(),
        });
    }
    if let Some(o) = &tab.compressed_origin {
        return Some(SavedTab::File {
            path: o.original.to_string_lossy().into_owned(),
        });
    }
    let path = tab.table.source_path.as_ref()?;
    if std::path::Path::new(path).starts_with(std::env::temp_dir()) {
        return None;
    }
    Some(SavedTab::File { path: path.clone() })
}

/// The tabs to reopen under the current switches, in their saved order.
pub(crate) fn wanted(tabs: Vec<SavedTab>, settings: &AppSettings) -> Vec<SavedTab> {
    tabs.into_iter()
        .filter(|t| match t {
            SavedTab::File { .. } => true,
            SavedTab::Cloud { .. } => settings.restore_session_cloud,
            SavedTab::Db { .. } | SavedTab::Api { .. } => settings.restore_session_connections,
        })
        .collect()
}

impl OctaApp {
    /// Write the open tabs to `session.toml` on exit. With the switch off the
    /// file is removed, so a list of paths does not linger once the feature
    /// is no longer wanted.
    pub(crate) fn save_session(&self) {
        let Some(path) = session_path() else { return };
        if !self.settings.restore_session {
            let _ = std::fs::remove_file(path);
            return;
        }
        let mut tabs: Vec<SavedTab> = Vec::new();
        for t in self.tabs.iter().filter_map(saved_tab) {
            if !tabs.contains(&t) {
                tabs.push(t);
            }
        }
        if let Ok(text) = toml::to_string_pretty(&SessionFile {
            tabs,
            layout: self.layout_to_save(),
            sql_editors: self.editors_to_save(),
        }) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            write_session_file(&path, &text);
        }
    }

    /// The editors `save_session` writes: those on the tabs, and any of the
    /// last session's still waiting for theirs. With Keep the SQL editor open
    /// off an editor closes with its tab, so closing Octa ends it too.
    pub(crate) fn editors_to_save(&self) -> Vec<SavedEditors> {
        if !self.settings.sql_keep_open {
            return Vec::new();
        }
        self.tabs
            .iter()
            .filter_map(saved_editors)
            .chain(self.pending_editors.iter().cloned())
            .collect()
    }

    /// Put the last session's editors back. Those whose tab is open go on it.
    /// The others wait while tabs may still arrive, then take a blank tab: the
    /// tab did not come back, and that is no reason to lose the text.
    pub(crate) fn adopt_session_editors(&mut self) {
        if self.pending_editors.is_empty() {
            return;
        }
        let loading = self.session_still_loading();
        for saved in std::mem::take(&mut self.pending_editors) {
            let own = saved.tab.as_ref().and_then(|wanted| {
                self.tabs
                    .iter()
                    .position(|t| saved_tab(t).as_ref() == Some(wanted))
            });
            let host = match own {
                Some(i) => i,
                None if loading => {
                    self.pending_editors.push(saved);
                    continue;
                }
                None => self.blank_tab_for_editors(),
            };
            for editor in saved.editors {
                self.tabs[host].put_saved_sql_editor(editor.query, editor.file.map(Into::into));
            }
            self.open_sql_panel(host);
        }
    }

    /// The active tab if nothing is on it, else a new one at the end of the
    /// bar: an editor never lands on another file, where `data` would change.
    fn blank_tab_for_editors(&mut self) -> usize {
        let active = &self.tabs[self.active_tab];
        if !active.has_own_content() && active.sql_can_host() {
            return self.active_tab;
        }
        self.tabs
            .push(TabState::new(self.settings.default_search_mode));
        self.tabs.len() - 1
    }

    /// The layout `save_session` writes: the last session's, while it still
    /// waits for its tabs (quitting before they reopened must not lose it),
    /// else the one on screen.
    pub(crate) fn layout_to_save(&self) -> SessionLayout {
        self.pending_layout
            .clone()
            .unwrap_or_else(|| self.session_layout())
    }

    /// The groups and tile sets worth saving: members that have a
    /// `SavedTab`, and only sets that keep two or more of them.
    pub(crate) fn session_layout(&self) -> SessionLayout {
        let members = |pick: &dyn Fn(&TabState) -> bool| -> Vec<SavedTab> {
            self.tabs
                .iter()
                .filter(|t| pick(t))
                .filter_map(saved_tab)
                .collect()
        };
        SessionLayout {
            groups: self
                .tab_groups
                .iter()
                .map(|g| SavedGroup {
                    name: g.name.clone(),
                    colour: g.colour,
                    folded: g.folded,
                    members: members(&|t| t.group == Some(g.id)),
                })
                .filter(|g| g.members.len() >= 2)
                .collect(),
            tiles: self
                .tile_sets
                .iter()
                .map(|s| SavedTiles {
                    layout: s.layout,
                    xs: s.xs.clone(),
                    ys: s.ys.clone(),
                    members: members(&|t| t.tile == Some(s.id)),
                })
                .filter(|s| s.members.len() >= 2)
                .collect(),
        }
    }

    /// Whether anything Octa opens for the user is still in flight, so a
    /// saved member that is not open yet may yet arrive: the open queue, a
    /// worker thread (cloud and API reopens run as long tasks, which the
    /// status bar spinner does not show), or a finished one whose result
    /// still waits in its slot to become a tab.
    fn session_still_loading(&self) -> bool {
        let waiting = |slot: Result<bool, ()>| slot.unwrap_or(false);
        !self.pending_open_queue.is_empty()
            || self.long_task_label().is_some()
            || self.long_tasks.running().is_some()
            || waiting(
                self.cloud_browser
                    .pending_open
                    .lock()
                    .map(|p| !p.is_empty())
                    .map_err(drop),
            )
            || waiting(
                self.db_browser
                    .pending_open
                    .lock()
                    .map(|p| !p.is_empty())
                    .map_err(drop),
            )
            || waiting(
                self.api_pending_open
                    .lock()
                    .map(|p| !p.is_empty())
                    .map_err(drop),
            )
    }

    /// Rebuild the saved groups and tile sets from the tabs that came back.
    /// Waits while members may still arrive; members that never open drop
    /// out, and a group or set left with fewer than two tabs is not made.
    pub(crate) fn adopt_session_layout(&mut self) {
        let Some(layout) = self.pending_layout.as_ref() else {
            return;
        };
        let open: Vec<(super::tabs::TabId, Option<SavedTab>)> =
            self.tabs.iter().map(|t| (t.id, saved_tab(t))).collect();
        // Each open tab matches at most one saved member (per saved list).
        let resolve = |members: &[SavedTab]| -> Vec<Option<super::tabs::TabId>> {
            let mut used = vec![false; open.len()];
            members
                .iter()
                .map(|m| {
                    let i = (0..open.len()).find(|&i| !used[i] && open[i].1.as_ref() == Some(m))?;
                    used[i] = true;
                    Some(open[i].0)
                })
                .collect()
        };
        let all_in = |r: &[Option<super::tabs::TabId>]| r.iter().all(Option::is_some);
        let g: Vec<_> = layout.groups.iter().map(|g| resolve(&g.members)).collect();
        let t: Vec<_> = layout.tiles.iter().map(|s| resolve(&s.members)).collect();
        if !g.iter().chain(&t).all(|r| all_in(r)) && self.session_still_loading() {
            return;
        }
        let layout = self.pending_layout.take().unwrap_or_default();
        let found = |r: Vec<Option<super::tabs::TabId>>| -> Vec<super::tabs::TabId> {
            r.into_iter().flatten().collect()
        };
        for (saved, ids) in layout.groups.into_iter().zip(g) {
            let ids = found(ids);
            if ids.len() < 2 {
                continue;
            }
            // `create_group` may reorder tabs, so index by id each time.
            let at = self.tab_indices(&ids);
            let id = self.create_group(saved.name, saved.colour, &at);
            if let Some(g) = self.tab_groups.iter_mut().find(|g| g.id == id) {
                g.folded = saved.folded;
            }
        }
        for (saved, ids) in layout.tiles.into_iter().zip(t) {
            let ids = found(ids);
            if ids.len() < 2 {
                continue;
            }
            let at = self.tab_indices(&ids);
            let id = self.tile_tabs(&at, saved.layout);
            let (cols, rows) = octa::ui::tiles::shape(saved.layout, at.len());
            if let Some(set) = self.tile_sets.iter_mut().find(|s| s.id == id) {
                if saved.xs.len() + 1 == cols {
                    set.xs = saved.xs;
                }
                if saved.ys.len() + 1 == rows {
                    set.ys = saved.ys;
                }
            }
        }
    }

    fn tab_indices(&self, ids: &[super::tabs::TabId]) -> Vec<usize> {
        (0..self.tabs.len())
            .filter(|&i| ids.contains(&self.tabs[i].id))
            .collect()
    }

    /// Reopen the last session at start-up. Files are returned for the
    /// caller's open queue (so they share the de-duplication with pinned tabs
    /// and command-line files); cloud, database and API tabs start their own
    /// background loads here. A file that is gone is skipped, not an error.
    pub(crate) fn restore_session(&mut self, ctx: &egui::Context) -> Vec<std::path::PathBuf> {
        if !self.settings.restore_session {
            return Vec::new();
        }
        let saved: SessionFile = session_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        self.pending_layout = Some(saved.layout).filter(|l| *l != SessionLayout::default());
        self.pending_editors = saved.sql_editors;
        let mut files = Vec::new();
        for tab in wanted(saved.tabs, &self.settings) {
            match tab {
                SavedTab::File { path } => {
                    let path = std::path::PathBuf::from(path);
                    if path.exists() {
                        files.push(path);
                    }
                }
                SavedTab::Cloud { conn_id, key } => {
                    let name = key.rsplit('/').next().unwrap_or(&key).to_string();
                    self.open_cloud_object(ctx, conn_id, key, name);
                }
                SavedTab::Db {
                    conn_id,
                    catalog,
                    schema,
                    table,
                } => self.open_db_table(ctx, conn_id, catalog, schema, table),
                SavedTab::Api { conn_id, path } => {
                    let conn = self
                        .settings
                        .api_connections
                        .iter()
                        .find(|c| c.id == conn_id)
                        .cloned();
                    if let Some(conn) = conn {
                        self.start_api_fetch(conn, path, ctx);
                    }
                }
            }
        }
        files
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
