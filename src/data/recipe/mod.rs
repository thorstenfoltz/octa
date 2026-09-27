//! Recipes: the steps done to a table, recorded by column **name** so they
//! can be replayed on next month's file.
//!
//! The undo log cannot serve here: it stores row and column *indices*
//! ("cell 5,3 changed"), which mean nothing on another file. A recipe stores
//! intent instead ("rename `Kunde` to `customer`", "drop duplicates on `id`").
//!
//! Saved as `.ocp` (Octa reCiPe), plain TOML so any editor can read it:
//!
//! ```toml
//! version = 1
//!
//! [[steps]]
//! step = "rename"
//! renames = [{ from = "Kunde", to = "customer" }]
//!
//! [[steps]]
//! step = "drop_duplicates"
//! columns = ["id"]
//! keep = "first"
//! ```
//!
//! One file per step kind under `steps/` (the modular-sets rule). Every step
//! mutates through `DataTable`'s own undoable primitives, so the GUI can fold
//! a whole replay into one undo entry with `coalesce_undo_since`; the CLI and
//! MCP simply ignore the undo stack.

use serde::{Deserialize, Serialize};

use crate::data::DataTable;

pub mod steps;

pub use steps::{
    CellEdit, ChangeType, DeleteColumns, DropDuplicates, Extract, Fill, FillMissing, Merge, Rename,
    RenamePair, RepairEncoding, Replace, SetCells, Sort, SortKey, Split, TidyId,
    duplicate_key_rows, guess_row_key,
};

/// The file extension, without the dot: **O**cta re**C**i**P**e.
pub const EXTENSION: &str = "ocp";

/// The format version written into every file, so a later Octa can tell an
/// old recipe from a new one.
pub const VERSION: u32 = 1;

/// One recorded step. Tagged `step = "..."` in the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum RecipeStep {
    Rename(Rename),
    DeleteColumns(DeleteColumns),
    ChangeType(ChangeType),
    Sort(Sort),
    DropDuplicates(DropDuplicates),
    FillMissing(FillMissing),
    Split(Split),
    Merge(Merge),
    Fill(Fill),
    Extract(Extract),
    Replace(Replace),
    RepairEncoding(RepairEncoding),
    SetCells(SetCells),
    TidyId(TidyId),
}

/// Dispatch one call to whichever step struct a variant holds.
macro_rules! each_step {
    ($self:ident, $s:ident => $body:expr) => {
        match $self {
            RecipeStep::Rename($s) => $body,
            RecipeStep::DeleteColumns($s) => $body,
            RecipeStep::ChangeType($s) => $body,
            RecipeStep::Sort($s) => $body,
            RecipeStep::DropDuplicates($s) => $body,
            RecipeStep::FillMissing($s) => $body,
            RecipeStep::Split($s) => $body,
            RecipeStep::Merge($s) => $body,
            RecipeStep::Fill($s) => $body,
            RecipeStep::Extract($s) => $body,
            RecipeStep::Replace($s) => $body,
            RecipeStep::RepairEncoding($s) => $body,
            RecipeStep::SetCells($s) => $body,
            RecipeStep::TidyId($s) => $body,
        }
    };
}

impl RecipeStep {
    /// Run the step. Errors name what no longer fits (a missing column, a
    /// broken pattern) and leave the table as it was.
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        // Engines read `rows`; pending edits must be part of what they see.
        table.apply_edits();
        each_step!(self, s => s.apply(table))
    }

    /// One plain sentence for the recipe panel and the apply preview.
    pub fn describe(&self) -> String {
        each_step!(self, s => s.describe())
    }

    /// Whether every column the step names exists in `table`. The apply
    /// preview uses it to warn before anything runs.
    pub fn missing_columns(&self, table: &DataTable) -> Vec<String> {
        each_step!(self, s => s.columns())
            .into_iter()
            .filter(|c| !table.columns.iter().any(|ci| &ci.name == c))
            .collect()
    }
}

/// A whole recipe, as saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub version: u32,
    #[serde(default)]
    pub steps: Vec<RecipeStep>,
}

impl Recipe {
    pub fn new(steps: Vec<RecipeStep>) -> Self {
        Self {
            version: VERSION,
            steps,
        }
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        let r: Recipe = toml::from_str(text)?;
        if r.version > VERSION {
            anyhow::bail!(
                "this recipe was written by a newer Octa (format {}, this one reads up to {VERSION})",
                r.version
            );
        }
        Ok(r)
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        Self::from_toml(&text).map_err(|e| anyhow::anyhow!("{}: {e:#}", path.display()))
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(path, self.to_toml()?)
            .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))
    }
}

/// What happened to one step during a replay.
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    pub description: String,
    /// `None` when the step ran; the reason when it was skipped.
    pub error: Option<String>,
}

/// Replay `recipe` on `table`, step by step. A step that cannot run (its
/// column is gone) is **skipped and reported**, not fatal: the rest of the
/// recipe usually still applies, and every surface shows which steps did not.
pub fn apply_recipe(table: &mut DataTable, recipe: &Recipe) -> Vec<StepOutcome> {
    recipe
        .steps
        .iter()
        .map(|step| StepOutcome {
            description: step.describe(),
            error: step.apply(table).err().map(|e| format!("{e:#}")),
        })
        .collect()
}

/// Index of the column named `name`, or an error naming it.
pub(crate) fn col(table: &DataTable, name: &str) -> anyhow::Result<usize> {
    table
        .columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("column `{name}` not found"))
}

/// `base`, or `base_2`, `base_3`, ... when a column already has the name.
pub fn unique_name(cols: &[String], base: &str) -> String {
    if !cols.iter().any(|c| c == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !cols.contains(c))
        .expect("an unbounded range always finds a free name")
}

/// A 1-based position typed by the user, or `default` when there is none or
/// it is out of range for `col_count` columns.
pub fn insert_index(position: Option<usize>, default: usize, col_count: usize) -> usize {
    position
        .filter(|v| (1..=col_count + 1).contains(v))
        .map(|v| v - 1)
        .unwrap_or(default)
}

pub(crate) fn names(table: &DataTable) -> Vec<String> {
    table.columns.iter().map(|c| c.name.clone()).collect()
}

#[cfg(test)]
#[path = "recipe_tests.rs"]
mod tests;
