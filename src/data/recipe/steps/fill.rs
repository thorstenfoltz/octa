use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::col;
use crate::data::recipe::steps::write_column;
use crate::data::transform::{fill_down, fill_up};
use crate::i18n::t;

/// Fill empty cells from the value above (`down`) or below (`up`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub column: String,
    pub direction: String,
}

impl Fill {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let values = match self.direction.as_str() {
            "down" => fill_down(table, c),
            "up" => fill_up(table, c),
            other => anyhow::bail!("direction must be down or up, not `{other}`"),
        };
        write_column(table, c, values);
        Ok(())
    }

    pub fn describe(&self) -> String {
        let key = if self.direction == "up" {
            "recipe.step_fill_up"
        } else {
            "recipe.step_fill_down"
        };
        t(key).replace("{column}", &self.column)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
