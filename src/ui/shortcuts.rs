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

use std::collections::HashMap;
use std::fmt::Write as _;

use eframe::egui;
use serde::{Deserialize, Serialize};

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
    pub fn triggered(&self, input: &egui::InputState) -> bool {
        let Some(key) = self.key else {
            return false;
        };
        let m = input.modifiers;
        // `command` collapses Cmd on mac and Ctrl elsewhere.
        if m.command != self.ctrl {
            return false;
        }
        if m.shift != self.shift {
            return false;
        }
        if m.alt != self.alt {
            return false;
        }
        input.key_pressed(key)
    }

    /// Human-readable label like "Ctrl+Shift+P" or "(none)".
    pub fn label(&self) -> String {
        let Some(key) = self.key else {
            return "(none)".to_string();
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
        _ => "?",
    }
}

/// All application actions that can be bound to a shortcut.
/// Every variant listed here is honored by the global key handler in `main.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumIter)]
pub enum ShortcutAction {
    NewFile,
    OpenFile,
    SaveFile,
    SaveFileAs,
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
    /// Resize every column in the active table to its best-fit width
    /// (header + content sampled up to 5000 rows). Same algorithm used
    /// by double-clicking the header seam between two columns, applied
    /// across the whole table.
    FitAllColumns,
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
    /// reachable via **Search -> Find duplicates...**.
    FindDuplicates,
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
    /// Open the Data-validation dialog. Also **Edit -> Data validation...**.
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
}

impl ShortcutAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::OpenFile => "Open file",
            Self::SaveFile => "Save file",
            Self::SaveFileAs => "Save file as...",
            Self::ReloadFile => "Reload file from disk",
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
            Self::Mark => "Mark selection (default color)",
            Self::Undo => "Undo last change",
            Self::Redo => "Redo last undone change",
            Self::OpenSettings => "Open settings",
            Self::OpenDocumentation => "Open documentation",
            Self::OpenColumnFilter => "Open column filter",
            Self::CycleViewMode => "Cycle view mode",
            Self::ToggleReadOnly => "Toggle read-only mode",
            Self::FitAllColumns => "Auto-fit all columns",
            Self::ReopenLastClosedTab => "Reopen last closed tab",
            Self::CompareSelectedTabs => "Compare selected tabs",
            Self::ColumnValueFrequency => "Show column value frequency",
            Self::FindDuplicates => "Find duplicate rows...",
            Self::ExportSchema => "Export schema...",
            Self::MultiSearch => "Open multi-search panel",
            Self::OpenChart => "Open chart tab",
            Self::ScrollPageUp => "Scroll up one page",
            Self::ScrollPageDown => "Scroll down one page",
            Self::ToggleChatPanel => "Toggle chat assistant panel",
            Self::OpenPivot => "Pivot / Unpivot...",
            Self::OpenTransform => "Transform column...",
            Self::OpenConditionalFormat => "Conditional formatting...",
            Self::OpenConditionalColumn => "Conditional column...",
            Self::OpenValidation => "Data validation...",
            Self::OpenMultiSort => "Sort by columns...",
            Self::OpenSummary => "Summary tab",
            Self::OpenNumberFormat => "Number format...",
            Self::CopyAsMarkdown => "Copy as Markdown table",
        }
    }

    /// Default key combination shipped with the app.
    pub fn default_combo(self) -> KeyCombo {
        use egui::Key;
        match self {
            Self::NewFile => KeyCombo::ctrl(Key::N),
            Self::OpenFile => KeyCombo::ctrl(Key::O),
            Self::SaveFile => KeyCombo::ctrl(Key::S),
            Self::SaveFileAs => KeyCombo::ctrl_shift(Key::S),
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
            Self::FitAllColumns => KeyCombo::ctrl_shift(Key::W),
            Self::ReopenLastClosedTab => KeyCombo::ctrl_shift(Key::T),
            Self::CompareSelectedTabs => KeyCombo::plain(Key::F9),
            Self::ColumnValueFrequency => KeyCombo::ctrl_shift(Key::I),
            Self::FindDuplicates => KeyCombo::ctrl_shift(Key::D),
            // F7 (not Ctrl+Shift+X): on Linux/Windows egui receives an
            // OS-level `Event::Cut` for Ctrl+Shift+X, which collides
            // with `do_cut`. F-keys don't generate clipboard events.
            Self::ExportSchema => KeyCombo::plain(Key::F7),
            // F6 mirrors the feature ID (v3 batch). Ctrl+Shift+F is
            // already the column-filter shortcut and Ctrl+F is the
            // active-tab search - both useful enough to keep.
            Self::MultiSearch => KeyCombo::plain(Key::F6),
            // F5 is the only free F-key in the (F4-F9) cluster after the
            // v3 batch; "refresh" semantics also map naturally to "re-plot".
            Self::OpenChart => KeyCombo::plain(Key::F5),
            // Ctrl+PageUp / Ctrl+PageDown - the spreadsheet convention
            // (Excel, LibreOffice) for paging through long tables.
            Self::ScrollPageUp => KeyCombo::ctrl(Key::PageUp),
            Self::ScrollPageDown => KeyCombo::ctrl(Key::PageDown),
            Self::ToggleChatPanel => KeyCombo::ctrl_shift(Key::A),
            // Newer feature dialogs. All Ctrl+Shift+<letter> on free letters
            // (the uniqueness test pins that none collide with the above).
            Self::OpenPivot => KeyCombo::ctrl_shift(Key::P),
            Self::OpenTransform => KeyCombo::ctrl_shift(Key::R),
            Self::OpenConditionalFormat => KeyCombo::ctrl_shift(Key::L),
            Self::OpenConditionalColumn => KeyCombo::ctrl_shift(Key::J),
            Self::OpenValidation => KeyCombo::ctrl_shift(Key::G),
            Self::OpenMultiSort => KeyCombo::ctrl_shift(Key::O),
            Self::OpenSummary => KeyCombo::ctrl_shift(Key::M),
            Self::OpenNumberFormat => KeyCombo::ctrl_shift(Key::N),
            Self::CopyAsMarkdown => KeyCombo::ctrl_shift(Key::B),
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
            | Self::OpenFile
            | Self::SaveFile
            | Self::SaveFileAs
            | Self::ExportSchema
            | Self::ReloadFile
            | Self::CloseTab
            | Self::ReopenLastClosedTab
            | Self::QuitApp => G::File,
            Self::NextTab | Self::PrevTab => G::Tabs,
            Self::FocusSearch
            | Self::ToggleFindReplace
            | Self::OpenColumnFilter
            | Self::FindDuplicates
            | Self::MultiSearch
            | Self::OpenChart => G::Search,
            Self::GoToCell
            | Self::JumpFirstRow
            | Self::JumpLastRow
            | Self::JumpFirstCol
            | Self::JumpLastCol
            | Self::ScrollPageUp
            | Self::ScrollPageDown => G::Navigation,
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
            Self::Mark => G::Marking,
            Self::UppercaseSelection | Self::LowercaseSelection => G::TextCase,
            Self::ZoomIn | Self::ZoomOut | Self::ZoomReset => G::Zoom,
            Self::CycleViewMode
            | Self::ToggleReadOnly
            | Self::ToggleSqlPanel
            | Self::ToggleChatPanel
            | Self::FitAllColumns
            | Self::CompareSelectedTabs => G::View,
            Self::ExportSqlResult => G::SqlPanel,
            Self::OpenNumberFormat | Self::CopyAsMarkdown => G::Editing,
            Self::OpenDocumentation
            | Self::OpenSettings
            | Self::ColumnValueFrequency
            | Self::OpenPivot
            | Self::OpenTransform
            | Self::OpenConditionalFormat
            | Self::OpenConditionalColumn
            | Self::OpenValidation
            | Self::OpenMultiSort
            | Self::OpenSummary => G::Dialogs,
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

impl Shortcuts {
    pub fn combo(&self, action: ShortcutAction) -> KeyCombo {
        self.bindings
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_combo())
    }

    pub fn triggered(&self, action: ShortcutAction, input: &egui::InputState) -> bool {
        self.combo(action).triggered(input)
    }

    pub fn set(&mut self, action: ShortcutAction, combo: KeyCombo) {
        self.bindings.insert(action, combo);
    }

    pub fn reset(&mut self, action: ShortcutAction) {
        self.bindings.insert(action, action.default_combo());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// No two actions may ship with the same default key combination, otherwise
    /// one of them would be silently unreachable until rebound. (Unbound
    /// defaults are exempt - several actions can legitimately ship with no key.)
    #[test]
    fn default_combos_are_unique() {
        let mut seen: HashMap<KeyCombo, ShortcutAction> = HashMap::new();
        for action in ShortcutAction::iter() {
            let combo = action.default_combo();
            if combo == KeyCombo::UNBOUND {
                continue;
            }
            if let Some(prev) = seen.insert(combo, action) {
                panic!(
                    "default key {} is bound to both {:?} and {:?}",
                    combo.label(),
                    prev,
                    action
                );
            }
        }
    }

    /// Every action must report a label and a group (the matches are
    /// exhaustive, so this mostly guards the `strum` iterator wiring).
    #[test]
    fn every_action_has_label_and_group() {
        for action in ShortcutAction::iter() {
            assert!(!action.label().is_empty());
            let _ = action.group();
        }
    }
}
