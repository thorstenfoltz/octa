use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::calculated::{CalcByName, CalcExtras, CalcKind, add_calculated_column};
use crate::i18n::t;

/// Columns > Calculated column..., by column name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calculated {
    pub kind: CalcKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descending: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(flatten)]
    pub extras: CalcExtras,
    /// The name the column got when it was recorded.
    pub name: String,
}

impl Calculated {
    pub fn by_name(&self) -> CalcByName {
        CalcByName {
            kind: self.kind,
            value: self.value.clone(),
            order: self.order.clone(),
            descending: self.descending,
            groups: self.groups.clone(),
            extras: self.extras,
        }
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let spec = self.by_name().resolve(table)?;
        add_calculated_column(table, &spec, Some(&self.name))?;
        Ok(())
    }

    pub fn describe(&self) -> String {
        t("recipe.step_calculated")
            .replace("{name}", &self.name)
            .replace("{kind}", &t(&self.kind.label_key()))
    }

    pub fn columns(&self) -> Vec<String> {
        self.by_name().columns()
    }
}
