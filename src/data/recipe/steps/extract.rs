use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::steps::insert_filled;
use crate::data::recipe::{col, insert_index, names, unique_name};
use crate::data::transform::extract_pattern;
use crate::i18n::t;

/// Copy the part of each cell that matches `pattern` into a new column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extract {
    pub column: String,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub new_name: String,
    /// 1-based position of the new column. None: right after `column`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
}

impl Extract {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let re = regex::Regex::new(&self.pattern)?;
        let values = extract_pattern(table, c, &re);
        let taken = names(table);
        let base = match self.new_name.trim() {
            "" => format!("{}_extracted", self.column),
            typed => typed.to_string(),
        };
        let name = unique_name(&taken, &base);
        let idx = insert_index(self.position, c + 1, taken.len());
        insert_filled(table, idx, name, values);
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_extract").replace("{column}", &self.column)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
