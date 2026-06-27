//! Sidebar cloud-storage browser: list folders/files, download-and-open, and
//! per-connection browser sign-in, all on background workers so the egui
//! update thread never blocks on the network or a CLI subprocess.
//!
//! Mirrors the worker idiom in [`super::multi_search`]: shared `Arc<Mutex<_>>`
//! state the worker writes and the panel reads each frame, plus a per-frame
//! drain of finished downloads (`drain_cloud_pending_open`, called from the
//! update loop). Credentials resolve *inside* the worker because the S3 chain
//! can shell out to the AWS CLI (`aws configure export-credentials`), which
//! blocks.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::cloud::{self, ObjectEntry};
use octa::ui::settings::cloud_secrets::resolve_creds;

use super::state::{CloudOrigin, OctaApp};

/// (connection id, prefix) key into the listings cache. `prefix == ""` is a
/// connection's bucket root.
pub(crate) type ConnPrefix = (String, String);

/// Cached state of one expanded node's listing.
pub(crate) enum ListState {
    Loading,
    Ready(Vec<ObjectEntry>),
    Error(String),
}

/// Per-connection sign-in progress, surfaced next to the connection row.
#[derive(Clone)]
pub(crate) enum SignInState {
    InProgress,
    Done,
    Failed(String),
}

/// A finished download waiting to be opened on the main thread (workers must
/// not touch tabs/egui), or a download that failed.
pub(crate) enum CloudOpenResult {
    Ready {
        /// Temp file the object bytes were written to (leaked; OS cleans /tmp).
        path: PathBuf,
        /// Display label for the new tab (`name @ scheme://bucket/key`).
        label: String,
        conn_id: String,
        key: String,
    },
    Failed(String),
}

pub(crate) struct CloudBrowserState {
    /// Whether the sidebar's cloud section is shown.
    pub(crate) visible: bool,
    /// Cached per-node listings, written by list workers.
    pub(crate) listings: Arc<Mutex<HashMap<ConnPrefix, ListState>>>,
    /// Which nodes the user has expanded (connection roots + sub-prefixes).
    pub(crate) expanded: HashSet<ConnPrefix>,
    /// Finished/failed downloads, drained on the main thread per frame.
    pub(crate) pending_open: Arc<Mutex<Vec<CloudOpenResult>>>,
    /// Per-connection sign-in status.
    pub(crate) sign_in_status: Arc<Mutex<HashMap<String, SignInState>>>,
    /// One-shot status message from a background upload (save-back), drained
    /// into the status bar per frame.
    pub(crate) status: Arc<Mutex<Option<String>>>,
    /// Memoised "is this cloud's CLI on PATH" per kind. `cli_available` shells
    /// out to `which`/`where`, so we compute it once per session instead of
    /// every repaint. (Install a CLI mid-session -> reopen Octa to pick it up.)
    pub(crate) cli_cache: HashMap<octa::cloud::CloudKind, bool>,
    /// Memoised "does this connection have a stored secret" per connection id.
    /// The lookup reads the OS keyring, so it is computed once and refreshed
    /// when the cloud section is (re)opened or after a sign-out. Drives the
    /// Sign in vs Sign out control + the status chip.
    pub(crate) secret_cache: HashMap<String, bool>,
    /// Connection id whose "Sign out (clear saved keys)" is armed for an
    /// explicit confirm click. Mirrors the Settings Clear-secret guard.
    pub(crate) sign_out_confirm: Option<String>,
}

impl Default for CloudBrowserState {
    fn default() -> Self {
        Self {
            visible: false,
            listings: Arc::new(Mutex::new(HashMap::new())),
            expanded: HashSet::new(),
            pending_open: Arc::new(Mutex::new(Vec::new())),
            sign_in_status: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(None)),
            cli_cache: HashMap::new(),
            secret_cache: HashMap::new(),
            sign_out_confirm: None,
        }
    }
}

impl OctaApp {
    /// Toggle the sidebar cloud-browser section. Opening it drops the
    /// secret-presence cache so any credential added/removed in Settings since
    /// last time is reflected.
    pub(crate) fn toggle_cloud_browser(&mut self) {
        self.cloud_browser.visible = !self.cloud_browser.visible;
        if self.cloud_browser.visible {
            self.cloud_browser.secret_cache.clear();
            self.cloud_browser.sign_out_confirm = None;
        }
    }

    /// Arm / disarm the "Sign out (clear saved keys)" confirm for a connection.
    pub(crate) fn arm_cloud_sign_out(&mut self, conn_id: Option<String>) {
        self.cloud_browser.sign_out_confirm = conn_id;
    }

    /// Clear a connection's saved secret (the sidebar "Sign out"). Local only:
    /// removes the keyring / plaintext secret, refreshes the cache, and drops
    /// the connection's cached listings so the next expand re-checks access.
    pub(crate) fn cloud_sign_out(&mut self, conn_id: String) {
        octa::ui::settings::cloud_secrets::delete_cloud_secret(&conn_id, &mut self.settings);
        self.settings.save();
        self.cloud_browser
            .secret_cache
            .insert(conn_id.clone(), false);
        if let Ok(mut m) = self.cloud_browser.listings.lock() {
            m.retain(|(c, _), _| c != &conn_id);
        }
        self.cloud_browser.sign_out_confirm = None;
        self.status_message = Some((octa::i18n::t("cloud.signed_out"), std::time::Instant::now()));
    }

    fn find_cloud_conn(&self, conn_id: &str) -> Option<octa::cloud::CloudConnection> {
        self.settings
            .cloud_connections
            .iter()
            .find(|c| c.id == conn_id)
            .cloned()
    }

    /// Expand (and lazily list) or collapse a cloud node.
    pub(crate) fn toggle_cloud_node(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        prefix: String,
    ) {
        let key = (conn_id.clone(), prefix.clone());
        if self.cloud_browser.expanded.contains(&key) {
            self.cloud_browser.expanded.remove(&key);
            return;
        }
        self.cloud_browser.expanded.insert(key.clone());
        let cached = self
            .cloud_browser
            .listings
            .lock()
            .map(|m| m.contains_key(&key))
            .unwrap_or(false);
        if !cached {
            self.start_cloud_list(ctx, conn_id, prefix);
        }
    }

    /// Drop a connection's cached listings, collapse its sub-folders, and
    /// re-list its root (Refresh button). Used after a sign-in or when the
    /// bucket has changed under us.
    pub(crate) fn refresh_cloud_conn(&mut self, ctx: &egui::Context, conn_id: String) {
        if let Ok(mut m) = self.cloud_browser.listings.lock() {
            m.retain(|(c, _), _| c != &conn_id);
        }
        self.cloud_browser
            .expanded
            .retain(|(c, p)| c != &conn_id || p.is_empty());
        self.cloud_browser
            .expanded
            .insert((conn_id.clone(), String::new()));
        self.start_cloud_list(ctx, conn_id, String::new());
    }

    fn start_cloud_list(&mut self, ctx: &egui::Context, conn_id: String, prefix: String) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        let settings = self.settings.clone();
        let key = (conn_id, prefix.clone());
        let listings = self.cloud_browser.listings.clone();
        if let Ok(mut m) = listings.lock() {
            m.insert(key.clone(), ListState::Loading);
        }
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<ObjectEntry>> {
                let creds = resolve_creds(&conn, &settings);
                let provider = cloud::build_provider(&conn, &creds)?;
                provider.list(&prefix)
            })();
            let state = match result {
                Ok(entries) => ListState::Ready(entries),
                Err(e) => ListState::Error(format!("{e:#}")),
            };
            if let Ok(mut m) = listings.lock() {
                m.insert(key, state);
            }
            ctx.request_repaint();
        });
    }

    /// Download a cloud object to a temp file and queue it for opening.
    pub(crate) fn open_cloud_object(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        key: String,
        name: String,
    ) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        let label = format!("{name} @ {}://{}/{}", conn.kind.scheme(), conn.bucket, key);
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.status_message = Some((
            format!("{} {name}", octa::i18n::t("cloud.opening")),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<PathBuf> {
                let creds = resolve_creds(&conn, &settings);
                let provider = cloud::build_provider(&conn, &creds)?;
                let bytes = provider.get(&key)?;
                // Suffix from the object name so the format registry routes the
                // temp file to the right reader.
                let ext = std::path::Path::new(&name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
                let suffix = format!(".{ext}");
                let tmp = tempfile::Builder::new()
                    .prefix("octa-cloud-")
                    .suffix(&suffix)
                    .tempfile()?;
                tmp.as_file().write_all(&bytes)?;
                let path = tmp.path().to_path_buf();
                // Leak the handle so streaming readers can keep reading from
                // disk past this call. OS cleans /tmp on reboot (same trick the
                // archive viewer uses).
                let _ = tmp.keep();
                Ok(path)
            })();
            let item = match result {
                Ok(path) => CloudOpenResult::Ready {
                    path,
                    label,
                    conn_id,
                    key,
                },
                Err(e) => CloudOpenResult::Failed(format!(
                    "{} {name}: {e:#}",
                    octa::i18n::t("cloud.open_failed")
                )),
            };
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Run browser sign-in for a connection (shells out to its cloud CLI). On
    /// success the worker only records `Done`; the main thread
    /// (`drain_cloud_sign_ins`) re-lists with the fresh token, since re-listing
    /// needs `&mut self`.
    pub(crate) fn cloud_sign_in(&mut self, ctx: &egui::Context, conn_id: String) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        let kind = conn.kind;
        let profile = conn.profile.clone();
        let status = self.cloud_browser.sign_in_status.clone();
        let ctx = ctx.clone();
        if let Ok(mut m) = status.lock() {
            m.insert(conn_id.clone(), SignInState::InProgress);
        }
        std::thread::spawn(move || {
            let result = cloud::interactive_login(kind, profile.as_deref());
            let state = match result {
                Ok(()) => SignInState::Done,
                Err(e) => SignInState::Failed(format!("{e:#}")),
            };
            if let Ok(mut m) = status.lock() {
                m.insert(conn_id, state);
            }
            ctx.request_repaint();
        });
    }

    /// Finish any just-completed sign-ins on the main thread: drop the
    /// connection's cached listings (clearing a stale auth error) and re-list
    /// its root if it is open, so the freshly-authenticated content appears
    /// without the user clicking Refresh. Called once per frame.
    pub(crate) fn drain_cloud_sign_ins(&mut self, ctx: &egui::Context) {
        let done: Vec<String> = {
            let Ok(mut m) = self.cloud_browser.sign_in_status.lock() else {
                return;
            };
            let done: Vec<String> = m
                .iter()
                .filter(|(_, s)| matches!(s, SignInState::Done))
                .map(|(k, _)| k.clone())
                .collect();
            for k in &done {
                m.remove(k);
            }
            done
        };
        for id in done {
            if let Ok(mut l) = self.cloud_browser.listings.lock() {
                l.retain(|(c, _), _| c != &id);
            }
            // A successful sign-in usually means new credentials work, so the
            // saved-secret picture may have changed too; recompute on next open.
            self.cloud_browser.secret_cache.remove(&id);
            let root_open = self
                .cloud_browser
                .expanded
                .contains(&(id.clone(), String::new()));
            if root_open {
                self.start_cloud_list(ctx, id.clone(), String::new());
            }
            self.status_message =
                Some((octa::i18n::t("cloud.signed_in"), std::time::Instant::now()));
        }
    }

    /// Upload a cloud-opened tab's freshly-saved temp file back to its object.
    /// Caller (`save_tab`) gates on `cloud_writes_enabled` and on the local
    /// save having completed. Runs on a worker; reports via `status`.
    pub(crate) fn upload_cloud_tab(&mut self, tab_idx: usize, local_path: std::path::PathBuf) {
        let Some(origin) = self.tabs[tab_idx].cloud_origin.clone() else {
            return;
        };
        let Some(conn) = self.find_cloud_conn(&origin.conn_id) else {
            return;
        };
        let bytes = match std::fs::read(&local_path) {
            Ok(b) => b,
            Err(e) => {
                self.status_message = Some((
                    format!("{} {e}", octa::i18n::t("cloud.upload_failed")),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let settings = self.settings.clone();
        let status = self.cloud_browser.status.clone();
        let url = format!("{}://{}/{}", conn.kind.scheme(), conn.bucket, origin.key);
        self.status_message = Some((
            format!("{} {url}", octa::i18n::t("cloud.uploading")),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                let creds = resolve_creds(&conn, &settings);
                let provider = cloud::build_provider(&conn, &creds)?;
                provider.put(&origin.key, bytes)
            })();
            let msg = match result {
                Ok(()) => format!("{} {url}", octa::i18n::t("cloud.uploaded")),
                Err(e) => format!("{} {url}: {e:#}", octa::i18n::t("cloud.upload_failed")),
            };
            if let Ok(mut s) = status.lock() {
                *s = Some(msg);
            }
        });
    }

    /// Open any downloads that finished since last frame, and surface any
    /// upload status. Runs on the main thread (touches tabs/egui); called
    /// from the update loop.
    pub(crate) fn drain_cloud_pending_open(&mut self) {
        if let Ok(mut s) = self.cloud_browser.status.lock()
            && let Some(msg) = s.take()
        {
            self.status_message = Some((msg, std::time::Instant::now()));
        }
        let drained: Vec<CloudOpenResult> = {
            let Ok(mut p) = self.cloud_browser.pending_open.lock() else {
                return;
            };
            if p.is_empty() {
                return;
            }
            std::mem::take(&mut *p)
        };
        for item in drained {
            match item {
                CloudOpenResult::Ready {
                    path,
                    label,
                    conn_id,
                    key,
                } => {
                    self.load_file_in_new_tab(path);
                    if let Some(tab) = self.tabs.last_mut() {
                        tab.cloud_origin = Some(CloudOrigin { conn_id, key });
                        tab.custom_tab_label = Some(label);
                    }
                }
                CloudOpenResult::Failed(msg) => {
                    self.status_message = Some((msg, std::time::Instant::now()));
                }
            }
        }
    }
}
