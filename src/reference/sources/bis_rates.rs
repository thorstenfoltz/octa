//! BIS bilateral exchange rates (`WS_XRU`): units of each currency per US
//! dollar, daily for about 60 currencies, monthly averages for about 150.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::data::decimal::Exact;
use crate::reference::download::{self, Fetched};
use crate::reference::merge::{self, SeriesSet};
use crate::reference::{
    Dataset, ReferenceSource, Refreshed, footer, is_day, is_month, store, validators_of,
    with_zip_entry,
};

pub const URL: &str = "https://data.bis.org/static/bulk/WS_XRU_csv_col.zip";
pub const DAILY: &str = "currency_daily.parquet";
pub const MONTHLY: &str = "currency_monthly.parquet";

pub struct BisRates;

/// Daily series and monthly averages from the column CSV. Several economies
/// share a currency (every euro area member lists EUR); the first value read
/// for a currency and period wins.
pub fn parse(csv: impl Read) -> anyhow::Result<(SeriesSet, SeriesSet)> {
    let mut r = csv::ReaderBuilder::new().flexible(true).from_reader(csv);
    let headers = r.headers()?.clone();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("the BIS file has no {name} column"))
    };
    let (freq, cur, coll, mult) = (
        col("FREQ")?,
        col("CURRENCY")?,
        col("COLLECTION")?,
        col("UNIT_MULT").ok(),
    );
    let first = col("Series")? + 1;
    let (mut daily, mut monthly) = (SeriesSet::new(), SeriesSet::new());
    for rec in r.records() {
        let rec = rec?;
        if mult
            .and_then(|m| rec.get(m))
            .is_some_and(|m| !m.is_empty() && m != "0")
        {
            continue; // scaled series; none exist today, never guess their unit
        }
        let (target, is_key): (&mut SeriesSet, fn(&str) -> bool) =
            match (rec.get(freq), rec.get(coll)) {
                (Some("D"), _) => (&mut daily, is_day),
                (Some("M"), Some("A")) => (&mut monthly, is_month),
                _ => continue,
            };
        let Some(code) = rec.get(cur).filter(|c| c.len() == 3) else {
            continue;
        };
        let series = target.entry(code.to_string()).or_default();
        for (h, v) in headers.iter().zip(rec.iter()).skip(first) {
            let v = v.trim();
            if !is_key(h) || v.is_empty() || v.eq_ignore_ascii_case("nan") {
                continue;
            }
            if let Some(x) = Exact::parse(v) {
                series.entry(h.to_string()).or_insert(x);
            }
        }
    }
    daily.retain(|_, s| !s.is_empty());
    monthly.retain(|_, s| !s.is_empty());
    Ok((daily, monthly))
}

impl ReferenceSource for BisRates {
    fn dataset(&self) -> Dataset {
        Dataset::ExchangeRates
    }

    fn files(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(DAILY), dir.join(MONTHLY)]
    }

    fn refresh_with(
        &self,
        dir: &Path,
        _: &[String],
        base: Option<&str>,
    ) -> anyhow::Result<Refreshed> {
        let (daily_path, monthly_path) = (dir.join(DAILY), dir.join(MONTHLY));
        let known = if monthly_path.exists() {
            validators_of(&daily_path)
        } else {
            download::Validators::default()
        };
        match download::fetch(&download::resolve_with(URL, base), &known)? {
            Fetched::NotModified => {
                store::touch(&daily_path)?;
                store::touch(&monthly_path)?;
                Ok(Refreshed {
                    downloaded: 0,
                    unchanged: 1,
                })
            }
            Fetched::Body { file, validators } => {
                let (daily, monthly) =
                    with_zip_entry(file.path(), |n| n.ends_with(".csv"), |r| parse(r))?;
                if daily.is_empty() {
                    bail!("{URL}: no daily exchange rates in the download");
                }
                let meta = footer(self.dataset(), URL, &validators);
                for (path, fresh, key) in [
                    (&daily_path, daily, "date"),
                    (&monthly_path, monthly, "month"),
                ] {
                    let old = if path.exists() {
                        merge::table_to_series(&store::read(path)?, key)?
                    } else {
                        SeriesSet::new()
                    };
                    let (merged, _) = merge::merge_series(&old, &fresh);
                    store::write(path, &merge::series_to_table(key, &merged, meta.clone()))?;
                }
                Ok(Refreshed {
                    downloaded: 1,
                    unchanged: 0,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "bis_rates_tests.rs"]
mod tests;
