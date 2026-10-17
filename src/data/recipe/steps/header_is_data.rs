use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::i18n::t;

/// Turn the column names into the first row (a file without a header).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeaderIsData {
    /// As [`super::UseHeaderRows::dates`].
    #[serde(default = "super::dates_by_default")]
    pub dates: bool,
}

impl HeaderIsData {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        crate::data::header_rows::header_is_data(table, self.dates)
    }

    pub fn describe(&self) -> String {
        t("recipe.step_header_is_data")
    }

    pub fn columns(&self) -> Vec<String> {
        Vec::new()
    }
}
