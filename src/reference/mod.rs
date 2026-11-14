//! Reference data Octa downloads on request: exchange rates, inflation,
//! postcodes, European regions and German postcode and municipality pairs.
//! Every source is off until switched on in Settings > Online data. Nothing
//! here is ever shipped with Octa; see the spec
//! `docs/superpowers/specs/2026-11-13-reference-data-design.md`.

pub mod download;
pub mod jobs;
pub mod merge;
pub mod sources;
pub mod store;
#[cfg(test)]
pub(crate) mod test_server;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dataset {
    ExchangeRates,
    Inflation,
    Postcodes,
    EuRegions,
    DePostcodeMunicipality,
}

impl Dataset {
    pub const ALL: [Dataset; 5] = [
        Dataset::ExchangeRates,
        Dataset::Inflation,
        Dataset::Postcodes,
        Dataset::EuRegions,
        Dataset::DePostcodeMunicipality,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Dataset::ExchangeRates => "exchange_rates",
            Dataset::Inflation => "inflation",
            Dataset::Postcodes => "postcodes",
            Dataset::EuRegions => "eu_regions",
            Dataset::DePostcodeMunicipality => "de_postcode_municipality",
        }
    }

    pub fn label_key(self) -> String {
        format!("online.{}", self.id())
    }

    pub fn hint_key(self) -> String {
        format!("online.{}_hint", self.id())
    }

    /// The credit line, shown in the dialog, Settings, Help and docs. ASCII:
    /// the GUI's bundled font draws a copyright sign as an empty box.
    pub fn credit(self) -> &'static str {
        match self {
            Dataset::ExchangeRates | Dataset::Inflation => {
                "Source: BIS (Bank for International Settlements), data.bis.org"
            }
            Dataset::Postcodes => {
                "Postcodes: GeoNames (geonames.org), CC BY 4.0. UK: Contains Royal Mail data (c) Royal Mail copyright and database right"
            }
            Dataset::EuRegions => {
                "(c) European Union - GISCO, postal code point dataset, Licence CC-BY-SA 4.0; NUTS and municipalities (c) European Union, Eurostat"
            }
            Dataset::DePostcodeMunicipality => {
                "Postcode and municipality keys: OpenPLZ (openplzapi.org), from OpenStreetMap, (c) OpenStreetMap contributors, ODbL 1.0"
            }
        }
    }

    pub fn credit_url(self) -> &'static str {
        match self {
            Dataset::ExchangeRates | Dataset::Inflation => "https://data.bis.org/help/legal",
            Dataset::Postcodes => "https://www.geonames.org/",
            Dataset::EuRegions => {
                "https://ec.europa.eu/eurostat/web/gisco/geodata/administrative-units/postal-codes"
            }
            Dataset::DePostcodeMunicipality => "https://www.openplzapi.org/",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalUnit {
    Days,
    Months,
}

/// One dataset's `[reference.<id>]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceSettings {
    /// Off until the user switches it on (user, 2026-11-13).
    pub enabled: bool,
    /// Refresh after this many units; 0 is read as 1.
    pub every: u32,
    pub unit: IntervalUnit,
}

impl Default for SourceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            every: 1,
            unit: IntervalUnit::Months,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceSettings {
    pub exchange_rates: SourceSettings,
    pub inflation: SourceSettings,
    pub postcodes: SourceSettings,
    pub eu_regions: SourceSettings,
    pub de_postcode_municipality: SourceSettings,
}

impl ReferenceSettings {
    pub fn get(&self, d: Dataset) -> &SourceSettings {
        match d {
            Dataset::ExchangeRates => &self.exchange_rates,
            Dataset::Inflation => &self.inflation,
            Dataset::Postcodes => &self.postcodes,
            Dataset::EuRegions => &self.eu_regions,
            Dataset::DePostcodeMunicipality => &self.de_postcode_municipality,
        }
    }

    pub fn get_mut(&mut self, d: Dataset) -> &mut SourceSettings {
        match d {
            Dataset::ExchangeRates => &mut self.exchange_rates,
            Dataset::Inflation => &mut self.inflation,
            Dataset::Postcodes => &mut self.postcodes,
            Dataset::EuRegions => &mut self.eu_regions,
            Dataset::DePostcodeMunicipality => &mut self.de_postcode_municipality,
        }
    }
}

/// Is data last checked at `last` due again? Never checked is due.
pub fn is_stale(last: Option<SystemTime>, s: &SourceSettings, now: SystemTime) -> bool {
    let Some(last) = last else {
        return true;
    };
    let every = s.every.max(1);
    let (last, now): (DateTime<Utc>, DateTime<Utc>) = (last.into(), now.into());
    // A due date past chrono's range means "practically never", so it is not stale.
    let due = match s.unit {
        IntervalUnit::Days => last.checked_add_signed(chrono::Duration::days(i64::from(every))),
        IntervalUnit::Months => last.checked_add_months(Months::new(every)),
    };
    due.is_some_and(|due| now >= due)
}

/// The refusal CLI and MCP give while a dataset is off.
pub fn off_message(d: Dataset) -> String {
    format!(
        "{} is switched off: switch it on in Settings > Online data, or set `enabled = true` under [reference.{}] in settings.toml",
        crate::i18n::t_english(&d.label_key()),
        d.id()
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refreshed {
    pub downloaded: usize,
    pub unchanged: usize,
}

/// One downloadable dataset. One file per source under `sources/`.
pub trait ReferenceSource: Sync {
    fn dataset(&self) -> Dataset;
    /// The files this source keeps (fixed names; GeoNames lists what is stored).
    fn files(&self, dir: &Path) -> Vec<PathBuf>;
    /// Download what changed, merge it with what is stored, write. `countries`
    /// matters to GeoNames only: ISO alpha-2 codes to fetch, empty = the ones
    /// already stored. `base` replaces scheme and host (tests).
    fn refresh_with(
        &self,
        dir: &Path,
        countries: &[String],
        base: Option<&str>,
    ) -> anyhow::Result<Refreshed>;

    fn refresh(&self, dir: &Path, countries: &[String]) -> anyhow::Result<Refreshed> {
        let base = std::env::var(download::BASE_ENV).ok();
        self.refresh_with(dir, countries, base.as_deref())
    }
}

pub fn source(d: Dataset) -> &'static dyn ReferenceSource {
    match d {
        Dataset::ExchangeRates => &sources::bis_rates::BisRates,
        Dataset::Inflation => &sources::bis_inflation::BisInflation,
        Dataset::Postcodes => &sources::geonames::GeoNames,
        Dataset::EuRegions => &sources::eurostat::Eurostat,
        Dataset::DePostcodeMunicipality => &sources::openplz::OpenPlz,
    }
}

/// The validators a stored file was downloaded with; none when it is missing.
pub fn validators_of(path: &Path) -> download::Validators {
    let meta = store::read_meta(path).unwrap_or_default();
    download::Validators {
        etag: meta.get(store::META_ETAG).cloned(),
        last_modified: meta.get(store::META_LAST_MODIFIED).cloned(),
    }
}

pub fn footer(d: Dataset, url: &str, v: &download::Validators) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(store::META_SOURCE.to_string(), url.to_string());
    m.insert(
        store::META_DOWNLOADED.to_string(),
        Utc::now().format("%Y-%m-%d %H:%M").to_string(),
    );
    m.insert(store::META_LICENCE.to_string(), d.credit().to_string());
    if let Some(e) = &v.etag {
        m.insert(store::META_ETAG.to_string(), e.clone());
    }
    if let Some(l) = &v.last_modified {
        m.insert(store::META_LAST_MODIFIED.to_string(), l.clone());
    }
    m
}

pub fn with_zip_entry<T>(
    zip: &Path,
    pick: impl Fn(&str) -> bool,
    read: impl FnOnce(&mut dyn Read) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    let name = archive
        .file_names()
        .filter_map(Result::ok)
        .find(|n| pick(n))
        .map(|n| n.into_owned())
        .ok_or_else(|| anyhow::anyhow!("the download holds no matching file"))?;
    let mut entry = archive.by_name(&name)?;
    read(&mut entry)
}

/// Is a switched-on dataset due for a refresh? Postcodes with nothing stored
/// wait for the first country used.
pub fn due(d: Dataset, dir: &Path, s: &SourceSettings, now: SystemTime) -> bool {
    let files = source(d).files(dir);
    if files.is_empty() {
        return d != Dataset::Postcodes;
    }
    let oldest = files.iter().map(|f| store::modified(f)).min().flatten();
    is_stale(oldest, s, now)
}

pub fn last_downloaded(d: Dataset, dir: &Path) -> Option<String> {
    source(d)
        .files(dir)
        .iter()
        .filter_map(|f| {
            store::read_meta(f)
                .ok()?
                .get(store::META_DOWNLOADED)
                .cloned()
        })
        .max()
}

pub fn stored_size(d: Dataset, dir: &Path) -> u64 {
    store::size_of(&source(d).files(dir))
}

pub fn delete(d: Dataset, dir: &Path) -> std::io::Result<()> {
    for f in source(d).files(dir) {
        if f.exists() {
            std::fs::remove_file(f)?;
        }
    }
    Ok(())
}

pub fn ensure_enabled(r: &ReferenceSettings, d: Dataset) -> anyhow::Result<()> {
    if r.get(d).enabled {
        Ok(())
    } else {
        anyhow::bail!("{}", off_message(d))
    }
}

pub fn is_month(key: &str) -> bool {
    let b = key.as_bytes();
    b.len() == 7 && b[4] == b'-' && b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit)
}

pub fn is_day(key: &str) -> bool {
    chrono::NaiveDate::parse_from_str(key, "%Y-%m-%d").is_ok() && key.len() == 10
}

pub fn prepare_in(
    dir: &Path,
    base: Option<&str>,
    r: &ReferenceSettings,
    needs: &[Dataset],
    countries: &[String],
    now: SystemTime,
) -> anyhow::Result<Vec<String>> {
    let mut warnings = Vec::new();
    for &d in needs {
        ensure_enabled(r, d)?;
        let src = source(d);
        let stale = due(d, dir, r.get(d), now);
        let (todo, usable): (Option<Vec<String>>, bool) = if d == Dataset::Postcodes {
            let missing: Vec<String> = countries
                .iter()
                .filter(|cc| !sources::geonames::file(dir, cc).exists())
                .cloned()
                .collect();
            let mut todo = missing.clone();
            if stale {
                todo.extend(sources::geonames::stored(dir));
            }
            todo.sort();
            todo.dedup();
            ((!todo.is_empty()).then_some(todo), missing.is_empty())
        } else {
            let files = src.files(dir);
            let present = files.iter().all(|f| f.exists());
            ((stale || !present).then(Vec::new), present)
        };
        let Some(todo) = todo else {
            continue;
        };
        if let Err(e) = src.refresh_with(dir, &todo, base) {
            let msg = format!("{}: {e:#}", crate::i18n::t_english(&d.label_key()));
            if usable {
                warnings.push(msg);
            } else {
                anyhow::bail!("{msg}");
            }
        }
    }
    Ok(warnings)
}

pub fn prepare(
    r: &ReferenceSettings,
    needs: &[Dataset],
    countries: &[String],
) -> anyhow::Result<(PathBuf, Vec<String>)> {
    let dir = store::dir()?;
    let base = std::env::var(download::BASE_ENV).ok();
    let warnings = prepare_in(
        &dir,
        base.as_deref(),
        r,
        needs,
        countries,
        SystemTime::now(),
    )?;
    Ok((dir, warnings))
}

/// One lookup, as the dialog, `octa --reference` and the MCP tool ask for it.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Currency(crate::data::reference::currency::CurrencySpec),
    Inflation(crate::data::reference::inflation::InflationSpec),
    Places(crate::data::reference::places::PlacesSpec),
}

/// Get what the request needs ready (refusing what is switched off), load
/// it and compute the new columns. The one path every surface takes.
pub fn lookup_in(
    dir: &Path,
    base: Option<&str>,
    table: &crate::data::DataTable,
    req: &Request,
    r: &ReferenceSettings,
    now: SystemTime,
) -> anyhow::Result<(crate::data::reference::Computed, Vec<String>)> {
    use crate::data::reference::{currency, inflation, places};
    match req {
        Request::Currency(spec) => {
            let warn = prepare_in(dir, base, r, &[Dataset::ExchangeRates], &[], now)?;
            Ok((
                currency::currency_columns(table, &currency::Rates::load(dir)?, spec)?,
                warn,
            ))
        }
        Request::Inflation(spec) => {
            let warn = prepare_in(dir, base, r, &[Dataset::Inflation], &[], now)?;
            Ok((
                inflation::inflation_columns(table, &inflation::Indices::load(dir)?, spec)?,
                warn,
            ))
        }
        Request::Places(spec) => {
            let needs = places::needs(r, spec.kind, &spec.outputs)
                .map_err(|d| anyhow::anyhow!(off_message(d)))?;
            let countries = places::countries_in(table, spec);
            let warn = prepare_in(dir, base, r, &needs, &countries, now)?;
            Ok((
                places::place_columns(table, &places::Places::load(dir, &countries)?, spec)?,
                warn,
            ))
        }
    }
}

pub fn lookup(
    table: &crate::data::DataTable,
    req: &Request,
    r: &ReferenceSettings,
) -> anyhow::Result<(crate::data::reference::Computed, Vec<String>)> {
    let dir = store::dir()?;
    let base = std::env::var(download::BASE_ENV).ok();
    lookup_in(&dir, base.as_deref(), table, req, r, SystemTime::now())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
