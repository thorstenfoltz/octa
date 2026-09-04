//! Per-tab session state: closed-tab snapshots, bookmarks, rename drafts,
//! search navigation and the two background Ask jobs.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

/// Snapshot of a tab that was just closed, used to power the
/// `ReopenLastClosedTab` (Ctrl+Shift+T) shortcut.
///
/// For tabs backed by a file on disk, the path is retained - reopening
/// rereads the file, which is cheaper than holding a full `TabState` clone
/// and keeps any concurrent edits visible. For scratch tabs (no source
/// path: parsed-in-new-tab, raw edits, empty welcome tab) only the textual
/// payload (`raw_content` + view mode + format label) is kept - enough to
/// recreate the visible state without trying to deep-clone egui textures,
/// commonmark caches, etc. Truly empty tabs are not snapshotted.
pub(crate) enum ClosedTabSnapshot {
    Path(std::path::PathBuf),
    Scratch {
        raw_content: String,
        view_mode: ViewMode,
        format_name: Option<String>,
    },
}

/// A named jump target within a tab. Session-only and fixed-position: a
/// bookmark points at a row (and optionally a column) index and does not track
/// later row inserts or deletes.
#[derive(Debug, Clone)]
pub(crate) struct Bookmark {
    pub name: String,
    pub row: usize,
    pub col: Option<usize>,
}

/// Draft state for the "name this bookmark" dialog.
#[derive(Clone)]
pub(crate) struct BookmarkDraft {
    pub name_buf: String,
    pub row: usize,
    pub col: Option<usize>,
    pub size: ui::settings::DialogSize,
}

/// Draft state for the "Rename tab" dialog. Renames the tab's display name only
/// (the file path is unchanged); an empty name reverts to the file name.
#[derive(Clone)]
pub(crate) struct TabRenameDraft {
    pub tab_index: usize,
    pub name_buf: String,
    pub size: ui::settings::DialogSize,
}

/// Cache entry for the SQL workspace inspector. Stores either the
/// successful introspection or the error message returned by the workspace
/// so the inspector can render the error inline instead of refetching every
/// frame.
#[derive(Debug, Clone)]
pub(crate) struct InspectorCacheEntry {
    pub(crate) result: Result<octa::sql::TableInspection, String>,
}

/// A file read running on a background thread so the UI stays responsive.
/// `finish_single_load` consumes the result when the worker completes.
pub(crate) struct PendingLoad {
    pub(crate) path: std::path::PathBuf,
    pub(crate) format_name: String,
    pub(crate) rx: std::sync::mpsc::Receiver<anyhow::Result<DataTable>>,
}

/// Per-tab transient state for highlight-search navigation. `match_count` and
/// `current` are recomputed by the active view each frame; `pending_jump` is a
/// one-shot request set by the search-bar buttons / Enter keys and consumed by
/// the view that owns the matches.
#[derive(Debug, Clone, Default)]
pub(crate) struct SearchNavState {
    pub(crate) match_count: usize,
    pub(crate) current: usize,
    pub(crate) pending_jump: Option<NavDir>,
}

impl SearchNavState {
    /// Reset to the empty state (no matches, no pending jump). Called whenever
    /// the query, search mode, view mode, or file changes.
    pub(crate) fn reset(&mut self) {
        self.match_count = 0;
        self.current = 0;
        self.pending_jump = None;
    }
}

/// Direction for next/previous match navigation in highlight search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavDir {
    Next,
    Prev,
}

/// One in-flight "Ask" request: which tab asked, and the slot the worker
/// writes its parsed answer into. Cancelled implicitly by dropping the job -
/// the worker's write is simply ignored once the slot is gone.
pub(crate) struct AskFilterJob {
    pub(crate) tab_idx: usize,
    pub(crate) result: Arc<Mutex<Option<Result<crate::app::chat::ask_filter::AskResult, String>>>>,
}

/// One in-flight "Ask SQL" request. `insert_at` is the byte offset in the
/// tab's `sql_query` recorded when the user pressed Ask, so a reply that
/// arrives after more typing still lands where they were pointing.
pub(crate) struct AskSqlJob {
    pub(crate) tab_idx: usize,
    pub(crate) insert_at: usize,
    pub(crate) result: Arc<Mutex<Option<Result<String, String>>>>,
}

#[derive(Clone)]
pub(crate) enum UpdateState {
    /// No check in progress
    Idle,
    /// Checking GitHub for latest version
    Checking,
    /// A newer version is available. The release body is not carried: the
    /// notes window shows the running version's notes, baked into the binary.
    Available { version: String },
    /// Already on the latest version.
    UpToDate,
    /// Currently downloading and installing
    Updating,
    /// Linux only: the new binary has been downloaded to `tmp_path`, but the
    /// install directory is not writable by the current user. Prompt the user
    /// to elevate so we can place the binary at `install_path`.
    NeedsElevation {
        version: String,
        install_path: std::path::PathBuf,
        tmp_path: std::path::PathBuf,
    },
    /// Update completed successfully
    Updated(String),
    /// An error occurred
    Error(String),
}

/// Views where filtering free text or collapsing nodes is meaningless, so the
/// search always highlights in place regardless of the Filter/Highlight toggle.
pub(crate) fn view_is_text_or_tree(vm: ViewMode) -> bool {
    matches!(
        vm,
        ViewMode::Notebook
            | ViewMode::Raw
            | ViewMode::Markdown
            | ViewMode::JsonTree
            | ViewMode::YamlTree
    )
}

/// Whether the active view highlights matches (vs filtering rows): true when
/// the session mode is `Highlight` or the view is a text/tree view.
pub(crate) fn effective_highlight(vm: ViewMode, mode: data::SearchResultMode) -> bool {
    mode == data::SearchResultMode::Highlight || view_is_text_or_tree(vm)
}
