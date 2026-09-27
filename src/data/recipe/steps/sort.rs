use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::col;
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
}

impl Sort {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let keys: Vec<(usize, bool)> = self
            .by
            .iter()
            .map(|k| Ok((col(table, &k.column)?, !k.descending)))
            .collect::<anyhow::Result<_>>()?;
        table.sort_rows_by_columns(&keys);
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
        t("recipe.step_sort").replace("{keys}", &keys.join(", "))
    }

    pub fn columns(&self) -> Vec<String> {
        self.by.iter().map(|k| k.column.clone()).collect()
    }
}
