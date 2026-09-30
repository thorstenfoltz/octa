//! **Reopen last session**: remember which tabs were open when Octa closed and
//! open them again at the next start. Files come back when
//! `restore_session` is on; cloud objects and database / API tabs each need
//! their own switch on top, because reopening them downloads or queries at
//! start-up.

use eframe::egui;
use serde::{Deserialize, Serialize};

use octa::ui::settings::AppSettings;

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionFile {
    #[serde(default)]
    tabs: Vec<SavedTab>,
}

fn session_path() -> Option<std::path::PathBuf> {
    AppSettings::config_dir().map(|d| d.join("session.toml"))
}

/// Where `tab` came from, if it can be opened again. A cloud tab's
/// `source_path` is a temp download and a compressed file's is the unpacked
/// temp copy, so both are recorded by their real origin instead; any other
/// path inside the temp folder (an archive member) is left out.
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
        if let Ok(text) = toml::to_string_pretty(&SessionFile { tabs }) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, text);
        }
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
mod tests {
    use super::*;

    #[test]
    fn cloud_and_connections_need_their_own_switch() {
        let tabs = vec![
            SavedTab::File {
                path: "a.csv".into(),
            },
            SavedTab::Cloud {
                conn_id: "c".into(),
                key: "k".into(),
            },
            SavedTab::Db {
                conn_id: "d".into(),
                catalog: None,
                schema: "s".into(),
                table: "t".into(),
            },
            SavedTab::Api {
                conn_id: "api".into(),
                path: None,
            },
        ];
        let mut settings = AppSettings {
            restore_session: true,
            ..Default::default()
        };
        assert_eq!(wanted(tabs.clone(), &settings).len(), 1);
        settings.restore_session_cloud = true;
        assert_eq!(wanted(tabs.clone(), &settings).len(), 2);
        settings.restore_session_connections = true;
        assert_eq!(wanted(tabs, &settings).len(), 4);
    }

    #[test]
    fn session_file_round_trips() {
        let file = SessionFile {
            tabs: vec![
                SavedTab::File {
                    path: "/x/a.csv".into(),
                },
                SavedTab::Db {
                    conn_id: "d".into(),
                    catalog: Some("cat".into()),
                    schema: "s".into(),
                    table: "t".into(),
                },
            ],
        };
        let text = toml::to_string_pretty(&file).unwrap();
        let back: SessionFile = toml::from_str(&text).unwrap();
        assert_eq!(back.tabs, file.tabs);
    }
}
