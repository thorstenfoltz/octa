use serde::{Deserialize, Serialize};

use crate::data::DataTable;
use crate::i18n::t;

/// Use table rows `first..=last` (1-based, as the user saw them) as the
/// column names; they and every row above them go. Positional on purpose:
/// next month's export has a title block of the same height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseHeaderRows {
    pub first: usize,
    pub last: usize,
    /// Whether retyping the columns looks for dates, as it did when the step
    /// was recorded (the "Detect dates on load" setting then), so a replay
    /// does what the recording did whatever this machine's Settings say.
    #[serde(default = "super::dates_by_default")]
    pub dates: bool,
}

impl UseHeaderRows {
    pub fn apply(&self, table: &mut DataTable) -> anyhow::Result<()> {
        if self.first == 0 || self.first > self.last {
            anyhow::bail!(
                "header rows are counted from 1 with first <= last, got {}-{}",
                self.first,
                self.last
            );
        }
        crate::data::header_rows::use_header_rows(table, self.first - 1, self.last - 1, self.dates)
    }

    pub fn describe(&self) -> String {
        if self.first == self.last {
            t("recipe.step_use_header_row").replace("{row}", &self.first.to_string())
        } else {
            t("recipe.step_use_header_rows")
                .replace("{first}", &self.first.to_string())
                .replace("{last}", &self.last.to_string())
        }
    }

    pub fn columns(&self) -> Vec<String> {
        Vec::new()
    }
}
