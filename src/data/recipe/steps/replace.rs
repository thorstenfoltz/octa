use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::SearchMode;
use crate::data::recipe::col;
use crate::data::recipe::steps::write_column;
use crate::data::search::RowMatcher;
use crate::data::transform::replace_in_column;
use crate::i18n::t;

/// Find and replace inside one column. `mode` is `plain`, `wildcard` or
/// `regex`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replace {
    pub column: String,
    pub find: String,
    #[serde(default)]
    pub with: String,
    #[serde(default = "plain")]
    pub mode: String,
}

fn plain() -> String {
    "plain".to_string()
}

impl Replace {
    pub fn new(column: String, find: String, with: String, mode: SearchMode) -> Self {
        let mode = match mode {
            SearchMode::Plain => "plain",
            SearchMode::Wildcard => "wildcard",
            SearchMode::Regex => "regex",
        };
        Self {
            column,
            find,
            with,
            mode: mode.to_string(),
        }
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let mode = match self.mode.as_str() {
            "plain" => SearchMode::Plain,
            "wildcard" => SearchMode::Wildcard,
            "regex" => SearchMode::Regex,
            other => anyhow::bail!("mode must be plain, wildcard or regex, not `{other}`"),
        };
        let matcher = RowMatcher::new(&self.find, mode);
        if matches!(matcher, RowMatcher::Invalid) {
            anyhow::bail!("`{}` is not a valid pattern", self.find);
        }
        let values = replace_in_column(table, c, &matcher, &self.with);
        write_column(table, c, values);
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_replace")
            .replace("{column}", &self.column)
            .replace("{find}", &self.find)
            .replace("{with}", &self.with)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
