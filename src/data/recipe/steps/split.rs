use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::recipe::steps::insert_filled;
use crate::data::recipe::{col, insert_index, names, unique_name};
use crate::data::transform::{SplitSpec, split_column};
use crate::i18n::t;

/// Split one column into several. `by` is `delimiter`, `regex` or `width`;
/// `value` holds the delimiter, the pattern or the width.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Split {
    pub column: String,
    pub by: String,
    pub value: String,
    /// Base name for the new columns (`<name>_1`, `<name>_2`, ...). Empty
    /// uses the engine's own names.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub new_name: String,
    /// 1-based position of the first new column. None: right after `column`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
}

impl Split {
    pub fn spec(&self) -> anyhow::Result<SplitSpec> {
        Ok(match self.by.as_str() {
            "delimiter" => SplitSpec::Delimiter(self.value.clone()),
            "regex" => SplitSpec::Regex(self.value.clone()),
            "width" => SplitSpec::FixedWidth(
                self.value
                    .trim()
                    .parse()
                    .ok()
                    .filter(|&w| w > 0)
                    .ok_or_else(|| anyhow::anyhow!("width must be a positive number"))?,
            ),
            other => anyhow::bail!("split by must be delimiter, regex or width, not `{other}`"),
        })
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let out = split_column(table, c, &self.spec()?)?;
        let mut taken = names(table);
        let start = insert_index(self.position, c + 1, taken.len());
        let base = self.new_name.trim();
        for (offset, (auto_name, values)) in out.into_iter().enumerate() {
            let proposed = if base.is_empty() {
                auto_name
            } else {
                format!("{base}_{}", offset + 1)
            };
            let name = unique_name(&taken, &proposed);
            taken.push(name.clone());
            insert_filled(table, start + offset, name, values);
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_split")
            .replace("{column}", &self.column)
            .replace("{value}", &self.value)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
