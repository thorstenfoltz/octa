//! `octa --funnel FILE --funnel-who COL --funnel-event COL --funnel-steps a,b,c`:
//! print how many people get through a series of steps in order. Same
//! engine as the GUI dialog and the MCP tool `funnel`.

use std::path::PathBuf;

use octa::data::funnel::{FunnelSpec, collect, steps_from, table};

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub who: String,
    pub event: String,
    pub when: Option<String>,
    pub steps: String,
    pub within_days: Option<u32>,
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let data = super::read_table(&path)?;
    let col = |name: &str| {
        data.columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| anyhow::anyhow!("--funnel: no column \"{name}\""))
    };
    let spec = FunnelSpec {
        who: col(&o.who)?,
        event: col(&o.event)?,
        when: o.when.as_deref().map(col).transpose()?,
        steps: steps_from(o.steps.split(',')),
        within_days: o.within_days,
    };
    let counts = collect(&data, &spec)
        .map_err(|e| anyhow::anyhow!("--funnel: {}", octa::i18n::t(e.key())))?;
    if counts.skipped > 0 {
        eprintln!(
            "{} rows had no who or no readable date and were left out",
            counts.skipped
        );
    }
    write_table(&table(&counts), format)
}
