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

/// The find and replace box over one editor (Ctrl+F / Ctrl+H while the
/// editor has focus). `SqlPane.find` is `None` while it is closed.
#[derive(Debug, Clone, Default)]
pub(crate) struct SqlFind {
    pub(crate) input: crate::view_modes::find_box::FindInput,
    /// Which match is current, an index into the matches of the find text.
    pub(crate) current: usize,
    /// Mark the current match in the editor and scroll to it on the next frame.
    pub(crate) jump: bool,
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
    /// **Fit columns** (button or shortcut) was asked for: the next frame of
    /// the result grid fits every column to its title and longest loaded
    /// value, then clears it.
    pub(crate) fit_columns: bool,
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
    /// The find box, when open.
    pub(crate) find: Option<SqlFind>,
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

    /// Holds text or belongs to a file, so it cannot just be written over.
    pub(crate) fn holds_text(&self) -> bool {
        !self.query.trim().is_empty() || self.file.is_some()
    }

    /// Nothing here to lose: no text, no file, no result, no query running.
    fn is_blank(&self) -> bool {
        !self.holds_text() && self.result.is_none() && self.running_since.is_none()
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

    /// Every editor, the hollow placeholder among them.
    fn sql_panes_all(&self) -> impl Iterator<Item = &SqlPane> {
        std::iter::once(&self.sql).chain(&self.sql_panes)
    }

    /// Whether anything of the user's sits in the SQL state: an editor that
    /// is not blank, or a table or database added to the workspace (`data`
    /// and the other open tabs are put there by Octa and come back by
    /// themselves).
    pub(crate) fn sql_holds_something(&self) -> bool {
        !self.sql_panes_all().all(SqlPane::is_blank)
            || self.sql_workspace.as_ref().is_some_and(|ws| {
                !ws.list_attached().is_empty()
                    || ws.list_tables().iter().any(|t| {
                        !matches!(t.origin, octa::sql::TableOrigin::ActiveTab)
                            && !self.sql_auto_registered.contains(&t.sql_name)
                    })
            })
    }

    /// Closing this tab must leave its editors behind: a query in one is
    /// still running, or the panel is open with something in it. A panel the
    /// user closed first has been dealt with.
    pub(crate) fn sql_survives_close(&self) -> bool {
        self.sql_panes_all().any(|p| p.running_since.is_some())
            || (self.sql_panel_open && self.sql_holds_something())
    }

    /// This tab can take a closed tab's editors: they would show here, and
    /// nothing of its own is lost by it.
    pub(crate) fn sql_can_host(&self) -> bool {
        self.view_mode == octa::data::ViewMode::Table
            && self.sql_import.is_none()
            && !self.sql_holds_something()
    }

    /// Hand the editors to `to`, with everything their queries depend on, as
    /// they are: pane ids stay, so a query still running lands in its editor,
    /// and results, files and the connection stay with it. The workspace goes
    /// along, since its paged results and attachments are what the editors
    /// refer to. What `to` had in their place goes to `self`.
    pub(crate) fn move_sql_to(&mut self, to: &mut TabState) {
        use std::mem::swap;
        swap(&mut self.sql, &mut to.sql);
        swap(&mut self.sql_panes, &mut to.sql_panes);
        swap(&mut self.sql_active_pane, &mut to.sql_active_pane);
        swap(&mut self.sql_ask_input, &mut to.sql_ask_input);
        swap(&mut self.sql_panel_open, &mut to.sql_panel_open);
        swap(&mut self.sql_target, &mut to.sql_target);
        swap(&mut self.sql_workspace, &mut to.sql_workspace);
        swap(&mut self.sql_job, &mut to.sql_job);
        swap(&mut self.sql_import, &mut to.sql_import);
        swap(&mut self.sql_other_tabs, &mut to.sql_other_tabs);
        swap(&mut self.sql_auto_registered, &mut to.sql_auto_registered);
        swap(
            &mut self.sql_auto_register_sig,
            &mut to.sql_auto_register_sig,
        );
        swap(&mut self.sql_workspace_open, &mut to.sql_workspace_open);
        swap(
            &mut self.sql_inspector_selection,
            &mut to.sql_inspector_selection,
        );
        swap(&mut self.sql_inspector_cache, &mut to.sql_inspector_cache);
        swap(
            &mut self.sql_workspace_tree_expanded,
            &mut to.sql_workspace_tree_expanded,
        );
        swap(&mut self.sql_write_back, &mut to.sql_write_back);
        // `data` in that workspace is not `to`'s table, so the next Run
        // copies it, and what is known about it is out of date.
        to.sql_data_stamp = None;
        to.sql_inspector_cache
            .remove(&crate::app::sql_panel::InspectorTarget::RegisteredTable {
                sql_name: "data".to_string(),
            });
        if let Some(job) = to.sql_job.as_mut() {
            job.table_gone();
        }
    }

    /// Every editor left to right, the active one in its place.
    pub(crate) fn sql_panes_in_order(&self) -> impl Iterator<Item = &SqlPane> {
        (0..self.sql_panes.len()).map(|i| {
            if i == self.sql_active_pane {
                &self.sql
            } else {
                &self.sql_panes[i]
            }
        })
    }

    /// Each editor's text, left to right.
    #[cfg(test)]
    pub(crate) fn sql_queries(&self) -> Vec<String> {
        self.sql_panes_in_order()
            .map(|pane| pane.query.clone())
            .collect()
    }

    /// Show a file read from disk: in the active editor when it is blank
    /// (no text, no file), else in a new editor beside the others.
    pub(crate) fn put_sql_file(&mut self, path: PathBuf, text: String, crlf: bool) {
        if self.sql.holds_text() {
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

    /// An editor from the last session, in the active editor when that is
    /// blank. Its file is read again rather than remembered, so one changed
    /// since shows as unsaved. One that is gone keeps the link, and Save writes
    /// it back there.
    pub(crate) fn put_saved_sql_editor(&mut self, query: String, file: Option<PathBuf>) {
        if self.sql.holds_text() {
            self.add_sql_pane();
        }
        self.sql.file = file.map(|path| {
            let (saved, crlf) = read_sql_file(&path).unwrap_or_default();
            SqlFile { path, saved, crlf }
        });
        self.sql.query = query;
    }
}

#[cfg(test)]
#[path = "sql_pane_tests.rs"]
mod tests;
