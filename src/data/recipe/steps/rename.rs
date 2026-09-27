use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::col;
use crate::i18n::t;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

/// Rename columns, in order, so `a -> b` then `b -> c` works as typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rename {
    pub renames: Vec<RenamePair>,
}

impl Rename {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        // Resolve every source first so a missing one changes nothing.
        let mut sim: Vec<String> = crate::data::recipe::names(table);
        for p in &self.renames {
            let i = sim
                .iter()
                .position(|n| n == &p.from)
                .ok_or_else(|| anyhow::anyhow!("column `{}` not found", p.from))?;
            sim[i] = p.to.clone();
        }
        for p in &self.renames {
            let i = col(table, &p.from)?;
            table.rename_column(i, p.to.clone());
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        let pairs: Vec<String> = self
            .renames
            .iter()
            .map(|p| format!("{} -> {}", p.from, p.to))
            .collect();
        t("recipe.step_rename").replace("{pairs}", &pairs.join(", "))
    }

    pub fn columns(&self) -> Vec<String> {
        // A later pair may rename what an earlier one produced; only names no
        // earlier pair creates must exist up front.
        let mut made = Vec::new();
        let mut need = Vec::new();
        for p in &self.renames {
            if !made.contains(&p.from) {
                need.push(p.from.clone());
            }
            made.push(p.to.clone());
        }
        need
    }
}
