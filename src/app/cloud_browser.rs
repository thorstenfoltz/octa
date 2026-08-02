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

/// From a flat cloud object listing, keep only real files (not sub-prefixes)
/// whose lowercased extension is one the reader registry supports. Returns
/// `(key, name)` pairs in listing order. Filtering here avoids downloading
/// obviously-irrelevant blobs before the Union dialog even reads them.
pub(crate) fn data_objects(
    entries: &[ObjectEntry],
    allowed_exts: &HashSet<String>,
) -> Vec<(String, String)> {
    entries
        .iter()
        .filter(|e| !e.is_prefix)
        .filter(|e| {
            std::path::Path::new(&e.name)
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| allowed_exts.contains(&x.to_ascii_lowercase()))
                .unwrap_or(false)
        })
        .map(|e| (e.key.clone(), e.name.clone()))
        .collect()
}

/// (connection id, prefix) key into the listings cache. `prefix == ""` is a
/// connection's bucket root.
pub(crate) type ConnPrefix = (String, String);

/// Map a browser node key to the connection+key to operate on.
///
/// For an account-level connection, node keys are bucket-qualified
/// ("<bucket>/<subkey>"); this returns a clone bound to <bucket> (account_level
/// cleared) and the bucket-relative <subkey>. For a normal connection it is a
/// pass-through.
pub(crate) fn bind_bucket(
    conn: &octa::cloud::CloudConnection,
    key: &str,
) -> (octa::cloud::CloudConnection, String) {
    if conn.account_level {
        let (bucket, sub) = key.split_once('/').unwrap_or((key, ""));
        let mut c = conn.clone();
        c.bucket = bucket.to_string();
        c.account_level = false;
        (c, sub.to_string())
    } else {
        (conn.clone(), key.to_string())
    }
}

/// The key a connection's tree roots at: its configured `prefix` (confining
/// browsing to that folder), or "" for a whole-bucket connection.
pub(crate) fn root_prefix(conn: &octa::cloud::CloudConnection) -> String {
    conn.prefix.clone().unwrap_or_default()
}

/// Download one cloud object into a temp file and return its path. Runs on a
/// worker thread (it does network IO and, for S3 SSO, shells out to the AWS
/// CLI), so it takes owned/borrowed data rather than `&OctaApp`.
///
/// The temp file keeps the object's extension so the format registry routes it
/// to the right reader, and the handle is leaked (`tmp.keep()`) so streaming
/// readers can keep reading from disk after this returns. The OS clears /tmp on
/// reboot, the same trick the archive viewer uses.
fn fetch_object_to_temp(
    conn: &octa::cloud::CloudConnection,
    key: &str,
    name: &str,
    settings: &octa::ui::settings::AppSettings,
) -> anyhow::Result<PathBuf> {
    let (bconn, real_key) = bind_bucket(conn, key);
    let creds = resolve_creds(&bconn, settings);
    let provider = cloud::build_provider(&bconn, &creds)?;
    let bytes = provider.get(&real_key)?;
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let tmp = tempfile::Builder::new()
        .prefix("octa-cloud-")
        .suffix(&format!(".{ext}"))
        .tempfile()?;
    tmp.as_file().write_all(&bytes)?;
    let path = tmp.path().to_path_buf();
    let _ = tmp.keep();
    Ok(path)
}

/// Cached state of one expanded node's listing.
pub(crate) enum ListState {
    Loading,
    Ready(Vec<ObjectEntry>),
    Error(String),
}

/// How the browser orders files within a folder. Folders are always listed
/// first, by name; this only affects the file entries. Session-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CloudSort {
    #[default]
    NameAsc,
    NameDesc,
    ModifiedNewest,
    ModifiedOldest,
    SizeLargest,
    SizeSmallest,
}

/// Order a folder's entries for display: folders first (by name), then files
/// by the chosen key. Returns references (no clone of the cached entries).
pub(crate) fn sorted_entries(entries: &[ObjectEntry], sort: CloudSort) -> Vec<&ObjectEntry> {
    use std::cmp::Ordering;
    let mut v: Vec<&ObjectEntry> = entries.iter().collect();
    v.sort_by(|a, b| match (a.is_prefix, b.is_prefix) {
        (true, true) => a.name.cmp(&b.name),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        // Option<T> orders None < Some, so files missing a size/date sort to
        // the "smallest/oldest" end, which is the sensible place for them.
        (false, false) => match sort {
            CloudSort::NameAsc => a.name.cmp(&b.name),
            CloudSort::NameDesc => b.name.cmp(&a.name),
            CloudSort::ModifiedNewest => b.modified.cmp(&a.modified),
            CloudSort::ModifiedOldest => a.modified.cmp(&b.modified),
            CloudSort::SizeLargest => b.size.cmp(&a.size),
            CloudSort::SizeSmallest => a.size.cmp(&b.size),
        },
    });
    v
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
    /// Several objects were downloaded for a Union (the sidebar's "Union
    /// selected files..."). `skipped` counts the ones that could not be read.
    UnionReady {
        paths: Vec<PathBuf>,
        skipped: usize,
    },
    /// A finished recursive inventory listing ("List contents as table...").
    InventoryReady {
        /// Boxed: a `DataTable` inline would dwarf the other variants.
        table: Box<octa::data::DataTable>,
        label: String,
        truncated: bool,
    },
    Failed(String),
}

/// Recursive-inventory object cap: a data-lake bucket can hold millions of
/// keys; past this the listing stops and the tab shows a truncation notice.
pub(crate) const INVENTORY_CAP: usize = 100_000;

/// One cloud object the user has ticked in the sidebar for a batch action.
/// Carries the name as well as the key because the download needs the file
/// extension to route the temp file to the right reader.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct CloudSelection {
    pub(crate) conn_id: String,
    pub(crate) key: String,
    pub(crate) name: String,
}

pub(crate) struct CloudBrowserState {
    /// Whether the sidebar's cloud section is shown.
    pub(crate) visible: bool,
    /// Objects Ctrl-clicked for a batch action (Union). Mirrors the directory
    /// tree's `selected`: a plain click still just opens the file.
    pub(crate) selected: HashSet<CloudSelection>,
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
    /// How files are ordered in every folder listing (session-only).
    pub(crate) sort: CloudSort,
}

impl Default for CloudBrowserState {
    fn default() -> Self {
        Self {
            visible: false,
            selected: HashSet::new(),
            listings: Arc::new(Mutex::new(HashMap::new())),
            expanded: HashSet::new(),
            pending_open: Arc::new(Mutex::new(Vec::new())),
            sign_in_status: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(None)),
            cli_cache: HashMap::new(),
            secret_cache: HashMap::new(),
            sign_out_confirm: None,
            sort: CloudSort::default(),
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

    pub(crate) fn find_cloud_conn(&self, conn_id: &str) -> Option<octa::cloud::CloudConnection> {
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
        let root = self
            .find_cloud_conn(&conn_id)
            .map(|c| root_prefix(&c))
            .unwrap_or_default();
        self.cloud_browser
            .expanded
            .insert((conn_id.clone(), root.clone()));
        self.start_cloud_list(ctx, conn_id, root);
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
                if conn.account_level {
                    if prefix.is_empty() {
                        // Root of an account-level connection: list buckets.
                        let buckets = cloud::list_account_buckets(&conn)?;
                        return Ok(buckets
                            .into_iter()
                            .map(|b| ObjectEntry {
                                name: b.clone(),
                                key: format!("{b}/"),
                                is_prefix: true,
                                size: None,
                                modified: None,
                                etag: None,
                                version: None,
                            })
                            .collect());
                    }
                    // Inside a bucket: bind a provider to it, list the relative
                    // subkey, then re-qualify keys with the bucket so the tree's
                    // child node keys remain "<bucket>/<subkey>".
                    let (bconn, sub) = bind_bucket(&conn, &prefix);
                    let bucket = bconn.bucket.clone();
                    let creds = resolve_creds(&bconn, &settings);
                    let provider = cloud::build_provider(&bconn, &creds)?;
                    let entries = provider.list(&sub)?;
                    return Ok(entries
                        .into_iter()
                        .map(|mut e| {
                            e.key = format!("{bucket}/{}", e.key);
                            e
                        })
                        .collect());
                }
                // ponytail: account-level browse degrades to an error message when the CLI can't enumerate; bucket-scoped connections cover the no-CLI case.
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
        // Account-level connections have an empty `bucket`; the `key` already
        // carries the bucket as its first segment, so don't double it up.
        let label = if conn.account_level {
            format!("{name} @ {}://{}", conn.kind.scheme(), key)
        } else {
            format!("{name} @ {}://{}/{}", conn.kind.scheme(), conn.bucket, key)
        };
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.status_message = Some((
            format!("{} {name}", octa::i18n::t("cloud.opening")),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let result = fetch_object_to_temp(&conn, &key, &name, &settings);
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

    /// Recursively list everything under (conn, prefix) into a detached
    /// inventory tab. The listing (network, possibly a CLI shell-out for
    /// credentials) runs on a worker thread; `drain_cloud_pending_open`
    /// opens the tab on the main thread.
    pub(crate) fn cloud_inventory(&mut self, ctx: &egui::Context, conn_id: String, prefix: String) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        // An account-level root has no bucket to list yet; ask the user to
        // run the inventory on a bucket (or deeper) instead.
        if conn.account_level && prefix.is_empty() {
            self.status_message = Some((
                octa::i18n::t("inventory.account_root"),
                std::time::Instant::now(),
            ));
            return;
        }
        let label = if conn.account_level {
            format!("{}://{}", conn.kind.scheme(), prefix.trim_end_matches('/'))
        } else if prefix.is_empty() {
            format!("{}://{}", conn.kind.scheme(), conn.bucket)
        } else {
            format!(
                "{}://{}/{}",
                conn.kind.scheme(),
                conn.bucket,
                prefix.trim_end_matches('/')
            )
        };
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.status_message = Some((
            format!("{} {label}", octa::i18n::t("inventory.listing")),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<(Vec<ObjectEntry>, bool)> {
                if conn.account_level {
                    // Bind the bucket named by the prefix's first segment and
                    // re-qualify keys so paths read "<bucket>/<key>".
                    let (bconn, sub) = bind_bucket(&conn, &prefix);
                    let bucket = bconn.bucket.clone();
                    let creds = resolve_creds(&bconn, &settings);
                    let provider = cloud::build_provider(&bconn, &creds)?;
                    let (entries, truncated) = provider.list_recursive(&sub, INVENTORY_CAP)?;
                    return Ok((
                        entries
                            .into_iter()
                            .map(|mut e| {
                                e.key = format!("{bucket}/{}", e.key);
                                e
                            })
                            .collect(),
                        truncated,
                    ));
                }
                let creds = resolve_creds(&conn, &settings);
                let provider = cloud::build_provider(&conn, &creds)?;
                provider.list_recursive(&prefix, INVENTORY_CAP)
            })();
            let item = match result {
                Ok((entries, truncated)) => CloudOpenResult::InventoryReady {
                    table: Box::new(octa::data::inventory::build_inventory_table(&entries)),
                    label,
                    truncated,
                },
                Err(e) => CloudOpenResult::Failed(format!(
                    "{} {label}: {e:#}",
                    octa::i18n::t("cloud.open_failed")
                )),
            };
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Union every readable table under a cloud folder into one table, the same
    /// way the sidebar's "Union selected files..." does, but without the user
    /// ticking each file. `recursive` also descends into subfolders.
    ///
    /// Listing + downloading run on one worker thread; the main thread picks up
    /// the temp paths in `drain_cloud_pending_open` and hands them to
    /// `open_union_for_files` (`CloudOpenResult::UnionReady`).
    pub(crate) fn cloud_union_folder(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        prefix: String,
        recursive: bool,
    ) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        if conn.account_level && prefix.is_empty() {
            // No bucket chosen yet: same guard as the inventory action.
            self.status_message = Some((
                octa::i18n::t("inventory.account_root"),
                std::time::Instant::now(),
            ));
            return;
        }
        // Extension allowlist, computed on the main thread (the registry lives
        // on `self`; the worker cannot borrow it).
        let allowed_exts: HashSet<String> = self
            .registry
            .all_extensions()
            .into_iter()
            .map(|e| e.to_ascii_lowercase())
            .collect();
        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        // Total is unknown until the listing returns, so start at 0 (= no
        // counter yet) and let the worker fill it in.
        let progress =
            super::state::UnionProgress::new(&octa::i18n::t("union_tree.folder_listing"), 0);
        let (label, done, total) = (
            progress.label.clone(),
            progress.done.clone(),
            progress.total.clone(),
        );
        self.union_progress = Some(progress);
        std::thread::spawn(move || {
            let item = (|| -> CloudOpenResult {
                // 1. List the folder (recursive or one level), then filter to
                //    readable files.
                let list_result = (|| -> anyhow::Result<Vec<ObjectEntry>> {
                    if conn.account_level {
                        let (bconn, sub) = bind_bucket(&conn, &prefix);
                        let bucket = bconn.bucket.clone();
                        let creds = resolve_creds(&bconn, &settings);
                        let provider = cloud::build_provider(&bconn, &creds)?;
                        let entries = if recursive {
                            provider.list_recursive(&sub, INVENTORY_CAP)?.0
                        } else {
                            provider.list(&sub)?
                        };
                        Ok(entries
                            .into_iter()
                            .map(|mut e| {
                                e.key = format!("{bucket}/{}", e.key);
                                e
                            })
                            .collect())
                    } else {
                        let creds = resolve_creds(&conn, &settings);
                        let provider = cloud::build_provider(&conn, &creds)?;
                        if recursive {
                            Ok(provider.list_recursive(&prefix, INVENTORY_CAP)?.0)
                        } else {
                            provider.list(&prefix)
                        }
                    }
                })();
                let entries = match list_result {
                    Ok(e) => e,
                    Err(e) => {
                        return CloudOpenResult::Failed(format!(
                            "{} {e:#}",
                            octa::i18n::t("cloud.open_failed")
                        ));
                    }
                };
                let mut files = data_objects(&entries, &allowed_exts);
                if files.len() < 2 {
                    return CloudOpenResult::Failed(octa::i18n::t("union.need_two"));
                }
                // 2. Cap, then download each to a temp. The Union dialog reads
                //    every one of them fully into memory, so an uncapped data
                //    lake would OOM. "Unlimited" resolves to `usize::MAX`,
                //    i.e. no truncation at all.
                let cap = settings.folder_union_cap();
                let mut skipped = files.len().saturating_sub(cap);
                files.truncate(cap);
                // Listing done: switch the status-bar spinner over to the
                // download phase, now that the count is known.
                if let Ok(mut l) = label.lock() {
                    *l = octa::i18n::t("cloud.union_downloading");
                }
                total.store(files.len(), std::sync::atomic::Ordering::Relaxed);
                let mut paths = Vec::new();
                for (key, name) in &files {
                    match fetch_object_to_temp(&conn, key, name, &settings) {
                        Ok(p) => paths.push(p),
                        Err(_) => skipped += 1,
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                if paths.len() < 2 {
                    CloudOpenResult::Failed(octa::i18n::t("union.need_two"))
                } else {
                    CloudOpenResult::UnionReady { paths, skipped }
                }
            })();
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Download every cloud object the user has selected in the sidebar and open
    /// the Union dialog over them, the same way "Union selected files..." works
    /// in the local directory tree.
    ///
    /// The downloads run on one worker thread (the network calls must not block
    /// the UI); the main thread picks the temp paths up in
    /// `drain_cloud_pending_open` and hands them to `open_union_for_files`.
    pub(crate) fn union_cloud_selection(&mut self, ctx: &egui::Context) {
        let selection: Vec<CloudSelection> = self.cloud_browser.selected.iter().cloned().collect();
        if selection.len() < 2 {
            self.status_message =
                Some((octa::i18n::t("union.need_two"), std::time::Instant::now()));
            return;
        }
        // Resolve each selection against its connection here, on the main
        // thread: `find_cloud_conn` borrows self, and the worker cannot.
        let mut jobs: Vec<(octa::cloud::CloudConnection, CloudSelection)> = Vec::new();
        for sel in selection {
            if let Some(conn) = self.find_cloud_conn(&sel.conn_id) {
                jobs.push((conn, sel));
            }
        }
        if jobs.len() < 2 {
            self.status_message =
                Some((octa::i18n::t("union.need_two"), std::time::Instant::now()));
            return;
        }

        let settings = self.settings.clone();
        let pending = self.cloud_browser.pending_open.clone();
        let ctx = ctx.clone();
        let progress =
            super::state::UnionProgress::new(&octa::i18n::t("cloud.union_downloading"), jobs.len());
        let done = progress.done.clone();
        self.union_progress = Some(progress);
        std::thread::spawn(move || {
            let mut paths = Vec::new();
            let mut failed = Vec::new();
            for (conn, sel) in &jobs {
                match fetch_object_to_temp(conn, &sel.key, &sel.name, &settings) {
                    Ok(p) => paths.push(p),
                    Err(e) => failed.push(format!("{}: {e:#}", sel.name)),
                }
                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let item = if paths.len() < 2 {
                CloudOpenResult::Failed(format!(
                    "{} {}",
                    octa::i18n::t("cloud.union_failed"),
                    failed.join("; ")
                ))
            } else {
                CloudOpenResult::UnionReady {
                    paths,
                    skipped: failed.len(),
                }
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
            let root = self
                .find_cloud_conn(&id)
                .map(|c| root_prefix(&c))
                .unwrap_or_default();
            let root_open = self
                .cloud_browser
                .expanded
                .contains(&(id.clone(), root.clone()));
            if root_open {
                self.start_cloud_list(ctx, id.clone(), root);
            }
            self.status_message =
                Some((octa::i18n::t("cloud.signed_in"), std::time::Instant::now()));
        }
    }

    /// Upload a cloud-opened tab's freshly-saved temp file back to its object.
    /// Caller (`save_tab`) gates on the connection's `allow_writes` and on the local
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
        let url = if conn.account_level {
            format!("{}://{}", conn.kind.scheme(), origin.key)
        } else {
            format!("{}://{}/{}", conn.kind.scheme(), conn.bucket, origin.key)
        };
        self.status_message = Some((
            format!("{} {url}", octa::i18n::t("cloud.uploading")),
            std::time::Instant::now(),
        ));
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                let (bconn, real_key) = bind_bucket(&conn, &origin.key);
                let creds = resolve_creds(&bconn, &settings);
                let provider = cloud::build_provider(&bconn, &creds)?;
                provider.put(&real_key, bytes)
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
                CloudOpenResult::UnionReady { paths, skipped } => {
                    // Download phase over; `open_union_for_files` installs its
                    // own read-phase progress right after, so the spinner runs
                    // continuously from download to dialog.
                    self.union_progress = None;
                    if skipped > 0 {
                        self.status_message = Some((
                            format!("{} {skipped}", octa::i18n::t("union_tree.skipped")),
                            std::time::Instant::now(),
                        ));
                    }
                    // Same dialog, same reconciliation plan as the local
                    // directory tree: the files just came from a bucket.
                    self.open_union_for_files(paths);
                }
                CloudOpenResult::InventoryReady {
                    table,
                    label,
                    truncated,
                } => {
                    let rows = table.row_count();
                    let mut new_tab =
                        super::state::TabState::new(self.settings.default_search_mode);
                    new_tab.table = *table;
                    new_tab.custom_tab_label = Some(format!(
                        "{} - {label}",
                        octa::i18n::t("inventory.tab_label")
                    ));
                    if truncated {
                        new_tab.parse_error_banner =
                            Some(octa::i18n::t("inventory.truncated").replace(
                                "{n}",
                                &octa::ui::status_bar::format_number(INVENTORY_CAP),
                            ));
                    }
                    self.push_result_tab(new_tab);
                    self.status_message = Some((
                        format!("{} {rows} ({label})", octa::i18n::t("inventory.done")),
                        std::time::Instant::now(),
                    ));
                }
                CloudOpenResult::Failed(msg) => {
                    // Covers a failed union listing/download too, so the
                    // spinner never outlives its job.
                    self.union_progress = None;
                    self.status_message = Some((msg, std::time::Instant::now()));
                }
            }
        }
    }
}

#[cfg(test)]
mod union_folder_tests {
    use super::data_objects;
    use octa::cloud::ObjectEntry;
    use std::collections::HashSet;

    fn entry(key: &str, name: &str, is_prefix: bool) -> ObjectEntry {
        ObjectEntry {
            name: name.to_string(),
            key: key.to_string(),
            is_prefix,
            size: None,
            modified: None,
            etag: None,
            version: None,
        }
    }

    #[test]
    fn keeps_only_supported_files_not_prefixes() {
        let entries = vec![
            entry("data/a.csv", "a.csv", false),
            entry("data/sub/", "sub", true), // a folder: excluded
            entry("data/notes.txt", "notes.txt", false), // unsupported ext
            entry("data/b.parquet", "b.parquet", false),
            entry("data/noext", "noext", false), // no extension
        ];
        let allowed: HashSet<String> = ["csv", "parquet"].into_iter().map(String::from).collect();
        let got = data_objects(&entries, &allowed);
        assert_eq!(
            got,
            vec![
                ("data/a.csv".to_string(), "a.csv".to_string()),
                ("data/b.parquet".to_string(), "b.parquet".to_string()),
            ]
        );
    }
}

#[cfg(test)]
mod sort_tests {
    use super::{CloudSort, sorted_entries};
    use chrono::{TimeZone, Utc};
    use octa::cloud::ObjectEntry;

    fn file(name: &str, size: u64, day: u32) -> ObjectEntry {
        ObjectEntry {
            name: name.to_string(),
            key: name.to_string(),
            is_prefix: false,
            size: Some(size),
            modified: Some(Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()),
            etag: None,
            version: None,
        }
    }
    fn folder(name: &str) -> ObjectEntry {
        ObjectEntry {
            name: name.to_string(),
            key: format!("{name}/"),
            is_prefix: true,
            size: None,
            modified: None,
            etag: None,
            version: None,
        }
    }

    #[test]
    fn folders_first_then_files_by_key() {
        let entries = vec![file("b.csv", 10, 2), folder("zzz"), file("a.csv", 30, 1)];
        // Size largest: folder still first, then a.csv (30) before b.csv (10).
        let by_size = sorted_entries(&entries, CloudSort::SizeLargest);
        assert_eq!(by_size[0].name, "zzz");
        assert_eq!(by_size[1].name, "a.csv");
        assert_eq!(by_size[2].name, "b.csv");
        // Newest first: day 2 (b.csv) before day 1 (a.csv).
        let by_date = sorted_entries(&entries, CloudSort::ModifiedNewest);
        assert_eq!(by_date[0].name, "zzz");
        assert_eq!(by_date[1].name, "b.csv");
        assert_eq!(by_date[2].name, "a.csv");
    }
}

#[cfg(test)]
mod bind_bucket_tests {
    use super::bind_bucket;
    use octa::cloud::{CloudConnection, CloudKind};

    fn account_level(kind: CloudKind) -> CloudConnection {
        let mut c = CloudConnection::ephemeral_s3("");
        c.kind = kind;
        c.bucket = String::new();
        c.account_level = true;
        c
    }

    #[test]
    fn account_level_keys_carry_the_bucket() {
        // The tree qualifies an account-level connection's keys as
        // `<bucket>/<key>`, and its `bucket` field is empty. Anything that
        // builds a provider from one has to split that back out first, or it
        // addresses bucket "" with a key that still names the bucket.
        let conn = account_level(CloudKind::Gcs);
        let (bound, key) = bind_bucket(&conn, "my-bucket/data/file.parquet");
        assert_eq!(bound.bucket, "my-bucket");
        assert!(!bound.account_level, "bound connection is bucket-scoped");
        assert_eq!(key, "data/file.parquet");

        // A bucket root (no sub-key) still binds.
        let (bound, key) = bind_bucket(&conn, "my-bucket");
        assert_eq!(bound.bucket, "my-bucket");
        assert_eq!(key, "");
    }

    #[test]
    fn two_buckets_of_one_account_level_connection_are_different_stores() {
        // The copy path picks the server-side lane when both ends share a
        // bucket. Comparing the *unbound* connections would see two empty
        // buckets and wrongly take that lane across genuinely different
        // buckets.
        let conn = account_level(CloudKind::Gcs);
        let (a, _) = bind_bucket(&conn, "bucket-a/x.csv");
        let (b, _) = bind_bucket(&conn, "bucket-b/x.csv");
        assert_ne!(a.bucket, b.bucket);
        assert_eq!(conn.bucket, "", "the unbound one would have compared equal");
    }

    #[test]
    fn a_normal_connection_passes_through() {
        let mut c = CloudConnection::ephemeral_s3("fixed-bucket");
        c.account_level = false;
        let (bound, key) = bind_bucket(&c, "data/file.csv");
        assert_eq!(bound.bucket, "fixed-bucket");
        assert_eq!(key, "data/file.csv");
    }
}
