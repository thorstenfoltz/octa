//! One SQL editor with its own query, result and run state. A tab's SQL
//! panel shows any number of them side by side; the workspace, the run
//! target and the Ask box stay shared per tab.
//!
//! The active pane lives in `TabState.sql`, so every path that works on "the"
//! editor (Run, Format, shortcuts, the write-back dialog, Ask) reads one plain
//! field and the borrow checker sees it as disjoint from the rest of the tab.
//! The others wait in `TabState.sql_panes`, whose slot at `sql_active_pane`
//! holds a hollow placeholder while its pane is out in `sql`. The panel draws
//! pane `i` by swapping it in, drawing the unchanged single-editor code, and
//! swapping back.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context as _;
use octa::data::DataTable;

use super::TabState;

/// Stable identity of a pane. Keys its egui widgets (the editor's caret must
/// not jump to another pane when the order changes) and lets a finished
/// server query find the pane it was started from after panes were closed.
/// 0 is the placeholder; real panes start at 1.
static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

/// Which of the three plan views a pane shows. Kept per pane for the session
/// and never reset by a new Explain, so the choice sticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PlanView {
    #[default]
    Tree,
    Table,
    Server,
}

/// The `.sql` file an editor was opened from or saved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SqlFile {
    pub(crate) path: PathBuf,
    /// The text as last read or written, LF line endings. The editor is
    /// unsaved while its text differs: a comparison rather than a flag,
    /// because Format, Ask, History and Insert all write `query` directly.
    pub(crate) saved: String,
    /// The file used CRLF, so Save writes it back that way.
    pub(crate) crlf: bool,
}

/// Read a `.sql` file for the editor: UTF-8, BOM dropped, CRLF folded to LF
/// (the editor works in LF). Returns the text and whether it had CRLF.
pub(crate) fn read_sql_file(path: &Path) -> anyhow::Result<(String, bool)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let owned = String::from_utf8(bytes)
        .with_context(|| format!("{} is not UTF-8 text", path.display()))?;
    let text = owned.strip_prefix('\u{feff}').unwrap_or(&owned);
    Ok((text.replace("\r\n", "\n"), text.contains("\r\n")))
}

/// Write the editor's text, with CRLF again when the file had it.
pub(crate) fn write_sql_file(path: &Path, text: &str, crlf: bool) -> anyhow::Result<()> {
    let lf = text.replace("\r\n", "\n");
    let out = if crlf { lf.replace('\n', "\r\n") } else { lf };
    octa::formats::write_atomically(path, |tmp| Ok(std::fs::write(tmp, out)?))
        .with_context(|| format!("writing {}", path.display()))
}

/// The file's name for labels and messages, or the whole path when it has none.
pub(crate) fn file_display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[derive(Default)]
pub(crate) struct SqlPane {
    pub(crate) id: u64,
    pub(crate) query: String,
    pub(crate) result: Option<DataTable>,
    pub(crate) error: Option<String>,
    /// Selected cells, rows and columns in the result grid: highlighted and
    /// used as the Ctrl+C copy target.
    pub(crate) result_sel: crate::view_modes::sql::SqlResultSelection,
    /// Rows the last result has in total, exact, from a `count(*)` over the
    /// materialised result. `result` holds only the pages scrolled to so far;
    /// `None` means the statement was not paged and `result` is everything.
    pub(crate) result_total: Option<usize>,
    /// The editor grabs keyboard focus on the next frame, so the user can
    /// type without clicking first. Consumed by `draw_sql_editor`.
    pub(crate) focus_pending: bool,
    /// Autocomplete popup: highlighted suggestion (clamped every frame).
    pub(crate) ac_selected: usize,
    /// Autocomplete popup: on while the user types; off after Escape, a pick,
    /// or any caret move that is not typing.
    pub(crate) ac_visible: bool,
    /// The editor's caret as the last frame left it, to tell a caret moved
    /// by typing from one moved by a click, an arrow or a selection.
    pub(crate) ac_caret: Option<egui::text::CCursorRange>,
    /// Last successfully executed SELECT, verbatim, so the write-back dialog
    /// has a source query to compose `CREATE TABLE AS ...` from.
    pub(crate) last_query: String,
    /// `last_query` ran on a server connection, not in the local workspace:
    /// its tables exist only there, so the write-back dialog writes the rows
    /// it returned instead of running it again locally.
    pub(crate) last_query_on_server: bool,
    /// How long the last query took, shown beside the row count.
    pub(crate) last_duration_ms: Option<u64>,
    /// Set when a query starts, cleared when it ends in any way. While set,
    /// the result area shows a running state instead of the previous result,
    /// which otherwise looked like the answer to the new query.
    pub(crate) running_since: Option<std::time::Instant>,
    /// The plan Explain showed last. Run and Clear drop it.
    pub(crate) plan: Option<octa::sql::explain::Plan>,
    /// The plan view this pane shows (Text tree until the user picks another).
    pub(crate) plan_view: PlanView,
    /// The file this editor was opened from or saved to.
    pub(crate) file: Option<SqlFile>,
}

impl SqlPane {
    pub(crate) fn new() -> Self {
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            ..Default::default()
        }
    }

    /// Has a file, and its text changed since the last read or save.
    pub(crate) fn is_dirty(&self) -> bool {
        self.file.as_ref().is_some_and(|f| f.saved != self.query)
    }

    /// The run is starting: drop what the last one showed.
    pub(crate) fn start_run(&mut self) {
        self.result = None;
        self.result_total = None;
        self.result_sel = Default::default();
        self.error = None;
        self.plan = None;
        self.running_since = Some(std::time::Instant::now());
    }

    /// Show `plan`: its steps go into the result grid (the Table view, which
    /// keeps Copy and Export working), the plan itself backs the other two.
    pub(crate) fn show_plan(&mut self, plan: octa::sql::explain::Plan) {
        self.result = Some(octa::sql::explain::steps_table(&plan));
        self.result_total = None;
        self.result_sel = Default::default();
        self.error = None;
        self.running_since = None;
        self.plan = Some(plan);
    }
}

impl TabState {
    pub(crate) fn sql_pane_count(&self) -> usize {
        self.sql_panes.len()
    }

    /// Make pane `i` the one in `self.sql`.
    pub(crate) fn activate_sql_pane(&mut self, i: usize) {
        let active = self.sql_active_pane;
        if i == active || i >= self.sql_panes.len() {
            return;
        }
        // Put the active pane back into its slot, then take pane `i` out.
        std::mem::swap(&mut self.sql, &mut self.sql_panes[active]);
        std::mem::swap(&mut self.sql, &mut self.sql_panes[i]);
        self.sql_active_pane = i;
    }

    /// A new, empty editor to the right of the others, focused.
    pub(crate) fn add_sql_pane(&mut self) {
        self.sql_panes.push(SqlPane::new());
        self.activate_sql_pane(self.sql_panes.len() - 1);
        self.sql.focus_pending = true;
    }

    /// Remove pane `i`. The last remaining pane stays.
    pub(crate) fn close_sql_pane(&mut self, i: usize) {
        let n = self.sql_panes.len();
        if n <= 1 || i >= n {
            return;
        }
        if i == self.sql_active_pane {
            self.activate_sql_pane(if i == 0 { 1 } else { i - 1 });
        }
        let closed = self.sql_panes.remove(i);
        if let Some(ws) = self.sql_workspace.as_ref() {
            ws.drop_result(closed.id);
        }
        if self.sql_active_pane > i {
            self.sql_active_pane -= 1;
        }
    }

    /// Id of the pane at position `i`.
    pub(crate) fn sql_pane_id(&self, i: usize) -> u64 {
        if i == self.sql_active_pane {
            self.sql.id
        } else {
            self.sql_panes[i].id
        }
    }

    pub(crate) fn has_sql_pane(&self, id: u64) -> bool {
        id != 0 && (self.sql.id == id || self.sql_panes.iter().any(|p| p.id == id))
    }

    /// The pane with this id, wherever it sits.
    pub(crate) fn sql_pane_by_id_mut(&mut self, id: u64) -> Option<&mut SqlPane> {
        if self.sql.id == id {
            return Some(&mut self.sql);
        }
        self.sql_panes.iter_mut().find(|p| p.id == id && id != 0)
    }

    /// Every editor's text is blank and none has a file.
    pub(crate) fn sql_editors_empty(&self) -> bool {
        let blank = |p: &SqlPane| p.query.trim().is_empty() && p.file.is_none();
        blank(&self.sql) && self.sql_panes.iter().all(blank)
    }

    /// Each editor's text and file, left to right.
    pub(crate) fn sql_editors(&self) -> Vec<(String, Option<SqlFile>)> {
        (0..self.sql_panes.len())
            .map(|i| {
                let p = if i == self.sql_active_pane {
                    &self.sql
                } else {
                    &self.sql_panes[i]
                };
                (p.query.clone(), p.file.clone())
            })
            .collect()
    }

    /// Take over a closed tab's editors: only when every editor here is
    /// blank (never overwrite what the user typed), and only the ones with
    /// text or a file.
    pub(crate) fn adopt_sql_editors(&mut self, editors: Vec<(String, Option<SqlFile>)>) {
        let editors: Vec<(String, Option<SqlFile>)> = editors
            .into_iter()
            .filter(|(q, f)| !q.trim().is_empty() || f.is_some())
            .collect();
        if self.sql_editors_empty() && !editors.is_empty() {
            self.set_sql_editors(editors);
        }
    }

    /// Replace the editors with one per entry (at least one), first active.
    pub(crate) fn set_sql_editors(&mut self, editors: Vec<(String, Option<SqlFile>)>) {
        let mut panes: Vec<SqlPane> = editors
            .into_iter()
            .map(|(query, file)| SqlPane {
                query,
                file,
                ..SqlPane::new()
            })
            .collect();
        if panes.is_empty() {
            panes.push(SqlPane::new());
        }
        self.sql = std::mem::take(&mut panes[0]);
        self.sql_panes = panes;
        self.sql_active_pane = 0;
    }

    /// Show a file read from disk: in the active editor when it is blank
    /// (no text, no file), else in a new editor beside the others.
    pub(crate) fn put_sql_file(&mut self, path: PathBuf, text: String, crlf: bool) {
        if !self.sql.query.trim().is_empty() || self.sql.file.is_some() {
            self.add_sql_pane();
        }
        self.sql.file = Some(SqlFile {
            path,
            saved: text.clone(),
            crlf,
        });
        self.sql.query = text;
        self.sql.focus_pending = true;
    }
}

#[cfg(test)]
#[path = "sql_pane_tests.rs"]
mod tests;
