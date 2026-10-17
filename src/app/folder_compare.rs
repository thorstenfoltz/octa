//! Compare folders in the app: each folder becomes a [`FolderSource`] on a
//! worker, the shared engine (`octa::data::folder_compare`, the same one the
//! CLI `--compare-folders` and the MCP tool `compare_folders` run) lists and
//! hashes there, and the result opens as a table tab. A row's two copies open
//! side by side in the Compare view through "Compare these files...".

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use eframe::egui;

use octa::data::folder_compare::scan::{FolderSource, ScanProgress, cloud_prefix, compare_sources};
use octa::data::folder_compare::{STATUS_DIFFERENT, STATUS_MISSING};
use octa::data::{CellValue, DataTable};
use octa::i18n::t;
use octa::ui::settings::AppSettings;
use octa::ui::settings::cloud_secrets::resolve_creds;

use super::cloud_browser::{CloudOpenResult, bind_bucket, fetch_object_to_temp};
use super::dialogs::folder_compare::FolderCompareState;
use super::state::{OctaApp, TabState};

/// Where a compared folder lives, kept so a row's files can be opened later.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GuiFolderRoot {
    Local(PathBuf),
    /// A folder in a saved cloud connection (sidebar), as the tree names it:
    /// bucket-qualified for an account-level connection.
    Cloud {
        conn_id: String,
        key: String,
    },
}

/// A folder-comparison result tab: what it compared, for "Compare these files...".
#[derive(Clone)]
pub(crate) struct FolderCompareOrigin {
    pub(crate) roots: Vec<GuiFolderRoot>,
    /// Column labels, one per root, in the same order.
    pub(crate) labels: Vec<String>,
}

/// `(different, missing)` rows of a comparison table.
pub(crate) fn status_counts(table: &DataTable) -> (usize, usize) {
    let count = |want: &str| {
        (0..table.row_count())
            .filter(|&r| matches!(table.get(r, 1), Some(CellValue::String(s)) if s == want))
            .count()
    };
    (count(STATUS_DIFFERENT), count(STATUS_MISSING))
}

/// Whether Octa can open `rel` as a table or text: its extension is one of
/// `known` (the registry's plus the Settings text-mode list), looking through
/// one `.gz` / `.zst`, since `data.csv.gz` opens as the csv inside it.
pub(crate) fn comparable(rel: &str, known: &[String]) -> bool {
    let path = Path::new(rel);
    let inner = octa::formats::compression::detect_codec(path)
        .and_then(|_| octa::formats::compression::inner_file_name(path));
    Path::new(inner.as_deref().unwrap_or(rel))
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| known.iter().any(|k| k.eq_ignore_ascii_case(ext)))
}

/// Resolve one root to a source, on the worker thread (cloud credentials can
/// shell out to the aws/az/gcloud CLIs). `bind_bucket` passes a normal
/// connection through and binds an account-level one to the key's bucket.
fn source_for_root(root: &GuiFolderRoot, settings: &AppSettings) -> anyhow::Result<FolderSource> {
    match root {
        GuiFolderRoot::Local(p) => Ok(FolderSource::Local(p.clone())),
        GuiFolderRoot::Cloud { conn_id, key } => {
            let conn = settings
                .cloud_connections
                .iter()
                .find(|c| &c.id == conn_id)
                .ok_or_else(|| anyhow::anyhow!("the cloud connection no longer exists"))?;
            let (bconn, sub) = bind_bucket(conn, key);
            let creds = resolve_creds(&bconn, settings);
            let provider = octa::cloud::build_provider(&bconn, &creds)?;
            Ok(FolderSource::Cloud {
                provider,
                prefix: cloud_prefix(&sub),
            })
        }
    }
}

impl OctaApp {
    /// Display string for a root: the path, or `<connection name>/<key>`.
    pub(crate) fn folder_root_display(&self, root: &GuiFolderRoot) -> String {
        match root {
            GuiFolderRoot::Local(p) => p.display().to_string(),
            GuiFolderRoot::Cloud { conn_id, key } => {
                let name = self
                    .settings
                    .cloud_connections
                    .iter()
                    .find(|c| &c.id == conn_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| conn_id.clone());
                format!("{name}/{}", key.trim_end_matches('/'))
            }
        }
    }

    /// Put a folder into the Compare folders dialog, opening it if needed.
    pub(crate) fn add_folder_to_comparison(&mut self, root: GuiFolderRoot) {
        self.folder_compare
            .get_or_insert_with(FolderCompareState::new)
            .add_root(root);
    }

    /// List, hash and compare the dialog's folders on a worker.
    pub(crate) fn start_folder_compare(&mut self, ctx: &egui::Context) {
        let Some(st) = &self.folder_compare else {
            return;
        };
        let roots = st.roots.clone();
        let names: Vec<String> = roots.iter().map(|r| self.folder_root_display(r)).collect();
        let settings = self.settings.clone();
        let cap = settings.folder_compare_cap();
        let Some(st) = self.folder_compare.as_mut() else {
            return;
        };
        st.run_roots = roots.clone();
        st.cancel.store(false, Ordering::Relaxed);
        st.running.store(true, Ordering::Relaxed);
        if let Ok(mut p) = st.progress.lock() {
            p.clear();
        }
        let recursive = st.recursive;
        let running = std::sync::Arc::clone(&st.running);
        let cancel = std::sync::Arc::clone(&st.cancel);
        let progress = std::sync::Arc::clone(&st.progress);
        let result = std::sync::Arc::clone(&st.result);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // Clears `running` however the worker ends, after the result.
            let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
            let outcome = (|| {
                let mut sources = Vec::with_capacity(roots.len());
                for (root, name) in roots.iter().zip(names) {
                    sources.push((name, source_for_root(root, &settings)?));
                }
                compare_sources(sources, recursive, cap, &cancel, &mut |p| {
                    let text = match p {
                        ScanProgress::Listing(l) => {
                            t("folder_compare.listing").replace("{folder}", l)
                        }
                        ScanProgress::Hashing { done, total } => t("folder_compare.hashing")
                            .replace("{done}", &done.to_string())
                            .replace("{total}", &total.to_string()),
                    };
                    if let Ok(mut s) = progress.lock() {
                        *s = text;
                    }
                    ctx.request_repaint();
                })
            })();
            if let Ok(mut slot) = result.lock() {
                *slot = Some(outcome.map_err(|e| format!("{e:#}")));
            }
            ctx.request_repaint();
        });
    }

    /// A finished comparison: open the table and close the dialog, or say
    /// why it failed and keep the dialog so a folder can be fixed. A cancel
    /// ends the same way as a failure (a bare "cancelled" error), so the flag
    /// is what tells them apart, never the message.
    pub(crate) fn drain_folder_compare(&mut self) {
        let Some(st) = &self.folder_compare else {
            return;
        };
        let outcome = st.result.lock().ok().and_then(|mut s| s.take());
        let Some(outcome) = outcome else {
            return;
        };
        let cancelled = st.cancel.load(Ordering::Relaxed);
        let roots = st.run_roots.clone();
        let (table, listings) = match outcome {
            Ok(done) => done,
            Err(e) => {
                if !cancelled {
                    self.status_message = Some((
                        t("folder_compare.failed").replace("{error}", &e),
                        std::time::Instant::now(),
                    ));
                }
                return;
            }
        };
        let labels: Vec<String> = listings.iter().map(|l| l.label.clone()).collect();
        let (diff, missing) = status_counts(&table);
        let rows = table.row_count();
        let mut tab = TabState::new(self.settings.default_search_mode);
        tab.table = table;
        tab.custom_tab_label = Some(format!(
            "{} ({})",
            t("folder_compare.tab_label"),
            labels.join(" / ")
        ));
        if let Some(l) = listings.iter().find(|l| l.truncated) {
            tab.parse_error_banner = Some(
                t("folder_compare.truncated")
                    .replace(
                        "{n}",
                        &octa::ui::status_bar::format_number(self.settings.folder_compare_cap()),
                    )
                    .replace("{folder}", &l.label),
            );
        }
        tab.folder_compare = Some(FolderCompareOrigin { roots, labels });
        tab.filter_dirty = true;
        self.push_result_tab(tab);
        self.folder_compare = None;
        self.status_message = Some((
            t("folder_compare.done")
                .replace("{n}", &octa::ui::status_bar::format_number(rows))
                .replace("{diff}", &diff.to_string())
                .replace("{missing}", &missing.to_string()),
            std::time::Instant::now(),
        ));
    }

    /// "Compare these files...": open folder `a`'s copy of the file on `row`
    /// and compare it with folder `b`'s. Cloud copies download on a worker
    /// first; local ones open at once.
    pub(crate) fn open_folder_compare_pair(
        &mut self,
        ctx: &egui::Context,
        row: usize,
        a: usize,
        b: usize,
    ) {
        let tab = &self.tabs[self.active_tab];
        let Some(origin) = tab.folder_compare.clone() else {
            return;
        };
        // By name: the user may have moved another column to the front.
        let path_col = tab.table.columns.iter().position(|c| c.name == "path");
        let Some(rel) = path_col
            .and_then(|c| tab.table.get(row, c))
            .map(|v| v.to_string())
        else {
            return;
        };
        let (Some(ra), Some(rb)) = (origin.roots.get(a).cloned(), origin.roots.get(b).cloned())
        else {
            return;
        };
        let mut known = self.registry.all_extensions();
        known.extend(self.settings.text_mode_extensions.iter().cloned());
        if !comparable(&rel, &known) {
            self.status_message = Some((
                t("folder_compare.not_a_table").replace("{file}", &rel),
                std::time::Instant::now(),
            ));
            return;
        }
        let label = format!("{rel} ({} / {})", origin.labels[a], origin.labels[b]);
        if let (GuiFolderRoot::Local(pa), GuiFolderRoot::Local(pb)) = (&ra, &rb) {
            self.start_pair_compare(pa.join(&rel), pb.join(&rel), label);
            return;
        }
        self.status_message = Some((
            t("folder_compare.downloading").replace("{file}", &rel),
            std::time::Instant::now(),
        ));
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let fetch = |root: &GuiFolderRoot| -> anyhow::Result<PathBuf> {
                match root {
                    GuiFolderRoot::Local(p) => Ok(p.join(&rel)),
                    GuiFolderRoot::Cloud { conn_id, key } => {
                        let conn = settings
                            .cloud_connections
                            .iter()
                            .find(|c| &c.id == conn_id)
                            .ok_or_else(|| {
                                anyhow::anyhow!("the cloud connection no longer exists")
                            })?;
                        let object = format!("{}{rel}", cloud_prefix(key));
                        fetch_object_to_temp(conn, &object, &rel, &settings)
                    }
                }
            };
            let item = match fetch(&ra).and_then(|left| Ok((left, fetch(&rb)?))) {
                Ok((left, right)) => CloudOpenResult::PairReady { left, right, label },
                Err(e) => {
                    CloudOpenResult::Failed(format!("{} {rel}: {e:#}", t("cloud.open_failed")))
                }
            };
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Open `left` in a new tab; `apply_loaded_table` then compares it with
    /// `right` once that very file has landed (a big file loads in the
    /// background, so the compare cannot simply follow the call).
    pub(crate) fn start_pair_compare(&mut self, left: PathBuf, right: PathBuf, label: String) {
        self.pending_pair_compare = Some((left.clone(), right));
        self.load_file_in_new_tab(left);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.custom_tab_label = Some(label);
        }
    }
}

#[cfg(test)]
#[path = "folder_compare_tests.rs"]
mod tests;
