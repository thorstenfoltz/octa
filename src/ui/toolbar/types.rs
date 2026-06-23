//! The toolbar's return type ([`ToolbarAction`]) and the [`ParseScope`] enum.
//! Split out of the main toolbar module purely for navigability - no behaviour
//! change. `draw_toolbar` fills a `ToolbarAction` and the app shell reads it
//! (the Interaction-struct pattern; no callbacks).

use crate::data::{MarkColor, MarkKey, ViewMode};

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
    pub open_file: bool,
    /// Open a folder as a Delta Lake / Apache Iceberg table (the table format
    /// is a directory, not a file). Fired by **File -> Open table folder...**.
    pub open_table_folder: bool,
    pub open_directory: bool,
    pub close_directory: bool,
    pub open_recent: Option<String>,
    /// Right-click -> "Remove from list" on a single recent-files entry.
    pub remove_recent: Option<String>,
    /// Right-click -> "Clear all" on a recent-files entry.
    pub clear_recent: bool,
    pub save_file: bool,
    pub save_file_as: bool,
    pub toggle_theme: bool,
    pub search_changed: bool,
    /// The search box lost focus with a non-empty query: record it in the
    /// persistent search history.
    pub commit_search_history: bool,
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
    /// Open the Pivot / Unpivot dialog for the active table.
    /// Fired by **Analyse -> Pivot / Unpivot...**.
    pub open_pivot: bool,
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
    /// Fired by **Edit -> Data validation...**.
    pub open_validation: bool,
    /// Open the Transform-column dialog for the active table.
    /// Fired by **Edit -> Transform column...**.
    pub open_transform: bool,
    /// Open the Conditional-column (CASE / if-elseif-else) dialog for the
    /// active table. Fired by **Edit -> Conditional column...**.
    pub open_conditional_column: bool,
    /// Open the Anonymise-columns dialog for the active table.
    /// Fired by **Edit -> Anonymise columns...**.
    pub open_anonymize: bool,
    /// Open the Fill-missing-values (impute) dialog for the active table.
    /// Fired by **Edit -> Fill missing values...**.
    pub open_impute: bool,
    /// Open the Drop-duplicate-rows dialog for the active table.
    /// Fired by **Edit -> Drop duplicate rows...**.
    pub open_dedupe: bool,
    /// Open the Find-near-duplicates (fuzzy) dialog for the active table.
    /// Fired by **Search -> Find near-duplicates...**.
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
    /// User clicked View -> Compare with...  The app shell opens a file
    /// picker, loads the picked file as the right side, and flips the
    /// active tab into `ViewMode::Compare`.
    pub compare_with: bool,
    /// Open the **Edit -> Find duplicates...** modal for the active tab.
    /// The dialog itself lives in `app::dialogs::find_duplicates`; the
    /// toolbar just signals "user wants it open".
    pub show_find_duplicates: bool,
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
}
