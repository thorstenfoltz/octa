use serde::{Deserialize, Serialize};

use crate::data::recipe::col;
use crate::data::{CellValue, DataTable};
use crate::i18n::t;

/// One hand-typed value: which row (by its key), which column, what value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellEdit {
    /// The row's values in the key columns, in `SetCells::key` order.
    pub row: Vec<String>,
    pub column: String,
    /// `None` clears the cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Cells typed by hand. A row number means nothing in next month's file, so
/// each edit names its row by the values of the key column(s), usually an
/// ID: "price = 9.99 in the row where id = 1042".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetCells {
    /// The column(s) that identify a row.
    pub key: Vec<String>,
    pub cells: Vec<CellEdit>,
}

impl SetCells {
    /// Every edit runs where its row is found. When some rows are missing
    /// the rest are still written and the error names the missing ones.
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        if self.key.is_empty() {
            anyhow::bail!("these cell edits have no ID column yet");
        }
        let key_cols: Vec<usize> = self
            .key
            .iter()
            .map(|k| col(table, k))
            .collect::<anyhow::Result<_>>()?;
        let text = |t: &DataTable, r: usize, c: usize| {
            t.get(r, c).map(|v| v.to_string()).unwrap_or_default()
        };
        let mut missing: Vec<String> = Vec::new();
        for edit in &self.cells {
            let c = col(table, &edit.column)?;
            let rows: Vec<usize> = (0..table.row_count())
                .filter(|&r| {
                    key_cols
                        .iter()
                        .zip(&edit.row)
                        .all(|(&kc, want)| text(table, r, kc) == *want)
                })
                .collect();
            if rows.is_empty() {
                missing.push(self.row_label(&edit.row));
            }
            for r in rows {
                let value = match &edit.value {
                    None => CellValue::Null,
                    Some(v) => match table.get(r, c) {
                        Some(existing) => CellValue::parse_like(existing, v),
                        None => CellValue::String(v.clone()),
                    },
                };
                table.set(r, c, value);
            }
        }
        if !missing.is_empty() {
            anyhow::bail!(
                "{} of {} edit(s) found no row: {}",
                missing.len(),
                self.cells.len(),
                missing.join("; ")
            );
        }
        Ok(())
    }

    fn row_label(&self, values: &[String]) -> String {
        self.key
            .iter()
            .zip(values)
            .map(|(k, v)| format!("{k} = {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn describe(&self) -> String {
        match self.cells.as_slice() {
            [one] if !self.key.is_empty() => t("recipe.step_set_cell")
                .replace("{column}", &one.column)
                .replace("{value}", one.value.as_deref().unwrap_or(""))
                .replace("{row}", &self.row_label(&one.row)),
            _ if self.key.is_empty() => {
                t("recipe.step_set_cells_no_key").replace("{count}", &self.cells.len().to_string())
            }
            _ => t("recipe.step_set_cells")
                .replace("{count}", &self.cells.len().to_string())
                .replace("{key}", &self.key.join(", ")),
        }
    }

    pub fn columns(&self) -> Vec<String> {
        let mut out = self.key.clone();
        for e in &self.cells {
            if !out.contains(&e.column) {
                out.push(e.column.clone());
            }
        }
        out
    }
}

/// The column that identifies a row, when one does so plainly: every value
/// present and different, and named like an identifier (`id`, `customer_id`,
/// `key`, `code`, `nr`, `no`, `number`), or else the first column when that
/// one is unique. `None` means ask the user: a unique column that merely
/// happens to be unique (a price, a timestamp) is not an ID to guess at.
pub fn guess_row_key(table: &DataTable) -> Option<String> {
    let unique = |c: usize| {
        let mut seen = std::collections::HashSet::new();
        (0..table.row_count()).all(|r| {
            let v = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
            !v.is_empty() && seen.insert(v)
        })
    };
    let id_like = |name: &str| {
        let n = name.to_ascii_lowercase();
        let words: Vec<&str> = n
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        n.ends_with("id")
            || words
                .iter()
                .any(|w| matches!(*w, "id" | "key" | "code" | "nr" | "no" | "number" | "uuid"))
    };
    if table.row_count() == 0 {
        return None;
    }
    (0..table.col_count())
        .find(|&c| id_like(&table.columns[c].name) && unique(c))
        .or_else(|| (table.col_count() > 0 && unique(0)).then_some(0))
        .map(|c| table.columns[c].name.clone())
}

/// Whether the values in `key` tell every row apart: how many rows share
/// their key with another row (0 = a good ID).
pub fn duplicate_key_rows(table: &DataTable, key: &[usize]) -> usize {
    let mut counts: std::collections::HashMap<Vec<String>, usize> = Default::default();
    for r in 0..table.row_count() {
        let k = key
            .iter()
            .map(|&c| table.get(r, c).map(|v| v.to_string()).unwrap_or_default())
            .collect();
        *counts.entry(k).or_insert(0) += 1;
    }
    counts.values().filter(|&&n| n > 1).sum()
}
