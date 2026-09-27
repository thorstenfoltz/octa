use serde::{Deserialize, Serialize};

use crate::data::id_checks::IdKind;
use crate::data::recipe::col;
use crate::data::{CellValue, DataTable};
use crate::i18n::t;

/// Write every valid ID in a column one standard way (`de89 3704 ...` ->
/// `DE89 3704 ...`). Invalid values are left exactly as they are: tidying
/// must never disguise a value that needs a human look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TidyId {
    pub column: String,
    /// `iban`, `card_number`, `gtin`, `vat_id` or `email`.
    pub kind: String,
}

impl TidyId {
    fn id_kind(&self) -> anyhow::Result<IdKind> {
        IdKind::from_id(&self.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown ID kind `{}`", self.kind))
    }

    /// `(row, tidy value)` for every cell whose tidy form differs from it.
    pub fn changes(table: &DataTable, col: usize, kind: IdKind) -> Vec<(usize, String)> {
        (0..table.row_count())
            .filter_map(|r| {
                let v = table.get(r, col)?.to_string();
                kind.tidy(&v).filter(|t| *t != v).map(|t| (r, t))
            })
            .collect()
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let kind = self.id_kind()?;
        let c = col(table, &self.column)?;
        for (r, v) in Self::changes(table, c, kind) {
            table.set(r, c, CellValue::String(v));
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_tidy_id")
            .replace("{column}", &self.column)
            .replace("{kind}", &self.kind)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
