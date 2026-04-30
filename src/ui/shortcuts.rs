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
}

impl ShortcutAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::OpenFile => "Open file",
            Self::SaveFile => "Save file",
            Self::SaveFileAs => "Save file as…",
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
        }
    }
}

/// Map of action → binding. Missing entries fall back to the default combo,
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
