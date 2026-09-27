use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::col;
use crate::data::retype::{Strictness, TargetType, apply_retype_with};
use crate::i18n::t;

/// Change a column's type. `to` is `text`, `integer`, `float`, `boolean`,
/// `date` or `datetime`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeType {
    pub column: String,
    pub to: String,
    /// Convert only when every value converts, like the dialog's checkbox.
    #[serde(default)]
    pub strict: bool,
}

impl ChangeType {
    pub fn new(column: String, target: TargetType, strict: bool) -> Self {
        let to = match target {
            TargetType::Text => "text",
            TargetType::Integer => "integer",
            TargetType::Float => "float",
            TargetType::Boolean => "boolean",
            TargetType::Date => "date",
            TargetType::DateTime => "datetime",
        };
        Self {
            column,
            to: to.to_string(),
            strict,
        }
    }

    fn target(&self) -> anyhow::Result<TargetType> {
        Ok(match self.to.to_ascii_lowercase().as_str() {
            "text" => TargetType::Text,
            "integer" => TargetType::Integer,
            "float" => TargetType::Float,
            "boolean" => TargetType::Boolean,
            "date" => TargetType::Date,
            "datetime" => TargetType::DateTime,
            other => anyhow::bail!(
                "unknown type `{other}`: use text, integer, float, boolean, date or datetime"
            ),
        })
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let strictness = if self.strict {
            Strictness::Strict
        } else {
            Strictness::Mixed
        };
        let out = apply_retype_with(table, c, self.target()?, strictness);
        if let Some(failed) = out.refused {
            anyhow::bail!("{failed} value(s) in `{}` would not convert", self.column);
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        let to = self
            .target()
            .map(|tt| t(tt.i18n_key()))
            .unwrap_or_else(|_| self.to.clone());
        t("recipe.step_change_type")
            .replace("{column}", &self.column)
            .replace("{type}", &to)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
