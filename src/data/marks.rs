//! Colour marking: the [`MarkColor`] palette, the [`MarkKey`] target key, and
//! the [`DataTable`] methods that set/clear/query marks. Split out of
//! `data/mod.rs`; the mark-shifting logic that runs during structural row/column
//! edits stays with those methods in the core module.

use super::{DataTable, UndoAction};

/// Available highlight colors for marking cells, rows, and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MarkColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Teal,
    Lime,
    Indigo,
    Brown,
    Grey,
}

impl MarkColor {
    pub const ALL: &'static [MarkColor] = &[
        MarkColor::Red,
        MarkColor::Orange,
        MarkColor::Yellow,
        MarkColor::Green,
        MarkColor::Blue,
        MarkColor::Purple,
        MarkColor::Pink,
        MarkColor::Teal,
        MarkColor::Lime,
        MarkColor::Indigo,
        MarkColor::Brown,
        MarkColor::Grey,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MarkColor::Red => "Red",
            MarkColor::Orange => "Orange",
            MarkColor::Yellow => "Yellow",
            MarkColor::Green => "Green",
            MarkColor::Blue => "Blue",
            MarkColor::Purple => "Purple",
            MarkColor::Pink => "Pink",
            MarkColor::Teal => "Teal",
            MarkColor::Lime => "Lime",
            MarkColor::Indigo => "Indigo",
            MarkColor::Brown => "Brown",
            MarkColor::Grey => "Grey",
        }
    }

    /// i18n key for this color's display name (`color.red`, ...). The English
    /// fallback in `label()` is kept for non-UI uses; UI call sites use
    /// [`label_t`](Self::label_t) so the name follows the chosen language.
    pub fn label_key(&self) -> &'static str {
        match self {
            MarkColor::Red => "color.red",
            MarkColor::Orange => "color.orange",
            MarkColor::Yellow => "color.yellow",
            MarkColor::Green => "color.green",
            MarkColor::Blue => "color.blue",
            MarkColor::Purple => "color.purple",
            MarkColor::Pink => "color.pink",
            MarkColor::Teal => "color.teal",
            MarkColor::Lime => "color.lime",
            MarkColor::Indigo => "color.indigo",
            MarkColor::Brown => "color.brown",
            MarkColor::Grey => "color.grey",
        }
    }

    /// The colour as RGB: the one palette the screen fill, the swatch, the
    /// PDF and the Excel fill are all made from.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            MarkColor::Red => (220, 38, 38),
            MarkColor::Orange => (234, 88, 12),
            MarkColor::Yellow => (250, 204, 21),
            MarkColor::Green => (34, 197, 94),
            MarkColor::Blue => (59, 130, 246),
            MarkColor::Purple => (168, 85, 247),
            MarkColor::Pink => (236, 72, 153),
            MarkColor::Teal => (20, 184, 166),
            MarkColor::Lime => (132, 204, 22),
            MarkColor::Indigo => (99, 102, 241),
            MarkColor::Brown => (146, 64, 14),
            MarkColor::Grey => (107, 114, 128),
        }
    }

    /// Translated display name for the active UI language.
    pub fn label_t(&self) -> String {
        crate::i18n::t(self.label_key())
    }

    /// Whether this mark needs dark text on top to stay readable. Yellow's
    /// and lime's backgrounds are too pale for white text; the rest are saturated enough
    /// that white reads cleanly. Used in the rainbow easter-egg theme where
    /// the normal text colour cycles through hues and would otherwise crash
    /// into the mark fill at unpredictable moments.
    pub fn needs_dark_text(self) -> bool {
        matches!(self, MarkColor::Yellow | MarkColor::Lime)
    }
}

/// Key identifying what is marked (cell, row, or column).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MarkKey {
    Cell(usize, usize),
    Row(usize),
    Column(usize),
}

impl DataTable {
    /// Set a color mark on a cell, row, or column.
    pub fn set_mark(&mut self, key: MarkKey, color: MarkColor) {
        let old_color = self.marks.get(&key).copied();
        self.undo_stack.push(UndoAction::SetMark {
            key: key.clone(),
            old_color,
            new_color: Some(color),
        });
        self.redo_stack.clear();
        self.marks.insert(key, color);
    }

    /// Remove a color mark.
    pub fn clear_mark(&mut self, key: MarkKey) {
        let old_color = self.marks.get(&key).copied();
        if old_color.is_some() {
            self.undo_stack.push(UndoAction::SetMark {
                key: key.clone(),
                old_color,
                new_color: None,
            });
            self.redo_stack.clear();
            self.marks.remove(&key);
        }
    }

    /// Remove every color mark on the table. Pushes one `SetMark` undo
    /// entry per cleared key so the operation is undoable one mark at a
    /// time - that matches the granularity of the per-key clear path
    /// without bloating the `UndoAction` enum with a new variant.
    pub fn clear_all_marks(&mut self) {
        if self.marks.is_empty() {
            return;
        }
        let entries: Vec<(MarkKey, MarkColor)> = self.marks.drain().collect();
        for (key, old_color) in entries {
            self.undo_stack.push(UndoAction::SetMark {
                key,
                old_color: Some(old_color),
                new_color: None,
            });
        }
        self.redo_stack.clear();
    }

    /// Get the effective mark color for a cell (cell mark > row mark > column mark).
    pub fn get_mark_color(&self, row: usize, col: usize) -> Option<MarkColor> {
        if let Some(&c) = self.marks.get(&MarkKey::Cell(row, col)) {
            return Some(c);
        }
        if let Some(&c) = self.marks.get(&MarkKey::Row(row)) {
            return Some(c);
        }
        if let Some(&c) = self.marks.get(&MarkKey::Column(col)) {
            return Some(c);
        }
        None
    }
}

#[cfg(test)]
#[path = "marks_tests.rs"]
mod tests;
