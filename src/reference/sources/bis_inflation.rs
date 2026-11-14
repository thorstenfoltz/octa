//! BIS long consumer price series (`WS_LONG_CPI`): monthly index, 2010 = 100,
//! for 63 economies, the euro area as `XM`.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::data::decimal::Exact;
use crate::reference::download::{self, Fetched};
use crate::reference::merge::{self, SeriesSet};
use crate::reference::{
    Dataset, ReferenceSource, Refreshed, footer, is_month, store, validators_of, with_zip_entry,
};

pub const URL: &str = "https://data.bis.org/static/bulk/WS_LONG_CPI_csv_col.zip";
pub const FILE: &str = "inflation.parquet";
/// BIS's code for "Index, 2010 = 100".
const INDEX: &str = "628";

pub struct BisInflation;

pub fn parse(csv: impl Read) -> anyhow::Result<SeriesSet> {
    let mut r = csv::ReaderBuilder::new().flexible(true).from_reader(csv);
    let headers = r.headers()?.clone();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("the BIS file has no {name} column"))
    };
    let (freq, area, unit) = (col("FREQ")?, col("REF_AREA")?, col("UNIT_MEASURE")?);
    let first = col("Series")? + 1;
    let mut set = SeriesSet::new();
    for rec in r.records() {
        let rec = rec?;
        if rec.get(freq) != Some("M") || rec.get(unit) != Some(INDEX) {
            continue;
        }
        let Some(code) = rec.get(area).filter(|c| !c.is_empty()) else {
            continue;
        };
        let series = set.entry(code.to_string()).or_default();
        for (h, v) in headers.iter().zip(rec.iter()).skip(first) {
            let v = v.trim();
            if is_month(h)
                && let Some(x) = Exact::parse(v)
            {
                series.entry(h.to_string()).or_insert(x);
            }
        }
    }
    set.retain(|_, s| !s.is_empty());
    Ok(set)
}

impl ReferenceSource for BisInflation {
    fn dataset(&self) -> Dataset {
        Dataset::Inflation
    }

    fn files(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(FILE)]
    }

    fn refresh_with(
        &self,
        dir: &Path,
        _: &[String],
        base: Option<&str>,
    ) -> anyhow::Result<Refreshed> {
        let path = dir.join(FILE);
        match download::fetch(&download::resolve_with(URL, base), &validators_of(&path))? {
            Fetched::NotModified => {
                store::touch(&path)?;
                Ok(Refreshed {
                    downloaded: 0,
                    unchanged: 1,
                })
            }
            Fetched::Body { file, validators } => {
                let fresh = with_zip_entry(file.path(), |n| n.ends_with(".csv"), |r| parse(r))?;
                if fresh.is_empty() {
                    bail!("{URL}: no monthly price index in the download");
                }
                let old = if path.exists() {
                    merge::table_to_series(&store::read(&path)?, "month")?
                } else {
                    SeriesSet::new()
                };
                let (merged, _) = merge::merge_series(&old, &fresh);
                store::write(
                    &path,
                    &merge::series_to_table(
                        "month",
                        &merged,
                        footer(self.dataset(), URL, &validators),
                    ),
                )?;
                Ok(Refreshed {
                    downloaded: 1,
                    unchanged: 0,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "bis_inflation_tests.rs"]
mod tests;
