use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::data::impute::{ImputeStrategy, impute_column};
use crate::data::recipe::col;
use crate::data::recipe::steps::write_column;
use crate::i18n::t;

/// Fill empty cells. `strategy` is `mean`, `median`, `mode`, `previous`,
/// `next` or `value` (with `value` set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillMissing {
    pub column: String,
    pub strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl FillMissing {
    pub fn new(column: String, strategy: &ImputeStrategy) -> Self {
        let (s, value) = match strategy {
            ImputeStrategy::Mean => ("mean", None),
            ImputeStrategy::Median => ("median", None),
            ImputeStrategy::Mode => ("mode", None),
            ImputeStrategy::ForwardFill => ("previous", None),
            ImputeStrategy::BackwardFill => ("next", None),
            ImputeStrategy::Constant(v) => ("value", Some(v.clone())),
        };
        Self {
            column,
            strategy: s.to_string(),
            value,
        }
    }

    fn strategy(&self) -> anyhow::Result<ImputeStrategy> {
        Ok(match self.strategy.to_ascii_lowercase().as_str() {
            "mean" => ImputeStrategy::Mean,
            "median" => ImputeStrategy::Median,
            "mode" => ImputeStrategy::Mode,
            "previous" => ImputeStrategy::ForwardFill,
            "next" => ImputeStrategy::BackwardFill,
            "value" => ImputeStrategy::Constant(self.value.clone().unwrap_or_default()),
            other => anyhow::bail!(
                "unknown strategy `{other}`: use mean, median, mode, previous, next or value"
            ),
        })
    }

    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        let c = col(table, &self.column)?;
        let values = impute_column(table, c, &self.strategy()?)?;
        write_column(table, c, values);
        Ok(())
    }

    pub fn describe(&self) -> String {
        // The Fill missing values dialog's own strategy names, so the panel
        // speaks the same words as the dialog that recorded the step.
        let how = match self.strategy.as_str() {
            "mean" => t("impute.strat_mean"),
            "median" => t("impute.strat_median"),
            "mode" => t("impute.strat_mode"),
            "previous" => t("impute.strat_ffill"),
            "next" => t("impute.strat_bfill"),
            "value" => self.value.clone().unwrap_or_default(),
            other => other.to_string(),
        };
        t("recipe.step_fill_missing")
            .replace("{column}", &self.column)
            .replace("{how}", &how)
    }

    pub fn columns(&self) -> Vec<String> {
        vec![self.column.clone()]
    }
}
