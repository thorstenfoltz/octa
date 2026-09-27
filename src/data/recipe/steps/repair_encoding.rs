use serde::{Deserialize, Serialize};

use crate::data::recipe::col;
use crate::data::{CellValue, DataTable};
use crate::i18n::t;

/// Repair text decoded with the wrong character set (`MÃ¼ller` -> `Müller`).
/// Cells the repair cannot prove stay as they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairEncoding {
    pub column: String,
}

impl RepairEncoding {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let fixes: Vec<(usize, String)> = (0..table.row_count())
            .filter_map(|r| match table.get(r, c) {
                Some(CellValue::String(v)) => crate::data::mojibake::repair(v).map(|f| (r, f)),
                _ => None,
            })
            .collect();
        for (r, v) in fixes {
            table.set(r, c, CellValue::String(v));
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_repair_encoding").replace("{column}", &self.column)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
