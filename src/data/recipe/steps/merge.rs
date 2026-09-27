use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::steps::insert_filled;
use crate::data::recipe::{col, insert_index, names, unique_name};
use crate::data::transform::merge_columns;
use crate::i18n::t;

/// Join several columns into one new text column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Merge {
    pub columns: Vec<String>,
    #[serde(default)]
    pub separator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub new_name: String,
    /// 1-based position of the new column. None: at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
}

impl Merge {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        if self.columns.len() < 2 {
            anyhow::bail!("merging needs at least two columns");
        }
        let cols: Vec<usize> = self
            .columns
            .iter()
            .map(|c| col(table, c))
            .collect::<anyhow::Result<_>>()?;
        let values = merge_columns(table, &cols, &self.separator);
        let taken = names(table);
        let base = self.new_name.trim();
        let name = unique_name(&taken, if base.is_empty() { "merged" } else { base });
        let idx = insert_index(self.position, taken.len(), taken.len());
        insert_filled(table, idx, name, values);
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_merge").replace("{columns}", &self.columns.join(", "))
    }

    pub fn columns(&self) -> Vec<String> {
        self.columns.clone()
    }
}
