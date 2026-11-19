//! User-customizable keyboard shortcut bindings.
//!
//! Each app action is represented by [`ShortcutAction`] and bound to a
//! [`KeyCombo`] (key + modifiers). [`Shortcuts`] stores the map from action to
//! binding; it is embedded in `AppSettings` and persists in `settings.toml`.
//!
//! At input time, the handler in `main.rs` calls [`Shortcuts::triggered`] to
//! check whether a given action's binding was just pressed.
//!
//! Some actions can be disabled by clearing their binding (see [`KeyCombo::is_empty`]).

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use serde::{Deserialize, Serialize};

/// Whether a key press is being captured for rebinding rather than acted on.
///
/// Process-wide because the dispatch sites are spread out: the app's shortcut
/// handler, the table view's own key handling and the clipboard actions in the
/// central panel all ask [`KeyCombo::triggered`] independently, and guarding
/// only the first of them meant recording Ctrl+C still copied the selection.
/// One flag inside the shared matcher covers every site, including later ones.
static CAPTURE_MODE: AtomicBool = AtomicBool::new(false);

/// Suppress (or re-enable) shortcut dispatch. Set once per frame by the app
/// from the Settings dialog's recording state.
pub fn set_capture_mode(on: bool) {
    CAPTURE_MODE.store(on, Ordering::Relaxed);
}

/// Whether shortcut dispatch is currently suppressed.
pub fn capture_mode() -> bool {
    CAPTURE_MODE.load(Ordering::Relaxed)
}

thread_local! {
    /// A command picked in Find a command, waiting for the next frame.
    /// Per thread: the GUI runs on one, and parallel tests driving the
    /// update loop then cannot take each other's command.
    static QUEUED: Cell<Option<ShortcutAction>> = const { Cell::new(None) };
    /// The command firing during this frame. Every dispatch site asks
    /// [`Shortcuts::triggered`], so one slot reaches all of them.
    static FIRING: Cell<Option<ShortcutAction>> = const { Cell::new(None) };
}

/// Run `action` on the next frame as if its shortcut had been pressed.
pub fn queue_command(action: ShortcutAction) {
    QUEUED.with(|q| q.set(Some(action)));
}

/// Called once at the start of every frame, before any shortcut is read:
/// the queued command fires during this frame and no other.
pub fn begin_frame() {
    let next = QUEUED.with(Cell::take);
    FIRING.with(|f| f.set(next));
}

fn firing(action: ShortcutAction) -> bool {
    FIRING.with(|f| f.get() == Some(action))
}

/// A command picked in Find a command fires this frame. The checks that keep
/// a key press out of a focused text field's way let it through: it was asked
/// for by name, and without this it was dropped with no word to the user.
pub fn command_firing() -> bool {
    FIRING.with(|f| f.get().is_some())
}

/// A keyboard combination: a main key plus modifier flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    /// `None` means "unbound" (the action has no keyboard shortcut).
    pub key: Option<egui::Key>,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
}

impl KeyCombo {
    pub const UNBOUND: Self = Self {
        key: None,
        ctrl: false,
        shift: false,
        alt: false,
    };

    pub const fn ctrl(key: egui::Key) -> Self {
        Self {
            key: Some(key),
            ctrl: true,
            shift: false,
            alt: false,
        }
    }

    pub const fn ctrl_shift(key: egui::Key) -> Self {
        Self {
            key: Some(key),
            ctrl: true,
            shift: true,
            alt: false,
        }
    }

    pub const fn ctrl_alt(key: egui::Key) -> Self {
        Self {
            key: Some(key),
            ctrl: true,
            shift: false,
            alt: true,
        }
    }

    pub const fn shift_alt(key: egui::Key) -> Self {
        Self {
            key: Some(key),
            ctrl: false,
            shift: true,
            alt: true,
        }
    }

    pub const fn plain(key: egui::Key) -> Self {
        Self {
            key: Some(key),
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Check whether this combination was just pressed this frame. Matches on
    /// exact modifier equality (e.g. Ctrl+A will not fire for Ctrl+Shift+A).
    ///
    /// Always false while the Settings dialog is recording a new binding: the
    /// key press belongs to the grid then, not to whatever currently owns it.
    pub fn triggered(&self, input: &egui::InputState) -> bool {
        self.key
            .is_some_and(|key| self.matches(key, input.modifiers) && input.key_pressed(key))
    }

    /// Whether a press of `key` with `m` held is this combination, by the
    /// same rule as [`Self::triggered`]. For code that walks the event list
    /// itself, like the text editors' extra carets.
    pub fn matches(&self, key: egui::Key, m: egui::Modifiers) -> bool {
        // `command` collapses Cmd on mac and Ctrl elsewhere.
        !capture_mode()
            && self.key == Some(key)
            && m.command == self.ctrl
            && m.shift == self.shift
            && m.alt == self.alt
    }

    /// Drop this combination's key press from the frame's events, so a
    /// focused text field does not act on it too (egui reads Ctrl+H and
    /// Ctrl+K as editing keys).
    pub fn consume(&self, input: &mut egui::InputState) {
        input.events.retain(|e| {
            !matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
                if self.matches(*key, *modifiers))
        });
    }

    /// Human-readable label like "Ctrl+Shift+P" or "(none)".
    pub fn label(&self) -> String {
        let Some(key) = self.key else {
            return crate::i18n::t("views.none");
        };
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        let _ = write!(s, "{}", key_label(key));
        s
    }
}

fn key_label(k: egui::Key) -> &'static str {
    // egui's Debug repr is fine for letters/digits but we spell out a few
    // special keys that are clearer as symbols.
    match k {
        egui::Key::Plus | egui::Key::Equals => "+",
        egui::Key::Minus => "-",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::Space => "Space",
        egui::Key::Enter => "Enter",
        egui::Key::Tab => "Tab",
        egui::Key::Escape => "Esc",
        egui::Key::Backspace => "Backspace",
        egui::Key::Delete => "Delete",
        egui::Key::Home => "Home",
        egui::Key::End => "End",
        egui::Key::PageUp => "PgUp",
        egui::Key::PageDown => "PgDn",
        egui::Key::ArrowUp => "Up",
        egui::Key::ArrowDown => "Down",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        _ => letter_or_other(k),
    }
}

fn letter_or_other(k: egui::Key) -> &'static str {
    // Return a static str for letter keys. egui::Key's Debug gives "A", "B", etc.,
    // but that's an owned string. Map the common letters explicitly so we keep
    // the signature &'static str (the combo label is produced on demand anyway).
    // Anything else falls back to egui's own name rather than a bare "?", which
    // told the user nothing about what they had just bound.
    match k {
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        other => other.name(),
    }
}

/// All application actions that can be bound to a shortcut.
/// Every variant listed here is honored by the global key handler in `main.rs`,
/// except the caret ones, which the text editors read themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumIter)]
pub enum ShortcutAction {
    NewFile,
    /// Open the New-table dialog (blank editable grid in a new tab). Also
    /// **File -> New Table...**. Ships unbound: no free Ctrl+Shift letter.
    NewTable,
    OpenFile,
    SaveFile,
    SaveFileAs,
    /// File -> Save with password...: the table inside an AES-256 zip.
    SaveWithPassword,
    /// File -> Save to cloud/SFTP...: the table as a file in a saved cloud or
    /// file-server connection.
    SaveToCloud,
    /// File -> Save to database...: the table into a saved connection.
    SaveTableToDb,
    /// Write a live-database tab's pending edits out as a SQL script instead
    /// of applying them. Deliberately separate from SaveFile: reviewing a
    /// script and saving are different intents.
    SaveDbSql,
    /// Write several open tabs into one .xlsx workbook.
    ExportWorkbook,
    /// Read a file straight from a web address.
    OpenUrl,
    OpenApiEndpoint,
    ReloadFile,
    FocusSearch,
    ToggleFindReplace,
    CloseTab,
    QuitApp,
    NextTab,
    PrevTab,
    SelectAllRows,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    LowercaseSelection,
    UppercaseSelection,
    EditCell,
    GoToCell,
    DuplicateRow,
    DeleteRow,
    InsertRowBelow,
    ToggleSqlPanel,
    /// Jump the selected cell to the top of the column.
    JumpFirstRow,
    /// Jump the selected cell to the bottom of the column.
    JumpLastRow,
    /// Jump the selected cell to the leftmost column.
    JumpFirstCol,
    /// Jump the selected cell to the rightmost column.
    JumpLastCol,
    /// When a full row is selected, add the row above to the selection.
    /// When a full column is selected, no-op (use `ExtendSelectionLeft`).
    ExtendSelectionUp,
    /// When a full row is selected, add the row below to the selection.
    ExtendSelectionDown,
    /// When a full column is selected, add the column on the left.
    ExtendSelectionLeft,
    /// When a full column is selected, add the column on the right.
    ExtendSelectionRight,
    /// Export the current SQL query result to a file (only when a result is
    /// available). No-op when no result has been produced yet.
    ExportSqlResult,
    /// Copy the current selection to the OS clipboard.
    Copy,
    /// Cut the current selection (copy then clear cells).
    Cut,
    /// Paste OS clipboard contents into the current selection.
    Paste,
    /// Apply the default mark color (configurable in Settings) to the current
    /// selection. Honors the same precedence as the toolbar Mark menu:
    /// rows > columns > free multi-cell selection > single cell.
    Mark,
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Open the Settings dialog.
    OpenSettings,
    /// Open the Documentation dialog.
    OpenDocumentation,
    /// Open the Excel-style Column Filter dialog for the active tab. The
    /// Search menu's entry deliberately omits this shortcut from its label
    /// (the binding is discoverable via Settings -> Shortcuts).
    OpenColumnFilter,
    /// Cycle to the next available view mode for the active tab. The cycle
    /// order matches the View menu (Table, Raw, Markdown, Notebook,
    /// JSON Tree); modes that aren't applicable to the current file are
    /// skipped. No-op when the focus is in a TextEdit so the keypress can
    /// reach the editor.
    CycleViewMode,
    /// Toggle session-only read-only mode. Disables every editing path
    /// (cell edits, structural changes, marks, undo/redo, cut/paste,
    /// SQL DML). Persists for the current session only - every launch
    /// starts editable.
    ToggleReadOnly,
    /// Follow the active tab's growing file, like `tail -f`. Also
    /// **View -> Follow file**.
    ToggleFollow,
    /// Open the Auto-refresh dialog for a database or API tab. Also
    /// **View -> Auto-refresh...**.
    OpenAutoRefresh,
    /// Resize every column in the active table to its best-fit width
    /// (header + content sampled up to 5000 rows). Same algorithm used
    /// by double-clicking the header seam between two columns, applied
    /// across the whole table.
    FitAllColumns,
    /// Give every row the height its content needs (turns cell line breaks
    /// on). Also **Edit -> Auto-fit All Rows**. Ships unbound.
    FitAllRows,
    /// **Columns -> Hide selected columns**. Display only. Ships unbound.
    HideSelectedColumns,
    /// **Edit -> Hide selected rows**. Display only. Ships unbound.
    HideSelectedRows,
    /// **Edit -> Show hidden rows**. Ships unbound.
    ShowHiddenRows,
    /// Reopen the most-recently-closed tab. Stack-based - repeated
    /// triggers walk back through close history (capacity 10). No-op
    /// when the close stack is empty.
    ReopenLastClosedTab,
    /// Compare the active tab with the Ctrl-click-selected tab. Requires
    /// exactly one tab in the multi-selection set; otherwise the shortcut
    /// no-ops. The active tab becomes the Left side, the selected tab the
    /// Right side, and the active tab switches into `ViewMode::Compare`.
    CompareSelectedTabs,
    /// Open the Value Frequency dialog for the column of the currently
    /// selected cell - `df.value_counts()` for the active column. Falls
    /// back to the first column when no cell is selected. Also reachable
    /// via the column-header right-click -> "Value frequency..." entry.
    ColumnValueFrequency,
    /// Open the Find Duplicates dialog for the active tab. The dialog
    /// seeds its key from the currently selected column or cell. Also
    /// reachable via **Data -> Find duplicates...**. Highlight, new tab,
    /// filter and drop modes all live in that one dialog.
    FindDuplicates,
    /// Open the Find-near-duplicates (fuzzy) dialog. Also
    /// **Data -> Find near-duplicates...**.
    OpenFuzzyDuplicates,
    /// Open the Schema Export dialog. The dialog itself lets the user
    /// pick which of the seven targets (Postgres / MySQL / SQLite /
    /// Pydantic v2 / TypeScript / JSON Schema / Rust) to render, so a
    /// single key is enough. Also reachable via **File -> Export schema...**.
    ExportSchema,
    /// Open the cross-tab + directory multi-search panel. The active-tab
    /// search bar (`Ctrl+F`) is unchanged - multi-search adds scope =
    /// All Open Tabs or Directory and a results list. Reachable via
    /// **Search -> Multi-search...**.
    MultiSearch,
    /// Jump to the next cell flagged by validation or outlier detection.
    NextProblem,
    /// Jump to the previous flagged cell.
    PrevProblem,
    /// Open a Chart tab for the active table. Equivalent to clicking
    /// **Analyse -> Chart** in the toolbar. The new tab is single-mode
    /// (`ViewMode::Chart`), holds a clone of the source table, and
    /// exposes the chart-kind / column / styling / export controls.
    OpenChart,
    /// Scroll the table viewport up by one full page and advance the
    /// selected cell by the same number of rows. Mirrors the way Excel
    /// and most spreadsheets treat PageUp. No-op when the focus is in a
    /// TextEdit so the editor can page through its own buffer.
    ScrollPageUp,
    /// Scroll the table viewport down by one full page and advance the
    /// selected cell by the same number of rows. See [`Self::ScrollPageUp`].
    ScrollPageDown,
    /// Toggle the in-GUI chat assistant panel open / closed. Also reachable
    /// via the toolbar Assistant button and **View -> Assistant panel**.
    ToggleChatPanel,
    /// Let the Assistant fill the window, or dock it again. Also the
    /// Maximise / Restore button in its header.
    ToggleChatMaximise,
    /// Open the Pivot / Unpivot dialog. Also **Analyse -> Pivot / Unpivot...**.
    OpenPivot,
    /// Open the Transform-column dialog. Also **Edit -> Transform column...**.
    OpenTransform,
    /// Open the Conditional-formatting dialog. Also
    /// **Edit -> Conditional formatting...**.
    OpenConditionalFormat,
    /// Open the Conditional-column (CASE) dialog. Also
    /// **Edit -> Conditional column...**.
    OpenConditionalColumn,
    /// Open the Hash-columns dialog. Also **Columns -> Hash columns...**.
    OpenHashColumns,
    /// Open the Calculated-column dialog. Also **Columns -> Calculated column...**.
    OpenCalculatedColumn,
    /// Columns -> Number bands...
    OpenBands,
    /// Data -> Reference data...
    OpenReferenceData,
    /// Open the Classify with Jev dialog. Also **Columns -> Classify with Jev...**.
    OpenClassify,
    /// Open the Anonymise-columns dialog. Also **Edit -> Anonymise columns...**.
    OpenAnonymize,
    /// Open the Data-validation dialog. Also **Data -> Data validation...**.
    OpenValidation,
    /// Open the multi-column Sort dialog. Also **Analyse -> Sort by columns...**.
    OpenMultiSort,
    /// Open the Summary tab. Also **Analyse -> Summary...**.
    OpenSummary,
    /// Open the per-column Number-format dialog. Also
    /// **Edit -> Number format...**.
    OpenNumberFormat,
    /// Copy the current selection as a GitHub Markdown table. Also
    /// **Edit -> Copy as Markdown table**.
    CopyAsMarkdown,
    /// Open the Find Duplicates dialog preset to drop the repeats: whole-row
    /// key, keep first. The former Drop-duplicate-rows dialog, kept as a
    /// shortcut so remapped bindings survive the merge.
    OpenDedupe,
    /// Open the Fill-missing-values (impute) dialog. Also
    /// **Edit -> Fill missing values...**.
    OpenImpute,
    /// Columns -> Change type... (re-type one column).
    OpenRetypeColumn,
    OpenCleanupPanel,
    OpenTimeseries,
    OpenBatchConvert,
    /// Open the Schema drift dialog. Also **File -> Schema drift...**.
    OpenSchemaDrift,
    /// Open the Merge versions dialog. Also **File -> Merge versions...**.
    OpenMergeVersions,
    /// Open the Earlier versions dialog. Also **File -> Earlier versions...**.
    OpenFileVersions,
    /// Export the open database table's schema to files. Also the
    /// Databases sidebar's schema menu **Export tables...**.
    OpenDbExport,
    OpenTestData,
    /// Open the Report dialog. Also **File -> Report...**.
    OpenReport,
    /// Open the Fuzzy join dialog. Also **Data -> Fuzzy join...**.
    OpenFuzzyJoin,
    /// Open the File internals tab. Also **Analyse -> File internals...**.
    OpenFileInternals,
    /// Open the database/cloud compare dialog. Also
    /// **Analyse -> Compare with database or cloud...**.
    OpenDbCompare,
    /// Open the Compare folders dialog. Also **Analyse -> Compare folders...**.
    OpenFolderCompare,
    OpenDataDrift,
    OpenRelMap,
    /// Open the Join key finder. Also **Analyse -> Join key finder...**.
    OpenJoinKeys,
    /// Open Join diagnostics. Also **Analyse -> Join diagnostics...**.
    OpenJoinDiag,
    /// Open the Harmonise schemas dialog. Also **File -> Harmonise schemas...**.
    OpenHarmonise,
    /// Toggle the search bar's plain-language Ask mode.
    ToggleAskFilter,
    /// Open the Union-tables dialog. Also **Analyse -> Union tables...**.
    OpenUnion,
    /// Open the Join-tables dialog. Also **Analyse -> Join tables...**.
    OpenJoin,
    /// Open the Split into files dialog. Also **Data -> Split into files...**.
    OpenPartition,
    /// Open the Detect-outliers dialog. Also **Analyse -> Detect outliers...**.
    OpenOutliers,
    /// Open the Detect-PII dialog. Also **Analyse -> Detect PII...**.
    OpenPii,
    /// Toggle "Filter to marked" for the active tab. Also
    /// **Edit -> Filter to marked**.
    FilterToMarked,
    /// Open a Data-quality report tab. Also **Analyse -> Data quality report...**.
    OpenQualityReport,
    /// Open the bulk "Rename columns" dialog. Also
    /// **Columns -> Rename columns...**.
    OpenRenameColumns,
    /// Add a bookmark at the current selection (opens the naming dialog).
    /// Also the toolbar **Bookmarks -> Add bookmark...** entry.
    AddBookmark,
    /// Rename the active tab (display name only; the file path is unchanged).
    /// Also the tab right-click **Rename tab...** entry.
    RenameActiveTab,
    /// Put the active tab (or the Ctrl-click selection) into a new tab group.
    /// Also the tab right-click **New group...** entry.
    NewTabGroup,
    /// Give the active tab the next colour of the palette, after the last one
    /// none. The tab right-click **Colour** picks one directly.
    NextTabColour,
    /// Dissolve the active tab's group; its tabs stay. Also the group chip's
    /// **Ungroup** entry.
    Ungroup,
    /// Tile the Ctrl-click selection plus the active tab side by side. Also
    /// **View -> Tile tabs -> Side by side**.
    TileSideBySide,
    /// Same, stacked as rows.
    TileStacked,
    /// Same, in a grid.
    TileGrid,
    /// Dissolve the active tab's tile set. Also **View -> Untile**.
    Untile,
    /// Open a Delta/Iceberg/parts dataset directory. Also
    /// **File -> Open table folder...**.
    OpenTableFolder,
    /// Run a recursive inventory of the first expanded cloud connection
    /// into a detached tab. Also the cloud tree's right-click
    /// **List contents as table...**.
    ListCloudInventory,
    /// Run the SQL editor's query on the active tab's live database server
    /// (only meaningful on a tab opened from the Databases tree).
    RunSqlOnServer,
    /// Comment or uncomment the SQL editor lines touched by the selection
    /// (`--` after any leading blanks). See `view_modes::sql::toggle_line_comments`.
    ToggleSqlComment,
    /// Lay out the SQL editor's query (the marked part, or all of it) in the
    /// style set under Settings -> SQL. Also the SQL panel's **Format**.
    FormatSql,
    /// Fit every column of the SQL result grid to its title and longest loaded
    /// value. Also the SQL panel's **Fit columns**.
    FitSqlResultColumns,
    /// Show the SQL editor's query plan without running it (the panel's
    /// **Explain**). Named apart from `ExplainFile`, the assistant's
    /// "Explain this file".
    ExplainQuery,
    /// Open another SQL editor beside the others (the panel's **+**), or
    /// open the panel when it is closed.
    AddSqlEditor,
    /// Read a `.sql` file into the SQL editor (the panel's **File -> Open...**),
    /// opening the panel when it is closed.
    OpenSqlFile,
    /// Save the SQL editor's query to a new `.sql` file (**File -> Save as...**).
    /// Plain Save is the Save file shortcut while the editor has focus.
    SaveSqlFileAs,
    /// Copy the selected cells as a SQL `IN` list, `('a', 'b')`. Also the
    /// cell right-click **Copy as IN list** and **Edit -> Copy as IN list**.
    CopyAsInList,
    /// Copy the selection as a JSON array of objects. Also **Edit -> Copy as**.
    CopyAsJson,
    /// Copy the selection as SQL `INSERT` statements. Also **Edit -> Copy as**.
    CopyAsInsert,
    /// Edit -> Fill series...
    FillSeries,
    /// Search -> Find a command... (Ctrl+K)
    FindCommand,
    /// Analyse -> Cohorts...
    OpenCohorts,
    /// Analyse -> Funnel...
    OpenFunnel,
    /// Analyse -> Customer segments (RFM)...
    OpenRfm,
    /// Open the Correlation dialog. Also **Analyse -> Correlation...**.
    OpenCorrelation,
    /// Open the Find lookup tables dialog. Also
    /// **Analyse -> Find lookup tables...**.
    OpenLookups,
    /// Open Cell history for the selected cell. Also the cell right-click
    /// **Cell history...**. Only for files inside a Git repository.
    OpenCellHistory,
    /// Open the binary viewer on the selected cell (the cell menu's
    /// **View contents...**).
    OpenBinaryViewer,
    /// Open the Compare-distributions dialog. Also
    /// **Analyse -> Compare distributions...**.
    OpenDistCompare,
    /// Open the Referential-integrity dialog. Also
    /// **Analyse -> Referential integrity...**.
    OpenReferential,
    /// Open a Transpose tab. Also **Analyse -> Transpose**.
    OpenTranspose,
    /// Open a Compare-rows tab for the selected or marked rows. Also
    /// **Analyse -> Compare rows...**.
    OpenRowCompare,
    /// Open the Random-sample dialog. Also **Analyse -> Random sample...**.
    OpenRandomSample,
    /// Count the rows of the source and offer to download every one the tab
    /// does not hold. Also **Data -> Load whole table...**.
    LoadWholeTable,
    /// Open the Tidy-up dialog. Also **Data -> Tidy up...**. No-op in
    /// read-only mode, like the menu entry.
    OpenTidyUp,
    /// Open the Date/Time calculation dialog. Also
    /// **Data -> Date/Time calculation...**.
    OpenTimeCalc,
    /// Open the PDF export dialog. Also **File -> Export to PDF...**.
    ExportPdf,
    /// Open a folder in the directory tree. Also
    /// **File -> Open Directory...**.
    OpenDirectory,
    /// Open the git-version compare dialog. Also
    /// **View -> Compare with git version...**.
    OpenGitCompare,
    /// Ask the assistant to explain the active file. Also
    /// **Analyse -> Explain this file**.
    ExplainFile,
    /// Open the "Report AI content" dialog. Also
    /// **Help -> Report AI content...**.
    OpenAiReport,
    /// Toggle the cloud-storage sidebar. Also **File -> Cloud/SFTP connections**.
    ToggleCloudBrowser,
    /// Toggle the databases sidebar. Also **File -> Databases**.
    ToggleDbBrowser,
    /// Split the table view into two stacked panes of the same table, or
    /// close the split. Also **View -> Split view**.
    ToggleSplitView,
    /// Split the table view into two side-by-side panes, or close the split.
    /// Also **View -> Split side by side**.
    ToggleSplitSideBySide,
    /// Cut one more band out of an existing split, up to
    /// `table_view::MAX_SPLIT_PANES`. Also **View -> Add pane**.
    AddSplitPane,
    /// Take one band away, down to two. Also **View -> Remove pane**.
    RemoveSplitPane,
    /// Toggle the column navigator panel open / closed. Also **View ->
    /// Column navigator**. Ships unbound: every clipboard-safe Ctrl+Shift
    /// letter is already spoken for.
    ToggleColumnNavigator,
    /// Toggle the edit audit trail panel open / closed. Also **View -> Edit
    /// audit trail**. Ships unbound: every clipboard-safe Ctrl+Shift letter
    /// is already spoken for.
    ToggleEditAudit,
    /// Edit -> Recipe panel.
    ToggleRecipePanel,
    /// Edit -> Apply recipe...
    ApplyRecipe,
    /// The recipe panel's "Choose ID column..." (which column names a row
    /// for recorded hand edits).
    ChooseRecipeKey,
    /// View -> Tab memory... (what each open tab is holding).
    OpenTabMemory,
    /// A caret on the line above, in the Raw, Markdown and SQL editors.
    /// Handled by `view_modes::multi_cursor`, not `handle_shortcuts`; so are
    /// the two below.
    AddCaretAbove,
    AddCaretBelow,
    /// A caret at the end of every marked line.
    CaretsAtLineEnds,
}

impl ShortcutAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::NewTable => "New table...",
            Self::OpenFile => "Open file",
            Self::SaveFile => "Save file",
            Self::SaveDbSql => "Save database changes as SQL...",
            Self::ExportWorkbook => "Export workbook...",
            Self::OpenUrl => "Open URL...",
            Self::OpenApiEndpoint => "Open API endpoint...",
            Self::SaveFileAs => "Save file as...",
            Self::SaveWithPassword => "Save with password...",
            Self::SaveToCloud => "Save to cloud/SFTP...",
            Self::ReloadFile => "Refresh tab",
            Self::FocusSearch => "Focus search box",
            Self::ToggleFindReplace => "Toggle find & replace",
            Self::CloseTab => "Close current tab",
            Self::QuitApp => "Quit application",
            Self::NextTab => "Next tab",
            Self::PrevTab => "Previous tab",
            Self::SelectAllRows => "Select all rows",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::ZoomReset => "Reset zoom",
            Self::LowercaseSelection => "Lowercase selected cells",
            Self::UppercaseSelection => "Uppercase selected cells",
            Self::EditCell => "Edit current cell",
            Self::GoToCell => "Go to cell (focus nav input)",
            Self::DuplicateRow => "Duplicate selected row(s)",
            Self::DeleteRow => "Delete selected row(s)",
            Self::InsertRowBelow => "Insert row below",
            Self::ToggleSqlPanel => "Toggle SQL panel",
            Self::JumpFirstRow => "Jump to first row",
            Self::JumpLastRow => "Jump to last row",
            Self::JumpFirstCol => "Jump to first column",
            Self::JumpLastCol => "Jump to last column",
            Self::ExtendSelectionUp => "Extend row selection up",
            Self::ExtendSelectionDown => "Extend row selection down",
            Self::ExtendSelectionLeft => "Extend column selection left",
            Self::ExtendSelectionRight => "Extend column selection right",
            Self::ExportSqlResult => "Export SQL result",
            Self::Copy => "Copy selection",
            Self::Cut => "Cut selection",
            Self::Paste => "Paste",
            Self::Mark => "Mark selection (default colour)",
            Self::Undo => "Undo last change",
            Self::Redo => "Redo last undone change",
            Self::OpenSettings => "Open settings",
            Self::OpenDocumentation => "Open documentation",
            Self::OpenColumnFilter => "Filter by value or shape...",
            Self::CycleViewMode => "Cycle view mode",
            Self::ToggleReadOnly => "Toggle read-only mode",
            Self::ToggleFollow => "Follow file",
            Self::OpenAutoRefresh => "Auto-refresh...",
            Self::FitAllColumns => "Auto-fit all columns",
            Self::FitAllRows => "Auto-fit all rows",
            Self::HideSelectedColumns => "Hide selected columns",
            Self::HideSelectedRows => "Hide selected rows",
            Self::ShowHiddenRows => "Show hidden rows",
            Self::ReopenLastClosedTab => "Reopen last closed tab",
            Self::CompareSelectedTabs => "Compare selected tabs",
            Self::ColumnValueFrequency => "Show column value frequency",
            Self::FindDuplicates => "Find duplicate rows...",
            Self::OpenFuzzyDuplicates => "Find near-duplicates...",
            Self::ExportSchema => "Export schema...",
            Self::SaveTableToDb => "Save to database...",
            Self::MultiSearch => "Open multi-search panel",
            Self::NextProblem => "Jump to next flagged cell",
            Self::PrevProblem => "Jump to previous flagged cell",
            Self::OpenChart => "Open chart tab",
            Self::ScrollPageUp => "Scroll up one page",
            Self::ScrollPageDown => "Scroll down one page",
            Self::ToggleChatPanel => "Toggle chat assistant panel",
            Self::ToggleChatMaximise => "Maximise / restore chat assistant",
            Self::OpenPivot => "Pivot / Unpivot...",
            Self::OpenTransform => "Transform column...",
            Self::OpenConditionalFormat => "Conditional formatting...",
            Self::OpenConditionalColumn => "Conditional column...",
            Self::OpenHashColumns => "Hash columns...",
            Self::OpenCalculatedColumn => "Calculated column...",
            Self::OpenBands => "Number bands...",
            Self::OpenReferenceData => "Reference data...",
            Self::OpenClassify => "Classify with Jev...",
            Self::OpenAnonymize => "Anonymise columns...",
            Self::OpenValidation => "Data validation...",
            Self::OpenMultiSort => "Sort by columns...",
            Self::OpenSummary => "Summary tab",
            Self::OpenNumberFormat => "Number format...",
            Self::CopyAsMarkdown => "Copy as Markdown table",
            Self::OpenDedupe => "Drop duplicate rows...",
            Self::OpenImpute => "Fill missing values...",
            Self::OpenRetypeColumn => "Change type...",
            Self::OpenCleanupPanel => "Clean-up suggestions",
            Self::OpenTimeseries => "Time series...",
            Self::OpenCorrelation => "Correlation...",
            Self::OpenLookups => "Find lookup tables...",
            Self::OpenCellHistory => "Cell history...",
            Self::OpenBinaryViewer => "View cell contents...",
            Self::OpenDistCompare => "Compare distributions...",
            Self::OpenReferential => "Referential integrity...",
            Self::OpenTranspose => "Transpose",
            Self::OpenRowCompare => "Compare rows...",
            Self::OpenRandomSample => "Random sample...",
            Self::LoadWholeTable => "Load whole table...",
            Self::OpenTidyUp => "Tidy up...",
            Self::OpenTimeCalc => "Date/Time calculation...",
            Self::ExportPdf => "Export to PDF...",
            Self::OpenDirectory => "Open directory...",
            Self::OpenGitCompare => "Compare with git version...",
            Self::ExplainFile => "Explain this file",
            Self::OpenAiReport => "Report AI content...",
            Self::ToggleCloudBrowser => "Toggle cloud panel",
            Self::ToggleDbBrowser => "Toggle databases panel",
            Self::ToggleSplitView => "Split view",
            Self::ToggleSplitSideBySide => "Split side by side",
            Self::AddSplitPane => "Add split pane",
            Self::RemoveSplitPane => "Remove split pane",
            Self::OpenBatchConvert => "Batch convert...",
            Self::OpenSchemaDrift => "Schema drift...",
            Self::OpenMergeVersions => "Merge versions...",
            Self::OpenFileVersions => "Earlier versions...",
            Self::OpenDbExport => "Export database tables...",
            Self::OpenTestData => "Generate test data...",
            Self::OpenReport => "Report...",
            Self::OpenFuzzyJoin => "Fuzzy join...",
            Self::OpenFileInternals => "File internals...",
            Self::OpenDbCompare => "Compare with database or cloud...",
            Self::OpenFolderCompare => "Compare folders...",
            Self::OpenDataDrift => "Data drift...",
            Self::OpenRelMap => "Relationship map...",
            Self::OpenJoinKeys => "Join key finder...",
            Self::OpenJoinDiag => "Join diagnostics...",
            Self::OpenHarmonise => "Harmonise schemas...",
            Self::ToggleAskFilter => "Toggle Ask (plain-language filter)",
            Self::OpenUnion => "Union tables...",
            Self::OpenJoin => "Join tables",
            Self::OpenPartition => "Split into files...",
            Self::OpenOutliers => "Detect outliers...",
            Self::OpenPii => "Detect PII...",
            Self::FilterToMarked => "Filter to marked",
            Self::OpenQualityReport => "Data quality report...",
            Self::OpenRenameColumns => "Rename columns...",
            Self::AddBookmark => "Add bookmark...",
            Self::RenameActiveTab => "Rename tab...",
            Self::NewTabGroup => "New tab group...",
            Self::NextTabColour => "Next tab colour",
            Self::Ungroup => "Ungroup tabs",
            Self::TileSideBySide => "Tile tabs side by side",
            Self::TileStacked => "Tile tabs stacked",
            Self::TileGrid => "Tile tabs in a grid",
            Self::Untile => "Untile tabs",
            Self::OpenTableFolder => "Open table folder",
            Self::ListCloudInventory => "Run inventory on expanded cloud connection",
            Self::RunSqlOnServer => "Run SQL on server",
            Self::ToggleSqlComment => "Comment / uncomment SQL lines",
            Self::FormatSql => "Format SQL",
            Self::FitSqlResultColumns => "Auto-fit SQL result columns",
            Self::ExplainQuery => "Explain query plan",
            Self::AddSqlEditor => "New SQL editor",
            Self::OpenSqlFile => "Open SQL file...",
            Self::SaveSqlFileAs => "Save SQL file as...",
            Self::CopyAsInList => "Copy as IN list",
            Self::CopyAsJson => "Copy as JSON",
            Self::CopyAsInsert => "Copy as SQL INSERT statements",
            Self::FillSeries => "Fill series...",
            Self::FindCommand => "Find a command...",
            Self::OpenCohorts => "Cohorts...",
            Self::OpenFunnel => "Funnel...",
            Self::OpenRfm => "Customer segments (RFM)...",
            Self::ToggleColumnNavigator => "Toggle column navigator",
            Self::ToggleEditAudit => "Toggle edit audit trail",
            Self::ToggleRecipePanel => "Toggle recipe panel",
            Self::ApplyRecipe => "Apply recipe...",
            Self::ChooseRecipeKey => "Choose recipe ID column...",
            Self::OpenTabMemory => "Tab memory...",
            Self::AddCaretAbove => "Add a caret above",
            Self::AddCaretBelow => "Add a caret below",
            Self::CaretsAtLineEnds => "A caret at the end of each line",
        }
    }

    /// Default key combination shipped with the app.
    pub fn default_combo(self) -> KeyCombo {
        use egui::Key;
        match self {
            Self::NewFile => KeyCombo::ctrl(Key::N),
            Self::NewTable => KeyCombo::UNBOUND,
            Self::OpenFile => KeyCombo::ctrl(Key::O),
            Self::SaveFile => KeyCombo::ctrl(Key::S),
            Self::SaveDbSql => KeyCombo::UNBOUND,
            Self::ExportWorkbook => KeyCombo::UNBOUND,
            Self::OpenUrl => KeyCombo::UNBOUND,
            Self::OpenApiEndpoint => KeyCombo::UNBOUND,
            Self::SaveFileAs => KeyCombo::ctrl_shift(Key::S),
            Self::SaveWithPassword => KeyCombo::UNBOUND,
            Self::SaveToCloud => KeyCombo::UNBOUND,
            Self::ReloadFile => KeyCombo::ctrl(Key::R),
            Self::FocusSearch => KeyCombo::ctrl(Key::F),
            Self::ToggleFindReplace => KeyCombo::ctrl(Key::H),
            Self::CloseTab => KeyCombo::ctrl(Key::W),
            Self::QuitApp => KeyCombo::ctrl(Key::Q),
            Self::NextTab => KeyCombo::ctrl(Key::Tab),
            Self::PrevTab => KeyCombo::ctrl_shift(Key::Tab),
            Self::SelectAllRows => KeyCombo::ctrl(Key::A),
            Self::ZoomIn => KeyCombo::ctrl(Key::Plus),
            Self::ZoomOut => KeyCombo::ctrl(Key::Minus),
            Self::ZoomReset => KeyCombo::ctrl(Key::Num0),
            Self::LowercaseSelection => KeyCombo::ctrl_alt(Key::L),
            Self::UppercaseSelection => KeyCombo::ctrl_alt(Key::U),
            Self::EditCell => KeyCombo::plain(Key::F2),
            Self::GoToCell => KeyCombo::ctrl(Key::G),
            Self::DuplicateRow => KeyCombo::ctrl(Key::D),
            Self::DeleteRow => KeyCombo::ctrl_shift(Key::K),
            Self::InsertRowBelow => KeyCombo::ctrl_shift(Key::Enter),
            Self::ToggleSqlPanel => KeyCombo::ctrl(Key::J),
            Self::JumpFirstRow => KeyCombo::ctrl_shift(Key::ArrowUp),
            Self::JumpLastRow => KeyCombo::ctrl_shift(Key::ArrowDown),
            Self::JumpFirstCol => KeyCombo::ctrl_shift(Key::ArrowLeft),
            Self::JumpLastCol => KeyCombo::ctrl_shift(Key::ArrowRight),
            Self::ExtendSelectionUp => KeyCombo::ctrl(Key::ArrowUp),
            Self::ExtendSelectionDown => KeyCombo::ctrl(Key::ArrowDown),
            Self::ExtendSelectionLeft => KeyCombo::ctrl(Key::ArrowLeft),
            Self::ExtendSelectionRight => KeyCombo::ctrl(Key::ArrowRight),
            Self::ExportSqlResult => KeyCombo::ctrl_shift(Key::E),
            Self::Copy => KeyCombo::ctrl(Key::C),
            Self::Cut => KeyCombo::ctrl(Key::X),
            Self::Paste => KeyCombo::ctrl(Key::V),
            Self::Mark => KeyCombo::ctrl(Key::M),
            Self::Undo => KeyCombo::ctrl(Key::Z),
            Self::Redo => KeyCombo::ctrl(Key::Y),
            Self::OpenSettings => KeyCombo::plain(Key::F3),
            Self::OpenDocumentation => KeyCombo::plain(Key::F1),
            Self::OpenColumnFilter => KeyCombo::ctrl_shift(Key::F),
            Self::CycleViewMode => KeyCombo::plain(Key::F4),
            Self::ToggleReadOnly => KeyCombo::plain(Key::F8),
            Self::ToggleFollow => KeyCombo::UNBOUND,
            Self::OpenAutoRefresh => KeyCombo::UNBOUND,
            Self::FitAllColumns => KeyCombo::ctrl_shift(Key::W),
            Self::FitAllRows => KeyCombo::UNBOUND,
            Self::HideSelectedColumns | Self::HideSelectedRows | Self::ShowHiddenRows => {
                KeyCombo::UNBOUND
            }
            Self::ReopenLastClosedTab => KeyCombo::ctrl_shift(Key::T),
            Self::CompareSelectedTabs => KeyCombo::plain(Key::F9),
            Self::ColumnValueFrequency => KeyCombo::ctrl_shift(Key::I),
            Self::FindDuplicates => KeyCombo::ctrl_shift(Key::D),
            Self::OpenFuzzyDuplicates => KeyCombo::ctrl_shift(Key::U),
            // F7 (not Ctrl+Shift+X): on Linux/Windows egui receives an
            // OS-level `Event::Cut` for Ctrl+Shift+X, which collides
            // with `do_cut`. F-keys don't generate clipboard events.
            Self::ExportSchema => KeyCombo::plain(Key::F7),
            // Unbound: every clipboard-safe Ctrl+Shift letter is taken.
            Self::SaveTableToDb => KeyCombo::UNBOUND,
            // F6 mirrors the feature ID (v3 batch). Ctrl+Shift+F is
            // already the column-filter shortcut and Ctrl+F is the
            // active-tab search - both useful enough to keep.
            Self::MultiSearch => KeyCombo::plain(Key::F6),
            // F10 and Shift+F10 are the only free function keys: F1-F9 are
            // documentation, edit cell, settings, cycle view, chart,
            // multi-search, schema export, read-only and compare.
            Self::NextProblem => KeyCombo::plain(Key::F10),
            Self::PrevProblem => KeyCombo {
                key: Some(Key::F10),
                ctrl: false,
                shift: true,
                alt: false,
            },
            // F5 is the only free F-key in the (F4-F9) cluster after the
            // v3 batch; "refresh" semantics also map naturally to "re-plot".
            Self::OpenChart => KeyCombo::plain(Key::F5),
            // Ctrl+PageUp / Ctrl+PageDown - the spreadsheet convention
            // (Excel, LibreOffice) for paging through long tables.
            Self::ScrollPageUp => KeyCombo::ctrl(Key::PageUp),
            Self::ScrollPageDown => KeyCombo::ctrl(Key::PageDown),
            Self::ToggleChatPanel => KeyCombo::ctrl_shift(Key::A),
            Self::ToggleChatMaximise => KeyCombo::UNBOUND,
            // Newer feature dialogs. All Ctrl+Shift+<letter> on free letters
            // (the uniqueness test pins that none collide with the above).
            Self::OpenPivot => KeyCombo::ctrl_shift(Key::P),
            Self::OpenTransform => KeyCombo::ctrl_shift(Key::R),
            Self::OpenConditionalFormat => KeyCombo::ctrl_shift(Key::L),
            Self::OpenConditionalColumn => KeyCombo::ctrl_shift(Key::J),
            Self::OpenHashColumns => KeyCombo::UNBOUND,
            Self::OpenCalculatedColumn => KeyCombo::UNBOUND,
            Self::OpenBands => KeyCombo::UNBOUND,
            Self::OpenReferenceData => KeyCombo::UNBOUND,
            Self::OpenClassify => KeyCombo::UNBOUND,
            Self::OpenAnonymize => KeyCombo::ctrl_shift(Key::Y),
            Self::OpenValidation => KeyCombo::ctrl_shift(Key::G),
            Self::OpenMultiSort => KeyCombo::ctrl_shift(Key::O),
            Self::OpenSummary => KeyCombo::ctrl_shift(Key::M),
            Self::OpenNumberFormat => KeyCombo::ctrl_shift(Key::N),
            Self::CopyAsMarkdown => KeyCombo::ctrl_shift(Key::B),
            Self::OpenDedupe => KeyCombo::ctrl_shift(Key::H),
            Self::OpenJoin => KeyCombo::ctrl_shift(Key::Q),
            // Ctrl+Shift+Z is free here: undo is Ctrl+Z, redo is Ctrl+Y.
            Self::OpenPartition => KeyCombo::ctrl_shift(Key::Z),
            // The Ctrl+Shift+<letter> space is exhausted, so these use
            // Ctrl+Alt+<letter> (the text-case actions already live there:
            // Ctrl+Alt+L / Ctrl+Alt+U). Ctrl+Alt stays clear of the
            // clipboard-event trap that rules out Ctrl+Shift+C/X/V. Union uses
            // N ("u-N-ion") because Ctrl+Alt+U is UppercaseSelection.
            Self::OpenImpute => KeyCombo::ctrl_alt(Key::I),
            // Unbound: every clipboard-safe Ctrl+Shift letter is taken.
            Self::OpenRetypeColumn => KeyCombo::UNBOUND,
            Self::OpenUnion => KeyCombo::ctrl_alt(Key::N),
            Self::OpenOutliers => KeyCombo::ctrl_alt(Key::O),
            Self::OpenPii => KeyCombo::ctrl_alt(Key::P),
            Self::FilterToMarked => KeyCombo::ctrl_alt(Key::M),
            Self::OpenQualityReport => KeyCombo::ctrl_alt(Key::Q),
            Self::OpenRenameColumns => KeyCombo::ctrl_alt(Key::R),
            Self::AddBookmark => KeyCombo::ctrl_alt(Key::B),
            Self::RenameActiveTab => KeyCombo::ctrl_alt(Key::T),
            Self::NewTabGroup
            | Self::NextTabColour
            | Self::Ungroup
            | Self::TileSideBySide
            | Self::TileStacked
            | Self::TileGrid
            | Self::Untile => KeyCombo::UNBOUND,
            // Ctrl+Shift+G is taken (Data validation); every clipboard-safe
            // Ctrl+Shift letter is spoken for, so these ship unbound like
            // Impute/Union/Outliers/PII.
            Self::OpenTableFolder => KeyCombo::UNBOUND,
            Self::ListCloudInventory => KeyCombo::UNBOUND,
            Self::RunSqlOnServer => KeyCombo::UNBOUND,
            // Asked for on an F key; F12 was the free one (F11 is fullscreen
            // on many window managers).
            Self::ToggleSqlComment => KeyCombo::plain(Key::F12),
            // Ctrl+T, "new tab", for a new editor. Not Ctrl+Alt+letter: on
            // German and Polish layouts that is AltGr and types a character
            // (Ctrl+Alt+E is the euro sign) into the very editor.
            Self::AddSqlEditor => KeyCombo::ctrl(Key::T),
            Self::ToggleColumnNavigator => KeyCombo::UNBOUND,
            Self::ToggleEditAudit => KeyCombo::UNBOUND,
            Self::ToggleRecipePanel => KeyCombo::UNBOUND,
            Self::ApplyRecipe => KeyCombo::UNBOUND,
            Self::ChooseRecipeKey => KeyCombo::UNBOUND,
            Self::OpenTabMemory => KeyCombo::UNBOUND,
            // VS Code's keys on Linux. Not Ctrl+Alt+Up/Down: GNOME switches
            // workspaces on those.
            Self::AddCaretAbove => KeyCombo::shift_alt(Key::ArrowUp),
            Self::AddCaretBelow => KeyCombo::shift_alt(Key::ArrowDown),
            Self::CaretsAtLineEnds => KeyCombo::shift_alt(Key::I),
            // Every Ctrl+Shift letter is taken, and Ctrl+Shift+C / X / V fire
            // egui clipboard events on the table.
            Self::OpenCleanupPanel => KeyCombo::UNBOUND,
            Self::OpenTimeseries => KeyCombo::UNBOUND,
            // Everything the menus can reach is bindable, so a user can put
            // the entries they actually use on keys of their own. There is no
            // free default chord left to hand out, so these arrive unbound
            // and Settings -> Shortcuts is where they get one.
            Self::OpenCorrelation
            | Self::OpenLookups
            | Self::OpenCellHistory
            | Self::OpenBinaryViewer
            | Self::OpenDistCompare
            | Self::OpenReferential
            | Self::OpenTranspose
            | Self::OpenRowCompare
            | Self::OpenRandomSample
            | Self::LoadWholeTable
            | Self::OpenTidyUp
            | Self::OpenTimeCalc
            | Self::ExportPdf
            | Self::OpenDirectory
            | Self::OpenGitCompare
            | Self::ExplainFile
            | Self::OpenAiReport
            | Self::ToggleCloudBrowser
            | Self::ToggleDbBrowser
            | Self::ToggleSplitView
            | Self::ToggleSplitSideBySide
            | Self::AddSplitPane
            | Self::RemoveSplitPane
            | Self::FormatSql
            | Self::FitSqlResultColumns
            | Self::ExplainQuery
            | Self::OpenSqlFile
            | Self::SaveSqlFileAs
            | Self::CopyAsInList
            | Self::CopyAsJson
            | Self::CopyAsInsert
            | Self::FillSeries
            | Self::OpenCohorts
            | Self::OpenFunnel
            | Self::OpenRfm => KeyCombo::UNBOUND,
            // The one default for a new action that is bound (the user's call):
            // the command search is for people who do not know the keys.
            Self::FindCommand => KeyCombo::ctrl(Key::K),
            Self::OpenBatchConvert => KeyCombo::UNBOUND,
            Self::OpenSchemaDrift => KeyCombo::UNBOUND,
            Self::OpenMergeVersions => KeyCombo::UNBOUND,
            Self::OpenFileVersions => KeyCombo::UNBOUND,
            Self::OpenDbExport => KeyCombo::UNBOUND,
            Self::OpenTestData => KeyCombo::UNBOUND,
            Self::OpenReport => KeyCombo::UNBOUND,
            Self::OpenFuzzyJoin => KeyCombo::UNBOUND,
            Self::OpenFileInternals => KeyCombo::UNBOUND,
            Self::OpenDbCompare => KeyCombo::UNBOUND,
            Self::OpenFolderCompare => KeyCombo::UNBOUND,
            // No clipboard-safe Ctrl+Shift letter is left, so this ships unbound.
            Self::OpenDataDrift => KeyCombo::UNBOUND,
            Self::OpenRelMap => KeyCombo::UNBOUND,
            Self::OpenJoinKeys => KeyCombo::UNBOUND,
            Self::OpenJoinDiag => KeyCombo::UNBOUND,
            Self::OpenHarmonise => KeyCombo::UNBOUND,
            Self::ToggleAskFilter => KeyCombo::UNBOUND,
        }
    }
}

/// Categories used to group the shortcut list in the Settings dialog (and the
/// docs reference page). Each [`ShortcutAction`] reports its group via
/// [`ShortcutAction::group`]; the dialog renders one section per group in
/// [`ShortcutGroup::ALL`] order. Adding a new action just picks a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutGroup {
    File,
    Tabs,
    Search,
    Navigation,
    Selection,
    Editing,
    Clipboard,
    Marking,
    TextCase,
    Zoom,
    View,
    SqlPanel,
    TextEditors,
    Dialogs,
}

impl ShortcutGroup {
    /// Display order for the Settings dialog. Mirrors `docs/reference/shortcuts.md`.
    pub const ALL: &[ShortcutGroup] = &[
        Self::File,
        Self::Tabs,
        Self::Search,
        Self::Navigation,
        Self::Selection,
        Self::Editing,
        Self::Clipboard,
        Self::Marking,
        Self::TextCase,
        Self::Zoom,
        Self::View,
        Self::SqlPanel,
        Self::TextEditors,
        Self::Dialogs,
    ];

    /// i18n key for the group heading (resolved via `crate::i18n::t`).
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::File => "shortcut_group.file",
            Self::Tabs => "shortcut_group.tabs",
            Self::Search => "shortcut_group.search",
            Self::Navigation => "shortcut_group.navigation",
            Self::Selection => "shortcut_group.selection",
            Self::Editing => "shortcut_group.editing",
            Self::Clipboard => "shortcut_group.clipboard",
            Self::Marking => "shortcut_group.marking",
            Self::TextCase => "shortcut_group.text_case",
            Self::Zoom => "shortcut_group.zoom",
            Self::View => "shortcut_group.view",
            Self::SqlPanel => "shortcut_group.sql_panel",
            Self::TextEditors => "shortcut_group.text_editors",
            Self::Dialogs => "shortcut_group.dialogs",
        }
    }
}

impl ShortcutAction {
    /// Which Settings-dialog section this action belongs to. Mirrors the
    /// grouping in `docs/reference/shortcuts.md`.
    pub fn group(self) -> ShortcutGroup {
        use ShortcutGroup as G;
        match self {
            Self::NewFile
            | Self::NewTable
            | Self::OpenFile
            | Self::SaveFile
            | Self::SaveFileAs
            | Self::SaveWithPassword
            | Self::SaveToCloud
            | Self::SaveDbSql
            | Self::ExportWorkbook
            | Self::OpenUrl
            | Self::OpenApiEndpoint
            | Self::ExportSchema
            | Self::SaveTableToDb
            | Self::ReloadFile
            | Self::CloseTab
            | Self::ReopenLastClosedTab
            | Self::OpenTableFolder
            | Self::QuitApp
            | Self::ExportPdf
            | Self::OpenDirectory => G::File,
            Self::NextTab
            | Self::PrevTab
            | Self::RenameActiveTab
            | Self::NewTabGroup
            | Self::NextTabColour
            | Self::Ungroup
            | Self::TileSideBySide
            | Self::TileStacked
            | Self::TileGrid
            | Self::Untile => G::Tabs,
            Self::FocusSearch
            | Self::ToggleAskFilter
            | Self::ToggleFindReplace
            | Self::OpenColumnFilter
            | Self::MultiSearch
            | Self::OpenChart => G::Search,
            Self::GoToCell
            | Self::JumpFirstRow
            | Self::JumpLastRow
            | Self::JumpFirstCol
            | Self::JumpLastCol
            | Self::ScrollPageUp
            | Self::ScrollPageDown
            | Self::NextProblem
            | Self::PrevProblem => G::Navigation,
            Self::SelectAllRows
            | Self::ExtendSelectionUp
            | Self::ExtendSelectionDown
            | Self::ExtendSelectionLeft
            | Self::ExtendSelectionRight => G::Selection,
            Self::EditCell
            | Self::InsertRowBelow
            | Self::DuplicateRow
            | Self::DeleteRow
            | Self::Undo
            | Self::Redo => G::Editing,
            Self::Copy | Self::Cut | Self::Paste => G::Clipboard,
            Self::Mark | Self::FilterToMarked => G::Marking,
            Self::AddBookmark => G::Navigation,
            Self::UppercaseSelection | Self::LowercaseSelection => G::TextCase,
            Self::ZoomIn | Self::ZoomOut | Self::ZoomReset => G::Zoom,
            Self::CycleViewMode
            | Self::ToggleReadOnly
            | Self::ToggleFollow
            | Self::ToggleSqlPanel
            | Self::ToggleChatPanel
            | Self::ToggleChatMaximise
            | Self::FitAllColumns
            | Self::FitAllRows
            | Self::HideSelectedColumns
            | Self::HideSelectedRows
            | Self::ShowHiddenRows
            | Self::CompareSelectedTabs
            | Self::ToggleCloudBrowser
            | Self::ToggleDbBrowser
            | Self::ToggleSplitView
            | Self::ToggleSplitSideBySide
            | Self::AddSplitPane
            | Self::RemoveSplitPane
            | Self::ToggleColumnNavigator
            | Self::FindCommand => G::View,
            Self::ToggleEditAudit
            | Self::ToggleRecipePanel
            | Self::ApplyRecipe
            | Self::ChooseRecipeKey => G::Editing,
            Self::OpenTabMemory => G::Dialogs,
            Self::ExportSqlResult
            | Self::RunSqlOnServer
            | Self::ToggleSqlComment
            | Self::FormatSql
            | Self::FitSqlResultColumns
            | Self::ExplainQuery
            | Self::AddSqlEditor
            | Self::OpenSqlFile
            | Self::SaveSqlFileAs => G::SqlPanel,
            Self::AddCaretAbove | Self::AddCaretBelow | Self::CaretsAtLineEnds => G::TextEditors,
            Self::ListCloudInventory => G::Search,
            Self::OpenNumberFormat
            | Self::CopyAsMarkdown
            | Self::CopyAsInList
            | Self::CopyAsJson
            | Self::CopyAsInsert
            | Self::FillSeries => G::Editing,
            Self::OpenDocumentation
            | Self::OpenSettings
            | Self::ColumnValueFrequency
            | Self::OpenPivot
            | Self::OpenTransform
            | Self::OpenConditionalFormat
            | Self::OpenConditionalColumn
            | Self::OpenHashColumns
            | Self::OpenCalculatedColumn
            | Self::OpenBands
            | Self::OpenReferenceData
            | Self::OpenClassify
            | Self::OpenAnonymize
            | Self::OpenValidation
            | Self::OpenMultiSort
            | Self::OpenSummary
            | Self::FindDuplicates
            | Self::OpenFuzzyDuplicates
            | Self::OpenDedupe
            | Self::OpenImpute
            | Self::OpenRetypeColumn
            | Self::OpenCleanupPanel
            | Self::OpenTimeseries
            | Self::OpenCohorts
            | Self::OpenFunnel
            | Self::OpenRfm
            | Self::OpenBatchConvert
            | Self::OpenSchemaDrift
            | Self::OpenMergeVersions
            | Self::OpenFileVersions
            | Self::OpenDbExport
            | Self::OpenTestData
            | Self::OpenReport
            | Self::OpenAutoRefresh
            | Self::OpenFuzzyJoin
            | Self::OpenFileInternals
            | Self::OpenDbCompare
            | Self::OpenFolderCompare
            | Self::OpenDataDrift
            | Self::OpenRelMap
            | Self::OpenJoinKeys
            | Self::OpenJoinDiag
            | Self::OpenHarmonise
            | Self::OpenUnion
            | Self::OpenJoin
            | Self::OpenPartition
            | Self::OpenOutliers
            | Self::OpenPii
            | Self::OpenQualityReport
            | Self::OpenRenameColumns
            | Self::OpenCorrelation
            | Self::OpenLookups
            | Self::OpenCellHistory
            | Self::OpenBinaryViewer
            | Self::OpenDistCompare
            | Self::OpenReferential
            | Self::OpenTranspose
            | Self::OpenRowCompare
            | Self::OpenRandomSample
            | Self::LoadWholeTable
            | Self::OpenTidyUp
            | Self::OpenTimeCalc
            | Self::OpenGitCompare
            | Self::ExplainFile
            | Self::OpenAiReport => G::Dialogs,
        }
    }
}

/// Map of action -> binding. Missing entries fall back to the default combo,
/// so older settings files continue to pick up new actions automatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    #[serde(default)]
    pub bindings: HashMap<ShortcutAction, KeyCombo>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        use strum::IntoEnumIterator;
        let mut bindings = HashMap::new();
        for action in ShortcutAction::iter() {
            bindings.insert(action, action.default_combo());
        }
        Self { bindings }
    }
}

impl ShortcutAction {
    /// i18n key of the command's name. A command with a menu entry uses the
    /// menu's own key, so Find a command and Settings > Shortcuts name it
    /// exactly as the menu does; the rest have a `command.*` key.
    pub fn name_key(self) -> &'static str {
        match self {
            Self::NewFile => "file_menu.new_file",
            Self::NewTable => "file_menu.new_table",
            Self::OpenFile => "common.open",
            Self::SaveFile => "common.save",
            Self::SaveDbSql => "file_menu.save_sql",
            Self::ExportWorkbook => "file_menu.export_workbook",
            Self::OpenUrl => "file_menu.open_url",
            Self::OpenApiEndpoint => "file_menu.open_api",
            Self::SaveFileAs => "common.save_as",
            Self::SaveWithPassword => "file_menu.save_password",
            Self::SaveToCloud => "file_menu.save_cloud",
            Self::ReloadFile => "refresh.menu",
            Self::FocusSearch => "search_menu.find",
            Self::ToggleFindReplace => "search_menu.find_replace",
            Self::CloseTab => "command.close_tab",
            Self::QuitApp => "file_menu.exit",
            Self::NextTab => "command.next_tab",
            Self::PrevTab => "command.prev_tab",
            Self::SelectAllRows => "command.select_all_rows",
            Self::ZoomIn => "command.zoom_in",
            Self::ZoomOut => "command.zoom_out",
            Self::ZoomReset => "command.zoom_reset",
            Self::LowercaseSelection => "command.lowercase_selection",
            Self::UppercaseSelection => "command.uppercase_selection",
            Self::EditCell => "command.edit_cell",
            Self::GoToCell => "command.go_to_cell",
            Self::DuplicateRow => "command.duplicate_row",
            Self::DeleteRow => "edit_menu.delete_row",
            Self::InsertRowBelow => "edit_menu.insert_row",
            Self::ToggleSqlPanel => "analyse_menu.sql",
            Self::JumpFirstRow => "command.jump_first_row",
            Self::JumpLastRow => "command.jump_last_row",
            Self::JumpFirstCol => "command.jump_first_col",
            Self::JumpLastCol => "command.jump_last_col",
            Self::ExtendSelectionUp => "command.extend_selection_up",
            Self::ExtendSelectionDown => "command.extend_selection_down",
            Self::ExtendSelectionLeft => "command.extend_selection_left",
            Self::ExtendSelectionRight => "command.extend_selection_right",
            Self::ExportSqlResult => "sql.export",
            Self::Copy => "header.copy",
            Self::Cut => "header.cut",
            Self::Paste => "header.paste",
            Self::Mark => "edit_menu.mark",
            Self::Undo => "edit_menu.undo",
            Self::Redo => "edit_menu.redo",
            Self::OpenSettings => "help_menu.settings",
            Self::OpenDocumentation => "help_menu.documentation",
            Self::OpenColumnFilter => "columns_menu.filter",
            Self::CycleViewMode => "command.cycle_view_mode",
            Self::ToggleReadOnly => "view_menu.readonly",
            Self::ToggleFollow => "live.follow",
            Self::OpenAutoRefresh => "live.auto_refresh",
            Self::FitAllColumns => "edit_menu.fit_all_columns",
            Self::FitAllRows => "edit_menu.fit_all_rows",
            Self::HideSelectedColumns => "edit_menu.hide_columns",
            Self::HideSelectedRows => "edit_menu.hide_rows",
            Self::ShowHiddenRows => "edit_menu.show_hidden_rows",
            Self::ReopenLastClosedTab => "edit_menu.reopen_tab",
            Self::CompareSelectedTabs => "command.compare_selected_tabs",
            Self::ColumnValueFrequency => "analyse_menu.value_frequency",
            Self::FindDuplicates => "search_menu.find_duplicates",
            Self::OpenFuzzyDuplicates => "fuzzy_dup.menu",
            Self::ExportSchema => "file_menu.export_schema",
            Self::SaveTableToDb => "file_menu.save_to_db",
            Self::MultiSearch => "search_menu.multi_search",
            Self::NextProblem => "command.next_problem",
            Self::PrevProblem => "command.prev_problem",
            Self::OpenChart => "analyse_menu.chart",
            Self::ScrollPageUp => "command.scroll_page_up",
            Self::ScrollPageDown => "command.scroll_page_down",
            Self::ToggleChatPanel => "analyse_menu.assistant",
            Self::ToggleChatMaximise => "command.chat_maximise",
            Self::OpenPivot => "analyse_menu.pivot",
            Self::OpenTransform => "transform.menu",
            Self::OpenConditionalFormat => "edit_menu.conditional_format",
            Self::OpenConditionalColumn => "ccol.menu",
            Self::OpenHashColumns => "hashcols.menu",
            Self::OpenCalculatedColumn => "calc.menu",
            Self::OpenBands => "bands.menu",
            Self::OpenReferenceData => "refdata.menu",
            Self::OpenClassify => "classify.menu",
            Self::OpenAnonymize => "anonymize.menu",
            Self::OpenValidation => "edit_menu.validation",
            Self::OpenMultiSort => "analyse_menu.multi_sort",
            Self::OpenSummary => "analyse_menu.describe",
            Self::OpenNumberFormat => "edit_menu.number_format",
            Self::CopyAsMarkdown => "command.copy_as_markdown",
            Self::OpenDedupe => "command.open_dedupe",
            Self::OpenImpute => "impute.menu",
            Self::OpenRetypeColumn => "retype.menu_entry",
            Self::OpenCleanupPanel => "analyse_menu.cleanup",
            Self::OpenTimeseries => "analyse_menu.timeseries",
            Self::OpenCohorts => "analyse_menu.cohorts",
            Self::OpenFunnel => "analyse_menu.funnel",
            Self::OpenRfm => "analyse_menu.rfm",
            Self::OpenCorrelation => "analyse_menu.correlation",
            Self::OpenLookups => "analyse_menu.lookups",
            Self::OpenCellHistory => "context_menu.cell_history",
            Self::OpenBinaryViewer => "binview.menu",
            Self::OpenDistCompare => "distcmp.menu",
            Self::OpenReferential => "refint.menu",
            Self::OpenTranspose => "analyse_menu.transpose",
            Self::OpenRowCompare => "analyse_menu.row_compare",
            Self::OpenRandomSample => "analyse_menu.random_sample",
            Self::LoadWholeTable => "loadall.menu",
            Self::OpenTidyUp => "tidyup.menu",
            Self::OpenTimeCalc => "toolbar.time_calc",
            Self::ExportPdf => "file_menu.export_pdf",
            Self::OpenDirectory => "file_menu.open_directory",
            Self::OpenGitCompare => "view_menu.compare_git",
            Self::ExplainFile => "chat.explain",
            Self::OpenAiReport => "ai_report.menu",
            Self::ToggleCloudBrowser => "file_menu.cloud_connections",
            Self::ToggleDbBrowser => "file_menu.databases",
            Self::ToggleSplitView => "view_menu.split",
            Self::ToggleSplitSideBySide => "view_menu.split_side",
            Self::AddSplitPane => "view_menu.add_pane",
            Self::RemoveSplitPane => "view_menu.remove_pane",
            Self::OpenBatchConvert => "file_menu.batch_convert",
            Self::OpenSchemaDrift => "file_menu.schema_drift",
            Self::OpenMergeVersions => "file_menu.merge_versions",
            Self::OpenFileVersions => "file_menu.versions",
            Self::OpenDbExport => "command.open_db_export",
            Self::OpenTestData => "testdata.menu",
            Self::OpenReport => "file_menu.report",
            Self::OpenFuzzyJoin => "fuzzy_join.menu",
            Self::OpenFileInternals => "analyse_menu.file_internals",
            Self::OpenDbCompare => "analyse_menu.db_compare",
            Self::OpenFolderCompare => "analyse_menu.folder_compare",
            Self::OpenDataDrift => "datadrift.menu",
            Self::OpenRelMap => "relmap.menu",
            Self::OpenJoinKeys => "analyse_menu.join_keys",
            Self::OpenJoinDiag => "analyse_menu.join_diag",
            Self::OpenHarmonise => "file_menu.harmonise",
            Self::ToggleAskFilter => "command.toggle_ask_filter",
            Self::OpenUnion => "union.menu",
            Self::OpenJoin => "join.menu",
            Self::OpenPartition => "partition.menu",
            Self::OpenOutliers => "outliers.menu",
            Self::OpenPii => "pii.menu",
            Self::FilterToMarked => "edit_menu.filter_to_marked",
            Self::OpenQualityReport => "analyse_menu.quality",
            Self::OpenRenameColumns => "edit_menu.rename_columns",
            Self::AddBookmark => "bookmarks.add",
            Self::RenameActiveTab => "context_menu.rename_tab",
            Self::NewTabGroup => "tabgroup.new_group",
            Self::NextTabColour => "tabcolour.next",
            Self::Ungroup => "tabgroup.ungroup",
            Self::TileSideBySide => "tiles.cmd_side_by_side",
            Self::TileStacked => "tiles.cmd_stacked",
            Self::TileGrid => "tiles.cmd_grid",
            Self::Untile => "tiles.cmd_untile",
            Self::OpenTableFolder => "file_menu.open_table_folder",
            Self::ListCloudInventory => "command.list_cloud_inventory",
            Self::RunSqlOnServer => "command.run_sql_on_server",
            Self::ToggleSqlComment => "command.toggle_sql_comment",
            Self::FormatSql => "sql.format",
            Self::FitSqlResultColumns => "sql.fit_columns",
            Self::ExplainQuery => "explain.button",
            Self::AddSqlEditor => "command.add_sql_editor",
            Self::OpenSqlFile => "command.open_sql_file",
            Self::SaveSqlFileAs => "command.save_sql_file_as",
            Self::CopyAsInList => "command.copy_as_in_list",
            Self::CopyAsJson => "command.copy_as_json",
            Self::CopyAsInsert => "command.copy_as_insert",
            Self::FillSeries => "edit_menu.fill_series",
            Self::FindCommand => "help_menu.find_command",
            Self::ToggleColumnNavigator => "view_menu.column_navigator",
            Self::ToggleEditAudit => "edit_menu.edit_audit",
            Self::ToggleRecipePanel => "edit_menu.recipe_panel",
            Self::ApplyRecipe => "edit_menu.apply_recipe",
            Self::ChooseRecipeKey => "recipe.choose_key",
            Self::OpenTabMemory => "tabmem.menu",
            Self::AddCaretAbove => "command.add_caret_above",
            Self::AddCaretBelow => "command.add_caret_below",
            Self::CaretsAtLineEnds => "command.carets_at_line_ends",
        }
    }

    /// The command's name in the user's language.
    pub fn display_name(self) -> String {
        crate::i18n::t(self.name_key())
    }

    /// Where the command lives: the top menu holding it (a `menu.*` key),
    /// else its shortcut group.
    pub fn place_key(self) -> &'static str {
        match self {
            Self::NewFile
            | Self::NewTable
            | Self::OpenFile
            | Self::SaveFile
            | Self::SaveDbSql
            | Self::ExportWorkbook
            | Self::OpenUrl
            | Self::OpenApiEndpoint
            | Self::SaveFileAs
            | Self::SaveWithPassword
            | Self::SaveToCloud
            | Self::QuitApp
            | Self::ExportSchema
            | Self::SaveTableToDb
            | Self::ExportPdf
            | Self::OpenDirectory
            | Self::ToggleCloudBrowser
            | Self::ToggleDbBrowser
            | Self::OpenBatchConvert
            | Self::OpenSchemaDrift
            | Self::OpenMergeVersions
            | Self::OpenFileVersions
            | Self::OpenReport
            | Self::OpenHarmonise
            | Self::OpenTableFolder => "menu.file",
            Self::DeleteRow
            | Self::InsertRowBelow
            | Self::Mark
            | Self::Undo
            | Self::Redo
            | Self::FitAllColumns
            | Self::FitAllRows
            | Self::HideSelectedRows
            | Self::ShowHiddenRows
            | Self::ReopenLastClosedTab
            | Self::CopyAsMarkdown
            | Self::CopyAsInList
            | Self::CopyAsJson
            | Self::CopyAsInsert
            | Self::FillSeries
            | Self::ToggleEditAudit
            | Self::ToggleRecipePanel
            | Self::ApplyRecipe => "menu.edit",
            Self::OpenColumnFilter
            | Self::OpenConditionalFormat
            | Self::OpenConditionalColumn
            | Self::OpenHashColumns
            | Self::OpenCalculatedColumn
            | Self::OpenBands
            | Self::OpenReferenceData
            | Self::OpenClassify
            | Self::OpenNumberFormat
            | Self::OpenRetypeColumn
            | Self::OpenRenameColumns
            | Self::HideSelectedColumns
            | Self::ToggleColumnNavigator => "menu.columns",
            Self::FindDuplicates
            | Self::OpenFuzzyDuplicates
            | Self::OpenTransform
            | Self::OpenAnonymize
            | Self::OpenValidation
            | Self::OpenMultiSort
            | Self::OpenImpute
            | Self::LoadWholeTable
            | Self::OpenTidyUp
            | Self::OpenTimeCalc
            | Self::OpenTestData
            | Self::OpenFuzzyJoin
            | Self::OpenUnion
            | Self::OpenJoin
            | Self::OpenPartition
            | Self::FilterToMarked
            | Self::AddBookmark
            | Self::OpenTabMemory => "menu.data",
            Self::ToggleSqlPanel
            | Self::ColumnValueFrequency
            | Self::OpenChart
            | Self::ToggleChatPanel
            | Self::ToggleChatMaximise
            | Self::OpenPivot
            | Self::OpenSummary
            | Self::OpenCleanupPanel
            | Self::OpenTimeseries
            | Self::OpenCohorts
            | Self::OpenFunnel
            | Self::OpenRfm
            | Self::OpenCorrelation
            | Self::OpenLookups
            | Self::OpenDistCompare
            | Self::OpenReferential
            | Self::OpenTranspose
            | Self::OpenRowCompare
            | Self::OpenRandomSample
            | Self::ExplainFile
            | Self::OpenFileInternals
            | Self::OpenDbCompare
            | Self::OpenFolderCompare
            | Self::OpenDataDrift
            | Self::OpenRelMap
            | Self::OpenJoinKeys
            | Self::OpenJoinDiag
            | Self::OpenOutliers
            | Self::OpenPii
            | Self::OpenQualityReport => "menu.analyse",
            Self::OpenSettings | Self::OpenDocumentation | Self::OpenAiReport => "menu.help",
            Self::FocusSearch | Self::ToggleFindReplace | Self::MultiSearch | Self::FindCommand => {
                "menu.search"
            }
            Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomReset
            | Self::CycleViewMode
            | Self::ToggleReadOnly
            | Self::ToggleFollow
            | Self::OpenAutoRefresh
            | Self::OpenGitCompare
            | Self::ToggleSplitView
            | Self::ToggleSplitSideBySide
            | Self::AddSplitPane
            | Self::RemoveSplitPane
            | Self::TileSideBySide
            | Self::TileStacked
            | Self::TileGrid
            | Self::Untile => "menu.view",
            _ => self.group().i18n_key(),
        }
    }

    /// Listed in Find a command. The caret commands are read off the event
    /// list by the editors (`multi_cursor`), not through `triggered`.
    pub fn in_command_search(self) -> bool {
        !matches!(
            self,
            Self::FindCommand | Self::AddCaretAbove | Self::AddCaretBelow | Self::CaretsAtLineEnds
        )
    }
}

impl Shortcuts {
    pub fn combo(&self, action: ShortcutAction) -> KeyCombo {
        self.bindings
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_combo())
    }

    /// Whether `action` fires this frame: its key was pressed, or Find a
    /// command picked it (see [`queue_command`]).
    pub fn triggered(&self, action: ShortcutAction, input: &egui::InputState) -> bool {
        firing(action) || self.combo(action).triggered(input)
    }

    pub fn set(&mut self, action: ShortcutAction, combo: KeyCombo) {
        self.bindings.insert(action, combo);
    }

    pub fn reset(&mut self, action: ShortcutAction) {
        self.bindings.insert(action, action.default_combo());
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
