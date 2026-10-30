use serde::{Deserialize, Serialize};

use crate::data::{DataTable, TextOrder};
use crate::i18n::t;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// Sort rows by one or more columns, first key first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sort {
    pub by: Vec<SortKey>,
    /// Text in natural order (`file2` before `file10`). Recorded from the
    /// setting at the time; a replay follows the recipe, not the replaying
    /// machine's setting. Absent in older recipes, which sort letter by letter.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub natural: bool,
}

impl Sort {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let keys: Vec<(usize, bool)> = self
            .by
            .iter()
            .map(|k| Ok((table.column_index(&k.column)?, !k.descending)))
            .collect::<anyhow::Result<_>>()?;
        table.sort_rows_by_columns(&keys, TextOrder::from_natural(self.natural));
        Ok(())
    }

    pub fn describe(&self) -> String {
        let keys: Vec<String> = self
            .by
            .iter()
            .map(|k| {
                let dir = if k.descending {
                    t("recipe.descending")
                } else {
                    t("recipe.ascending")
                };
                format!("{} ({dir})", k.column)
            })
            .collect();
        let text = t("recipe.step_sort").replace("{keys}", &keys.join(", "));
        if self.natural {
            format!("{text}, {}", t("natsort.recipe_suffix"))
        } else {
            text
        }
    }

    pub fn columns(&self) -> Vec<String> {
        self.by.iter().map(|k| k.column.clone()).collect()
    }
}
