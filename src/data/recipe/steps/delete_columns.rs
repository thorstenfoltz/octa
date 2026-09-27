use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::col;
use crate::i18n::t;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteColumns {
    pub columns: Vec<String>,
}

impl DeleteColumns {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let mut idx: Vec<usize> = self
            .columns
            .iter()
            .map(|c| col(table, c))
            .collect::<anyhow::Result<_>>()?;
        idx.sort_unstable();
        idx.dedup();
        for i in idx.into_iter().rev() {
            table.delete_column(i);
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_delete_columns").replace("{columns}", &self.columns.join(", "))
    }

    pub fn columns(&self) -> Vec<String> {
        self.columns.clone()
    }
}
