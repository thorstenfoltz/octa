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
use octa::ui::tree_filter::{self, SearchSlot, TreeSearch};

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
pub(crate) fn fetch_object_to_temp(
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

/// Download several cloud objects at once instead of one after another.
///
/// Returns one result per input, **in input order**, so the union still sees
/// the files in the order the folder listed them; a job whose worker died
/// comes back as an error rather than shortening the vec. `done` ticks once
/// per finished object, driving the status-bar progress exactly as the serial
/// loop did.
///
/// Each object resolves its own credentials and builds its own provider, just
/// as the serial version did. That is not redundant: an account-level
/// connection binds its bucket from the key, so two jobs in one batch can
/// legitimately target different buckets and could not share one provider.
fn fetch_objects_parallel(
    jobs: &[(octa::cloud::CloudConnection, String, String)],
    settings: &octa::ui::settings::AppSettings,
    done: &std::sync::atomic::AtomicUsize,
) -> Vec<Result<PathBuf, String>> {
    // How many at once is the user's call (Settings -> Performance -> Cloud
    // union). `run_in_parallel` clamps it to at least one, so a corrupt 0
    // cannot leave the download with no workers.
    let results = run_in_parallel(
        jobs,
        settings.cloud_download_concurrency,
        |(conn, key, name)| {
            let result = fetch_object_to_temp(conn, key, name, settings)
                .map_err(|e| format!("{name}: {e:#}"));
            done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            result
        },
    );
    results
        .into_iter()
        .enumerate()
        .map(|(i, slot)| {
            slot.unwrap_or_else(|| Err(format!("{}: download did not finish", jobs[i].2)))
        })
        .collect()
}

/// Run `work` over every job, at most `concurrency` at a time, returning the
/// results in **input order** rather than completion order.
///
/// The pool shares one cursor instead of giving each worker a fixed slice:
/// cloud objects are of wildly different sizes, so handing every worker an
/// equal COUNT would leave most of them idle while one finished the big
/// files.
///
/// A job whose worker panicked comes back as `None` rather than shortening
/// the vec, so the caller can always line the results up with its inputs.
fn run_in_parallel<T, R>(
    jobs: &[T],
    concurrency: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<Option<R>>
where
    T: Sync,
    R: Send,
{
    use std::sync::atomic::Ordering;

    if jobs.is_empty() {
        return Vec::new();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let collected: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..concurrency.clamp(1, jobs.len()))
            .map(|_| {
                // Move the REFERENCES into each worker, not the values: the
                // cursor and the closure are shared by the whole pool.
                let (next, work) = (&next, &work);
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        // Take the index from `fetch_add` itself. Reading
                        // `next` back afterwards would race: another worker
                        // can claim the next job in between, and the result
                        // would be filed under someone else's index.
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break;
                        };
                        mine.push((index, work(job)));
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });

    let mut slots: Vec<Option<R>> = (0..jobs.len()).map(|_| None).collect();
    for (index, result) in collected {
        if let Some(slot) = slots.get_mut(index) {
            *slot = Some(result);
        }
    }
    slots
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
    /// Objects picked in the SQL workspace's cloud picker, downloaded and
    /// waiting to be registered as workspace tables. `skipped` counts the ones
    /// that could not be read.
    WorkspaceReady {
        /// `(temp file, workspace table name)` in pick order.
        files: Vec<(PathBuf, String)>,
        /// Register the lot as ONE table instead of one table each. The picker
        /// leaves this off: combining is the user's call, not a default.
        combine: bool,
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

/// How many objects one deep search lists before it stops, across every
/// connection it walks. A data lake can hold millions of keys.
const SEARCH_MAX_OBJECTS: usize = 10_000;

/// One object a deep search found.
#[derive(Debug, Clone)]
pub(crate) struct CloudHit {
    pub(crate) conn_id: String,
    pub(crate) conn_name: String,
    /// Full key, bucket-qualified for an account-level connection (the same
    /// shape the tree's node keys have, so `open_cloud_object` takes it).
    pub(crate) key: String,
    pub(crate) name: String,
    /// A folder hit: its key ends with `/`, and clicking it reveals it in
    /// the tree instead of opening it.
    pub(crate) is_folder: bool,
}

/// Whether a deep-search entry matches: by its name, or by its whole path
/// once the query holds a `/` (`2024/sales`).
fn search_matches(name: &str, key: &str, needle: &str) -> bool {
    tree_filter::matches(if needle.contains('/') { key } else { name }, needle)
}

/// Every folder key (ending in `/`) strictly below `start` that the object
/// `keys` sit in, sorted and without repeats.
fn folders_below<'a>(start: &str, keys: impl Iterator<Item = &'a str>) -> Vec<String> {
    let floor = start.trim_end_matches('/').len();
    let mut out = std::collections::BTreeSet::new();
    for key in keys {
        for (i, _) in key.match_indices('/') {
            if i > floor {
                out.insert(key[..=i].to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// The tree nodes to expand so the folder `key` shows opened: the root, then
/// every folder from the top down to `key` itself.
pub(crate) fn reveal_path(root: &str, key: &str) -> Vec<String> {
    let mut out = vec![root.to_string()];
    let floor = root.trim_end_matches('/').len();
    out.extend(
        key.match_indices('/')
            .filter(|(i, _)| *i > floor)
            .map(|(i, _)| key[..=i].to_string()),
    );
    out
}

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
    /// A cloned egui context, stamped on the first frame, so a worker can wake
    /// the UI. The upload worker has no context of its own (`save_tab`, three
    /// calls up, never had one to pass), and without a repaint its result sat
    /// invisible until the user happened to move the mouse.
    pub(crate) repaint: Option<egui::Context>,
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
    /// The search box's text.
    pub(crate) search_query: String,
    /// The deep search, written by its worker.
    pub(crate) search: Arc<Mutex<SearchSlot<CloudHit>>>,
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
            repaint: None,
            cli_cache: HashMap::new(),
            secret_cache: HashMap::new(),
            sign_out_confirm: None,
            sort: CloudSort::default(),
            search_query: String::new(),
            search: Arc::new(Mutex::new(SearchSlot::default())),
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

    /// Populate a cloud node's listing if nobody has yet, leaving the sidebar's
    /// expansion state alone. The SQL workspace's object picker browses through
    /// this so it shares the sidebar's cache and its worker.
    pub(crate) fn ensure_cloud_listing(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        prefix: String,
    ) {
        let key = (conn_id.clone(), prefix.clone());
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

    /// Expand the tree down to the folder `key` (a folder search hit),
    /// listing each level not listed yet. Never collapses anything.
    pub(crate) fn reveal_cloud_folder(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        key: String,
    ) {
        let Some(conn) = self.find_cloud_conn(&conn_id) else {
            return;
        };
        for prefix in reveal_path(&root_prefix(&conn), &key) {
            if !self
                .cloud_browser
                .expanded
                .contains(&(conn_id.clone(), prefix.clone()))
            {
                self.toggle_cloud_node(ctx, conn_id.clone(), prefix);
            }
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

    /// Search every object under the expanded connections for names that
    /// contain the search box's text, on a worker. An account-level
    /// connection is searched inside the buckets the user has opened: walking
    /// every bucket of an account is not a search, it is an inventory.
    pub(crate) fn start_cloud_search(&mut self, ctx: &egui::Context) {
        let query = self.cloud_browser.search_query.clone();
        let Some(needle) = tree_filter::needle(&query) else {
            return;
        };
        // (connection, bucket-qualified start key) pairs to walk.
        let mut roots: Vec<(cloud::CloudConnection, String)> = Vec::new();
        for conn in &self.settings.cloud_connections {
            let root = root_prefix(conn);
            if !self
                .cloud_browser
                .expanded
                .contains(&(conn.id.clone(), root.clone()))
            {
                continue;
            }
            if conn.account_level {
                // Expanded bucket nodes are the one-segment keys "<bucket>/".
                roots.extend(
                    self.cloud_browser
                        .expanded
                        .iter()
                        .filter(|(c, k)| {
                            c == &conn.id && k.ends_with('/') && k.matches('/').count() == 1
                        })
                        .map(|(_, k)| (conn.clone(), k.clone())),
                );
            } else {
                roots.push((conn.clone(), root));
            }
        }
        let slot = self.cloud_browser.search.clone();
        let Ok(mut s) = slot.lock() else {
            return;
        };
        let stop = s.start(&query);
        if roots.is_empty() {
            s.state = TreeSearch::Failed(octa::i18n::t("treesearch.need_expand"));
            return;
        }
        drop(s);
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut hits = Vec::new();
            let mut errors = Vec::new();
            let mut listed = 0usize;
            let mut stopped_at = None;
            for (conn, start) in &roots {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let room = SEARCH_MAX_OBJECTS.saturating_sub(listed);
                if room == 0 {
                    stopped_at = Some(listed);
                    break;
                }
                let walked = (|| -> anyhow::Result<(Vec<ObjectEntry>, bool)> {
                    let (bconn, sub) = bind_bucket(conn, start);
                    let creds = resolve_creds(&bconn, &settings);
                    let provider = cloud::build_provider(&bconn, &creds)?;
                    let (mut entries, truncated) =
                        provider.list_recursive_until(&sub, room, &stop)?;
                    // Keys under an account-level connection carry their
                    // bucket, like the tree's node keys do.
                    if conn.account_level {
                        for e in &mut entries {
                            e.key = format!("{}/{}", bconn.bucket, e.key);
                        }
                    }
                    Ok((entries, truncated))
                })();
                match walked {
                    Ok((entries, truncated)) => {
                        listed += entries.len();
                        let hit = |key: String, is_folder: bool| {
                            let name = key.trim_end_matches('/').rsplit('/').next()?.to_string();
                            search_matches(&name, &key, &needle).then(|| CloudHit {
                                conn_id: conn.id.clone(),
                                conn_name: conn.name.clone(),
                                key,
                                name,
                                is_folder,
                            })
                        };
                        // A recursive listing has no folder entries: the
                        // folders are the key prefixes below the start.
                        let folders = folders_below(start, entries.iter().map(|e| e.key.as_str()));
                        hits.extend(folders.into_iter().filter_map(|k| hit(k, true)));
                        hits.extend(
                            entries
                                .into_iter()
                                .filter(|e| !e.key.ends_with('/'))
                                .filter_map(|e| hit(e.key, false)),
                        );
                        if truncated {
                            stopped_at = Some(listed);
                            break;
                        }
                    }
                    Err(e) => errors.push(format!("{}: {e:#}", conn.name)),
                }
            }
            // A connection that failed does not hide what the others found.
            let state = if hits.is_empty() && !errors.is_empty() {
                TreeSearch::Failed(
                    octa::i18n::t("treesearch.failed").replace("{error}", &errors.join("; ")),
                )
            } else {
                TreeSearch::Done { hits, stopped_at }
            };
            if let Ok(mut s) = slot.lock() {
                s.finish(&stop, state);
            }
            ctx.request_repaint();
        });
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
                let jobs: Vec<_> = files
                    .iter()
                    .map(|(key, name)| (conn.clone(), key.clone(), name.clone()))
                    .collect();
                let mut paths = Vec::new();
                for result in fetch_objects_parallel(&jobs, &settings, &done) {
                    match result {
                        Ok(p) => paths.push(p),
                        Err(_) => skipped += 1,
                    }
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
            let fetches: Vec<_> = jobs
                .iter()
                .map(|(conn, sel)| (conn.clone(), sel.key.clone(), sel.name.clone()))
                .collect();
            let mut paths = Vec::new();
            let mut failed = Vec::new();
            for result in fetch_objects_parallel(&fetches, &settings, &done) {
                match result {
                    Ok(p) => paths.push(p),
                    Err(e) => failed.push(e),
                }
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
        let repaint = self.cloud_browser.repaint.clone();
        std::thread::spawn(move || {
            // Reading the file happens here, not on the UI thread: a saved
            // table is exactly as big as the table, and the window froze for
            // as long as the read took.
            let result = (|| -> anyhow::Result<()> {
                let bytes = std::fs::read(&local_path)?;
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
            // Every other worker in this file wakes the UI; this one did not,
            // so its result waited for an unrelated repaint.
            if let Some(ctx) = repaint {
                ctx.request_repaint();
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
                    // A refresh lands in the tab it came from; anything
                    // else gets a tab of its own.
                    if self.take_reload_slot(&super::refresh::cloud_source(&conn_id, &key)) {
                        self.load_file(path);
                    } else {
                        self.load_file_in_new_tab(path);
                    }
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
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
                CloudOpenResult::WorkspaceReady {
                    files,
                    combine,
                    skipped,
                } => {
                    self.union_progress = None;
                    self.workspace_add_cloud_files(files, combine, skipped);
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

#[cfg(test)]
mod parallel_tests {
    use super::run_in_parallel;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// The union hands the downloaded paths to the column reconciler in the
    /// order the folder listed them, so the pool must return results in INPUT
    /// order even though they finish in whatever order the network allows.
    /// Reversing the work time makes completion order the opposite of input
    /// order, so a pool that returned results as they landed would fail here.
    #[test]
    fn results_come_back_in_input_order_not_completion_order() {
        let jobs: Vec<usize> = (0..24).collect();
        let out = run_in_parallel(&jobs, 8, |n| {
            std::thread::sleep(Duration::from_millis((24 - *n) as u64));
            n * 2
        });
        let got: Vec<usize> = out
            .into_iter()
            .map(|o| o.expect("no worker panicked"))
            .collect();
        assert_eq!(got, (0..24).map(|n| n * 2).collect::<Vec<_>>());
    }

    /// Every job runs exactly once. A cursor that used `load` instead of the
    /// value `fetch_add` returned would double-run some and skip others.
    #[test]
    fn every_job_runs_exactly_once() {
        let jobs: Vec<usize> = (0..200).collect();
        let runs: Vec<AtomicUsize> = (0..200).map(|_| AtomicUsize::new(0)).collect();
        run_in_parallel(&jobs, 8, |n| runs[*n].fetch_add(1, Ordering::SeqCst));
        for (i, count) in runs.iter().enumerate() {
            assert_eq!(
                count.load(Ordering::SeqCst),
                1,
                "job {i} ran the wrong number of times"
            );
        }
    }

    /// The whole point: jobs overlap. And they overlap no more than the limit,
    /// because a burst of unbounded requests is what the cloud providers rate
    /// limit.
    #[test]
    fn jobs_overlap_but_never_exceed_the_limit() {
        let jobs: Vec<usize> = (0..32).collect();
        let inflight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        run_in_parallel(&jobs, 4, |_| {
            let now = inflight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            inflight.fetch_sub(1, Ordering::SeqCst);
        });
        let peak = peak.load(Ordering::SeqCst);
        assert!(
            peak > 1,
            "the pool never ran two jobs at once (peak {peak})"
        );
        assert!(
            peak <= 4,
            "the pool exceeded its concurrency limit (peak {peak})"
        );
    }

    /// Degenerate inputs: no jobs spawns nothing, and fewer jobs than workers
    /// must not spawn idle threads or index past the end.
    #[test]
    fn empty_and_short_job_lists_are_handled() {
        let none: Vec<usize> = Vec::new();
        assert!(run_in_parallel(&none, 8, |n| *n).is_empty());

        let two = vec![7usize, 9];
        let out: Vec<usize> = run_in_parallel(&two, 8, |n| *n)
            .into_iter()
            .map(|o| o.expect("no worker panicked"))
            .collect();
        assert_eq!(out, vec![7, 9]);
    }

    /// A concurrency of zero would otherwise spawn no workers and hang the
    /// download forever; it is clamped to one.
    #[test]
    fn zero_concurrency_still_runs_the_jobs() {
        let jobs = vec![1usize, 2, 3];
        let out: Vec<usize> = run_in_parallel(&jobs, 0, |n| n * 10)
            .into_iter()
            .map(|o| o.expect("no worker panicked"))
            .collect();
        assert_eq!(out, vec![10, 20, 30]);
    }
}

#[cfg(test)]
mod search_tests {
    use super::{folders_below, reveal_path, search_matches};

    #[test]
    fn a_recursive_listing_yields_its_folders_below_the_start_only() {
        let keys = [
            "data/fact_hotel_price/part-0.parquet",
            "data/fact_hotel_price/_delta_log/0.json",
            "data/x.csv",
        ];
        assert_eq!(
            folders_below("data/", keys.into_iter()),
            [
                "data/fact_hotel_price/",
                "data/fact_hotel_price/_delta_log/"
            ]
        );
        // A connection prefix without its trailing slash is the same floor.
        assert_eq!(folders_below("data", keys.into_iter()).len(), 2);
        assert_eq!(folders_below("", ["a/b/c"].into_iter()), ["a/", "a/b/"]);
    }

    #[test]
    fn a_slash_in_the_query_matches_the_path_instead_of_the_name() {
        assert!(search_matches(
            "part-0.parquet",
            "2024/sales/part-0.parquet",
            "4/sal"
        ));
        assert!(!search_matches(
            "part-0.parquet",
            "2024/sales/part-0.parquet",
            "sales"
        ));
        assert!(search_matches("sales", "2024/sales/", "sales"));
    }

    #[test]
    fn revealing_a_folder_expands_every_level_down_to_it() {
        assert_eq!(
            reveal_path("", "bucket/a/b/"),
            ["", "bucket/", "bucket/a/", "bucket/a/b/"]
        );
        assert_eq!(reveal_path("data/", "data/a/"), ["data/", "data/a/"]);
    }
}
