use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::dedupe::{KeepWhich, dedupe_dropped_indices};
use crate::data::recipe::col;
use crate::i18n::t;

/// Drop repeated rows. Empty `columns` compares whole rows. `keep` is
/// `first` or `last`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropDuplicates {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default = "first")]
    pub keep: String,
}

fn first() -> String {
    "first".to_string()
}

impl DropDuplicates {
    pub fn new(columns: Vec<String>, keep: KeepWhich) -> Self {
        let keep = match keep {
            KeepWhich::First => "first",
            KeepWhich::Last => "last",
        };
        Self {
            columns,
            keep: keep.to_string(),
        }
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let keys: Vec<usize> = self
            .columns
            .iter()
            .map(|c| col(table, c))
            .collect::<anyhow::Result<_>>()?;
        let keep = match self.keep.to_ascii_lowercase().as_str() {
            "first" => KeepWhich::First,
            "last" => KeepWhich::Last,
            other => anyhow::bail!("keep must be `first` or `last`, not `{other}`"),
        };
        // Highest index first, so the indices still ahead stay valid.
        for r in dedupe_dropped_indices(table, &keys, keep) {
            table.delete_row(r);
        }
        table.structural_changes = true;
        Ok(())
    }

    pub fn describe(&self) -> String {
        if self.columns.is_empty() {
            t("recipe.step_dedupe_rows")
        } else {
            t("recipe.step_dedupe").replace("{columns}", &self.columns.join(", "))
        }
    }

    pub fn columns(&self) -> Vec<String> {
        self.columns.clone()
    }
}
