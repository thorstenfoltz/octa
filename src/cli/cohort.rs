//! `octa --cohort FILE --cohort-who COL --cohort-when COL` - print a cohort
//! table: who first appeared in which period, and how many came back (or a
//! column's sum) in each later period. Same engine as the GUI dialog and the
//! MCP tool `cohort_table`.

use std::path::PathBuf;

use octa::data::cohort::{CohortSpec, Period, Show, collect, wide};

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub who: String,
    pub when: String,
    pub period: Option<String>,
    pub show: Option<String>,
    pub sum: Option<String>,
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let period = match o.period.as_deref() {
        None => Period::Month,
        Some(p) => Period::parse(p).ok_or_else(|| {
            anyhow::anyhow!(
                "--cohort-period: unknown \"{p}\", expected day, week, month, quarter or year"
            )
        })?,
    };
    let show = match o.show.as_deref() {
        None => Show::Share,
        Some(s) => Show::parse(s).ok_or_else(|| {
            anyhow::anyhow!("--cohort-show: unknown \"{s}\", expected share, count or sum")
        })?,
    };
    let table = super::read_table(&path)?;
    let col = |name: &str| {
        table
            .columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| anyhow::anyhow!("--cohort: no column \"{name}\""))
    };
    let spec = CohortSpec {
        who: col(&o.who)?,
        when: col(&o.when)?,
        period,
        show,
        sum_col: o.sum.as_deref().map(col).transpose()?,
    };
    let cells = collect(&table, &spec)
        .map_err(|e| anyhow::anyhow!("--cohort: {}", octa::i18n::t(e.key())))?;
    if cells.skipped > 0 {
        eprintln!(
            "{} rows had no who or no readable date and were left out",
            cells.skipped
        );
    }
    write_table(&wide(&cells), format)
}
