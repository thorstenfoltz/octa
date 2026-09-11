//! The toolbar's return type ([`ToolbarAction`]) and the [`ParseScope`] enum.
//! Split out of the main toolbar module purely for navigability - no behaviour
//! change. `draw_toolbar` fills a `ToolbarAction` and the app shell reads it
//! (the Interaction-struct pattern; no callbacks).

use std::collections::HashSet;

use crate::data::{DataTable, MarkColor, MarkKey, SearchMode, SearchResultMode, ViewMode};
use crate::ui::theme::ThemeColors;

/// Everything the eight top-level menus read, in one place.
///
/// `draw_toolbar` used to be a single ~2,000-line function taking 53
/// parameters behind an `#[allow(clippy::too_many_arguments)]`. Splitting it
/// one-file-per-menu would have needed that `#[allow]` on every one of the
/// eight, since the widest block (View) touches fourteen of them, and
/// CLAUDE.md forbids silencing the lint. Bundling instead follows the pattern
/// [`AskControls`] already set in the same signature.
///
/// Deliberately `Copy` (every field is a scalar or a shared reference) so each
/// menu can destructure it into locals with the original names. That keeps the
/// moved menu bodies byte-identical: no `cx.` prefix threaded through hundreds
/// of lines, and no chance of a rename touching an i18n key such as
/// `"menu.table"`.
///
/// Read-only by construction. Menus report what the user chose by writing to
/// [`ToolbarAction`]; nothing here is a `&mut`.
///
/// Note what is *not* here: the session bookmark list. **Data -> Add bookmark**
/// only sets `action.add_bookmark`, which needs no data; the one place that
/// iterates the list is the Bookmarks dropdown on the search bar, which
/// `draw_toolbar` still renders itself. So the list stays a `draw_toolbar`
/// parameter rather than becoming a field no menu reads.
#[derive(Clone, Copy)]
pub struct ToolbarCtx<'a> {
    pub colors: ThemeColors,
    /// A table is open (`col_count() > 0`). Menus whose entries all need one
    /// show a short note instead of their contents.
    pub has_data: bool,
    pub has_edits: bool,
    /// This tab has a file on disk: gates Reopen as and git compare, which
    /// genuinely need a file to re-read.
    pub has_source_path: bool,
    /// Save can write this tab without asking for a path. Also true for a
    /// live-database tab, whose Save is the write-back dialog. Distinct from
    /// `has_source_path`, and conflating the two hid Save on every db tab.
    pub can_save_in_place: bool,
    pub is_db_tab: bool,
    pub selected_cell: Option<(usize, usize)>,
    pub selected_rows: &'a HashSet<usize>,
    pub selected_cols: &'a HashSet<usize>,
    pub selected_cells: &'a HashSet<(usize, usize)>,
    pub row_count: usize,
    pub col_count: usize,
    pub current_view_mode: ViewMode,
    pub has_raw_content: bool,
    pub has_markdown: bool,
    pub has_notebook: bool,
    pub has_epub: bool,
    pub has_map: bool,
    pub has_record: bool,
    pub has_json: bool,
    pub has_yaml: bool,
    /// Whether at least one chat model profile is configured, so the entries
    /// that talk to the assistant can grey themselves out with a reason.
    pub chat_profile_available: bool,
    pub readonly_mode: bool,
    /// Whether the active tab's table is split into two scrolling panes.
    pub split_view: bool,
    /// Which way that split runs: `true` = side by side, `false` = stacked.
    /// Only meaningful while `split_view` is on.
    pub split_side_by_side: bool,
    /// How many bands the split is showing, 1 when there is no split. Decides
    /// whether Add pane / Remove pane are enabled.
    pub split_panes: usize,
    /// Whether "Filter to marked" is active for this tab, so the Edit menu can
    /// show the clear variant.
    pub mark_filter_active: bool,
    pub zoom_percent: u32,
    pub recent_files: &'a [String],
    pub directory_tree_open: bool,
    pub first_row_is_header: bool,
    pub has_hidden_columns: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub can_reopen_tab: bool,
    pub table: &'a DataTable,
    /// Small logo texture for the left of the bar. `None` until the first
    /// frame that builds it.
    pub logo_texture: Option<&'a egui::TextureHandle>,
    /// Render close / maximise / minimise at the right edge, and make the
    /// toolbar background the window's drag handle. Paired with
    /// `AppSettings.use_custom_title_bar`, which also strips the system
    /// decorations in `main.rs`.
    pub show_window_controls: bool,
}

/// The search bar's state: the one part of the toolbar that edits what it is
/// given, which is why it is a separate bundle from [`ToolbarCtx`] rather than
/// more fields on it.
///
/// Not `Copy`, and not shared with the menus: it holds `&mut` borrows of the
/// active tab's search fields, so `draw_toolbar` destructures it once and
/// renders the bar itself.
pub struct SearchControls<'a> {
    pub text: &'a mut String,
    pub mode: &'a mut SearchMode,
    /// Case-sensitive (`Aa`) and whole-word toggles, and the column scope
    /// (`None` = whole table, `Some(col)` = one column). Edited in place.
    pub case_sensitive: &'a mut bool,
    pub whole_word: &'a mut bool,
    pub scope_col: &'a mut Option<usize>,
    /// Column names for the scope dropdown, in table order.
    pub column_names: &'a [String],
    pub ask: AskControls<'a>,
    /// Recent queries, most recent first, for the history dropdown.
    pub history: &'a [String],
    /// Session search behaviour (Filter vs Highlight). The table respects it;
    /// text and tree views always highlight.
    pub result_mode: &'a mut SearchResultMode,
    /// Matches are highlighted rather than filtered for this tab's view: true
    /// when `result_mode == Highlight` OR the view is a text/tree view. Drives
    /// whether the count and next/prev controls appear.
    pub highlight_active: bool,
    /// Match count and current 1-based position, computed by the view on the
    /// previous frame.
    pub match_count: usize,
    pub match_current: usize,
    pub focus_requested: bool,
    pub show_replace_bar: bool,
    pub replace_text: &'a mut String,
    /// Session bookmarks as `(name, row, col)`: a library-safe shape, since the
    /// `Bookmark` type lives in the binary-side `app` module. Lives here rather
    /// than on `ToolbarCtx` because only the bar's Bookmarks dropdown iterates
    /// the list; **Data -> Add bookmark** just sets a flag.
    pub bookmarks: &'a [(String, usize, Option<usize>)],
}

/// Which slice of the active table to feed into the "Parse in new tab"
/// modal. Set by the Edit menu submenu or the table's right-click context
/// menu; the app shell turns it into a [`PendingParseModal`] for the
/// dialog renderer to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseScope {
    /// Single cell at `(row, col)` (display-row coordinates).
    Cell { row: usize, col: usize },
    /// Whole row at display-row index `row`.
    Row { row: usize },
    /// Whole column at index `col`.
    Column { col: usize },
    /// The entire active table.
    Table,
}

#[derive(Default)]
pub struct ToolbarAction {
    pub new_file: bool,
    /// Open the New-table dialog (a blank editable grid in a new tab).
    /// Fired by **File -> New Table...**.
    pub new_table: bool,
    pub open_file: bool,
    /// User picked **View -> Reopen as -> <format>**: re-read the active tab's
    /// file through the named reader, for a file whose extension lies about its
    /// format. Carries the `FormatRegistry` reader name (e.g. "JSON").
    pub open_as: Option<&'static str>,
    /// User picked **File -> Open as -> <format>**: pick one or more files and
    /// open them all through the named reader. Same idea as `open_as`, but for
    /// files that are not open yet.
    pub open_as_files: Option<&'static str>,
    /// Open a folder as a Delta Lake / Apache Iceberg table (the table format
    /// is a directory, not a file). Fired by **File -> Open table folder...**.
    pub open_table_folder: bool,
    /// Open the Batch convert dialog for a folder the user picks.
    /// Fired by **File -> Batch convert...**.
    pub open_batch_convert: bool,
    pub open_directory: bool,
    pub close_directory: bool,
    /// Toggle the sidebar cloud-storage browser (File -> Cloud connections).
    pub toggle_cloud_browser: bool,
    /// Toggle the sidebar live-database browser (File -> Databases).
    pub toggle_db_browser: bool,
    pub open_recent: Option<String>,
    /// Right-click -> "Remove from list" on a single recent-files entry.
    pub remove_recent: Option<String>,
    /// Right-click -> "Clear all" on a recent-files entry.
    pub clear_recent: bool,
    pub save_file: bool,
    pub save_file_as: bool,
    /// Export a live-database tab's pending edits as a reviewable SQL script.
    pub save_db_sql: bool,
    /// Write several open tabs into one .xlsx, one sheet per tab.
    pub export_workbook: bool,
    /// Read a file straight from a web address.
    pub open_url: bool,
    pub toggle_theme: bool,
    pub search_changed: bool,
    /// The search box lost focus with a non-empty query: record it in the
    /// persistent search history.
    pub commit_search_history: bool,
    /// Ask mode is on and the user pressed Enter in the search box: turn the
    /// typed sentence into filters via the chosen assistant profile.
    pub ask_submitted: bool,
    /// The Filter/Highlight search-behaviour toggle was flipped this frame.
    pub search_result_mode_changed: bool,
    /// Jump to the next highlight-search match (`>` button or Enter).
    pub find_next: bool,
    /// Jump to the previous highlight-search match (`<` button or Shift+Enter).
    pub find_prev: bool,
    pub add_row: bool,
    pub delete_row: bool,
    pub add_column: bool,
    pub time_calc: bool,
    pub delete_column: bool,
    pub move_row_up: bool,
    pub move_row_down: bool,
    pub move_col_left: bool,
    pub move_col_right: bool,
    pub sort_rows_asc_by: Option<usize>,
    pub sort_rows_desc_by: Option<usize>,
    /// Reorder all columns alphabetically by name (case-insensitive).
    pub sort_columns_asc: bool,
    /// Reorder all columns reverse-alphabetically by name (case-insensitive).
    pub sort_columns_desc: bool,
    /// Clear the active tab's `hidden_columns` so every column becomes
    /// visible again. Wired to Edit -> Show hidden columns.
    pub show_all_columns: bool,
    /// Open the Excel-style Column Filter dialog. Outer `Some` = the user
    /// invoked the action this frame (menu click, header context menu,
    /// status-bar chip, ...); inner `Some(col)` = preselect that column, inner
    /// `None` = no preselect (dialog opens on the first column or the
    /// previously remembered one).
    pub show_column_filter: Option<Option<usize>>,
    pub discard_edits: bool,
    pub view_mode_changed: Option<ViewMode>,
    pub show_settings: bool,
    pub show_about: bool,
    /// Open the "Report AI content" dialog from the Help menu.
    pub show_ai_report: bool,
    pub check_for_updates: bool,
    pub export_debug_report: bool,
    pub replace_next: bool,
    pub replace_all: bool,
    pub toggle_replace_bar: bool,
    pub search_focus: bool,
    pub show_documentation: bool,
    pub exit: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    pub zoom_reset: bool,
    pub toggle_sql_panel: bool,
    /// Open a Chart tab for the active table. Fired by **Analyse ->
    /// Chart** (toolbar) or the `OpenChart` shortcut. Independent from
    /// `toggle_sql_panel` so the user can have either / both / neither.
    pub open_chart_tab: bool,
    /// Open the Value Frequency column picker (no column context). Fired by
    /// **Analyse -> Value frequency...**.
    pub open_value_frequency: bool,
    /// Open a Summary tab (per-column statistics via DuckDB SUMMARIZE) for
    /// the active table. Fired by **Analyse -> Summary...**.
    pub open_describe_tab: bool,
    /// **Analyse -> File internals...**: open a detached tab showing how the
    /// active tab's file is physically written (row groups, compression,
    /// column statistics).
    pub open_file_internals: bool,
    /// **Analyse -> Compare with database table...**: diff the active tab
    /// against a table on a saved database connection.
    pub open_db_compare: bool,
    /// Analyse -> Data drift...
    pub open_drift: bool,
    /// Analyse -> Relationship map...
    pub open_rel_map: bool,
    /// **Analyse -> Join key finder...**: rank the column pairs that would
    /// join the open tabs.
    pub open_join_keys: bool,
    pub open_join_diag: bool,
    pub open_schema_drift: bool,
    pub open_harmonise: bool,
    pub open_report: bool,
    /// Open the "Export to PDF" dialog for the active tab's view.
    pub export_pdf: bool,
    pub open_fuzzy_join: bool,
    /// Open the Pivot / Unpivot dialog for the active table.
    /// Fired by **Analyse -> Pivot / Unpivot...**.
    pub open_pivot: bool,
    /// Open the Time series dialog (bucketing / rolling window) for the active
    /// table. Fired by **Analyse -> Time series...**.
    pub open_timeseries: bool,
    /// Toggle the clean-up suggestions panel for the active table.
    /// Fired by **Analyse -> Clean-up suggestions**. Opening the panel starts
    /// the scan; nothing runs until then.
    pub open_cleanup_panel: bool,
    /// Open the multi-column sort dialog for the active table.
    /// Fired by **Analyse -> Sort by columns...**.
    pub open_multi_sort: bool,
    /// Copy the current selection to the clipboard as a Markdown table.
    /// Fired by **Edit -> Copy as Markdown table**.
    pub copy_as_markdown: bool,
    /// Open the per-column Number-format dialog for the selected column.
    /// Fired by **Edit -> Number format...**.
    pub open_column_format: bool,
    /// Open the Conditional formatting dialog for the active table.
    /// Fired by **Edit -> Conditional formatting...**.
    pub open_conditional_format: bool,
    /// Open the Data validation dialog for the active table.
    /// Fired by **Data -> Data validation...**.
    pub open_validation: bool,
    /// Open the Transform-column dialog for the active table.
    /// Fired by **Edit -> Transform column...**.
    pub open_transform: bool,
    /// Open the Conditional-column (CASE / if-elseif-else) dialog for the
    /// active table. Fired by **Edit -> Conditional column...**.
    pub open_conditional_column: bool,
    /// Toggle "Filter to marked": keep only marked rows/columns/cells.
    /// Fired by **Edit -> Filter to marked**.
    pub filter_to_marked: bool,
    /// Open the bulk "Rename columns" dialog for the active table.
    /// Fired by **Columns -> Rename columns...**.
    pub open_rename_columns: bool,
    /// Columns -> "Fix duplicate names...": the rename dialog, opened with its
    /// duplicates half already ticked.
    pub fix_duplicate_columns: bool,
    /// Open a Data-quality report tab for the active table.
    /// Fired by **Analyse -> Data quality report...**.
    pub open_quality: bool,
    /// Open a transposed (rows <-> columns) copy of the active table in a new
    /// tab. Fired by **Analyse -> Transpose**.
    pub open_transpose: bool,
    /// Compare the rows the user selected (or marked), field by field, in a
    /// new tab.
    pub open_row_compare: bool,
    /// Open the Random-sample dialog for the active table.
    /// Fired by **Analyse -> Random sample...**.
    pub open_random_sample: bool,
    /// Open the Tidy-up dialog (trim / clean headers) for the active table.
    /// Fired by **Data -> Tidy up...**.
    pub open_tidy_up: bool,
    /// Start adding a bookmark at the current selection (opens the naming
    /// dialog). Fired by the toolbar **Bookmarks -> Add bookmark...** entry.
    pub add_bookmark: bool,
    /// Jump to the bookmark at this index in the active tab's list.
    pub jump_bookmark: Option<usize>,
    /// Delete the bookmark at this index in the active tab's list.
    pub delete_bookmark: Option<usize>,
    /// Open the Anonymise-columns dialog for the active table.
    /// Fired by **Edit -> Anonymise columns...**.
    pub open_anonymize: bool,
    /// Open the Fill-missing-values (impute) dialog for the active table.
    /// Fired by **Edit -> Fill missing values...**.
    pub open_impute: bool,
    /// Open the Find-near-duplicates (fuzzy) dialog for the active table.
    /// Fired by **Data -> Find near-duplicates...**.
    pub open_fuzzy_duplicates: bool,
    /// Open the Partition-by-column dialog. Fired by **Analyse -> Partition by column...**.
    pub open_partition: bool,
    /// Open the Union-tables dialog. Fired by **Analyse -> Union tables...**.
    pub open_union: bool,
    /// Open the Join-tables dialog. Fired by **Analyse -> Join tables...**.
    pub open_join: bool,
    /// Open the Detect-outliers dialog. Fired by **Analyse -> Detect outliers...**.
    pub open_outliers: bool,
    /// Open the Detect-PII dialog. Fired by **Analyse -> Detect PII...**.
    pub open_pii: bool,
    /// Toggle "first row is header" for the active table.
    pub toggle_first_row_header: bool,
    /// Apply a color mark to a set of keys (cell/row/column).
    pub set_marks: Vec<(MarkKey, MarkColor)>,
    /// Clear color marks from a set of keys.
    pub clear_marks: Vec<MarkKey>,
    /// Clear every color mark on the active table. Wired to the new
    /// "Clear all marks" entry in **Edit -> Mark**; reachable even
    /// without a selection so users can wipe duplicate-row highlights
    /// without first selecting the rows.
    pub clear_all_marks: bool,
    /// Undo the last change.
    pub undo: bool,
    /// Redo the last undone change.
    pub redo: bool,
    /// Logo in the top-left was clicked. Wired to a hidden easter-egg counter
    /// in the app shell - most users never trigger it.
    pub logo_clicked: bool,
    /// Toggle session-only read-only mode (also bound to F8 by default).
    pub toggle_readonly: bool,
    /// Toggle the stacked split view of the active tab's table.
    pub toggle_split_view: bool,
    /// Toggle the side-by-side split view of the active tab's table.
    pub toggle_split_side_by_side: bool,
    /// Cut one more band out of the split, up to `MAX_SPLIT_PANES`.
    pub add_split_pane: bool,
    /// Take one band away, down to two.
    pub remove_split_pane: bool,
    /// Open the "Parse in new tab" modal pre-seeded with this scope.
    /// `None` means the menu wasn't clicked this frame.
    pub parse_in_new_tab: Option<ParseScope>,
    /// Restore the most-recently-closed tab. Wired to the Edit menu entry
    /// (the Ctrl+Shift+T shortcut is handled separately in
    /// `shortcuts_dispatch`).
    pub reopen_last_closed_tab: bool,
    /// Resize every column in the active table to its best-fit width.
    /// Wired to the Edit menu entry (the Ctrl+Shift+W shortcut is handled
    /// separately in `shortcuts_dispatch`).
    pub fit_all_columns: bool,
    /// Give every row the height its content needs (turns cell line breaks
    /// on). Fired by **Edit -> Auto-fit All Rows**.
    pub fit_all_rows: bool,
    /// User clicked View -> Compare with...  The app shell opens a file
    /// picker, loads the picked file as the right side, and flips the
    /// active tab into `ViewMode::Compare`.
    pub compare_with: bool,
    /// User clicked View -> Compare with git version...  The app shell opens
    /// the revision-picker dialog (or shows a status message when the file is
    /// not in a git repo).
    pub open_git_compare: bool,
    /// Open the Correlation-matrix dialog for the active table.
    /// Fired by **Analyse -> Correlation...**.
    pub open_correlation: bool,
    pub open_dist_compare: bool,
    pub open_referential: bool,
    /// Open the **Edit -> Find duplicates...** modal for the active tab.
    /// The dialog itself lives in `app::dialogs::find_duplicates`; the
    /// toolbar just signals "user wants it open".
    pub show_find_duplicates: bool,
    /// Open the write-to-database dialog sourced from the **open table**
    /// rather than from a SQL result. Fired by **File -> Save to
    /// database...** and the `SaveTableToDb` keyboard shortcut.
    pub open_table_to_db: bool,
    /// Open the Schema Export dialog. The dialog itself lets the user
    /// switch between the seven supported targets; there's no need for
    /// the toolbar to pre-pick one. Fired by **File -> Export schema...**
    /// and the `ExportSchema` keyboard shortcut.
    pub show_schema_export: bool,
    /// Toggle the cross-tab + directory multi-search panel. Fired by
    /// **Search -> Multi-search...** and the `MultiSearch` keyboard
    /// shortcut.
    pub toggle_multi_search: bool,
    /// Toggle the in-GUI chat assistant panel. Fired by the toolbar Assistant
    /// button, **View -> Assistant panel**, and the `ToggleChatPanel` shortcut.
    pub toggle_chat_panel: bool,
    /// Ask the assistant to explain the active tab, in the chat panel.
    pub explain_file: bool,
}

/// The search bar's "Ask" controls, bundled so `draw_toolbar` takes one
/// parameter rather than four more.
///
/// `enabled` is false when no chat profile is configured; the toggle and the
/// profile picker are then greyed out and their tooltip explains why rather
/// than repeating the label.
pub struct AskControls<'a> {
    pub enabled: bool,
    /// Configured profiles as `(id, display name)`, in Settings order.
    pub profiles: &'a [(String, String)],
    /// Whether Ask mode is on for the active tab. Edited in place.
    pub mode: &'a mut bool,
    /// Id of the profile that will answer. Edited in place.
    pub profile_id: &'a mut String,
}
