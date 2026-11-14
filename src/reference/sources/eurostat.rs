//! Eurostat: GISCO postcode points (CC BY-SA 4.0) for 34 countries, the
//! official municipality to NUTS workbook, and NUTS names. Postcodes give one
//! municipality each (the one their point lies in); the workbook gives every
//! municipality its NUTS 3 region.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use arrow::array::{Array, StringArray};
use arrow::datatypes::DataType;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::reference::download::{self, Fetched};
use crate::reference::merge::{self, Groups, Kind, Row};
use crate::reference::sources::geonames::norm_postcode;
use crate::reference::{Dataset, ReferenceSource, Refreshed, footer, store, validators_of};

pub const POSTCODES: &str = "eu_postcodes.parquet";
pub const MUNICIPALITIES: &str = "eu_municipalities.parquet";
pub const NUTS: &str = "nuts.parquet";

const PCODE_DATASETS: &str =
    "https://gisco-services.ec.europa.eu/distribution/v2/pcode/datasets.json";
const NUTS_DATASETS: &str =
    "https://gisco-services.ec.europa.eu/distribution/v2/nuts/datasets.json";
const LAU_PAGE: &str = "https://ec.europa.eu/eurostat/web/nuts/local-administrative-units";
const WORKBOOK_DIR: &str = "https://ec.europa.eu/eurostat/documents/345175/501971/";

pub struct Eurostat;

pub fn iso2(code: &str) -> String {
    match code {
        "EL" => "GR".to_string(),
        "UK" => "GB".to_string(),
        other => other.to_string(),
    }
}

pub fn newest_year(datasets_json: &str, prefix: &str) -> Option<u32> {
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(datasets_json).ok()?;
    map.keys()
        .filter_map(|k| k.strip_prefix(prefix)?.strip_prefix('-')?.parse().ok())
        .max()
}

pub fn newest_workbook(page: &str) -> Option<String> {
    let re = regex::Regex::new(r"EU-27-LAU-(\d{4})-NUTS-(\d{4})\.xlsx").ok()?;
    re.captures_iter(page)
        .filter_map(|c| {
            Some((
                (c[1].parse::<u32>().ok()?, c[2].parse::<u32>().ok()?),
                c[0].to_string(),
            ))
        })
        .max()
        .map(|(_, name)| name)
}

fn unquote(s: &str) -> Option<String> {
    let s = s.trim().trim_matches('"').trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// `DE_05166036` -> `05166036`.
fn strip_country(id: &str) -> String {
    match id.split_once('_') {
        Some((cc, rest)) if cc.len() == 2 => rest.to_string(),
        _ => id.to_string(),
    }
}

pub fn parse_postcodes(path: &Path) -> anyhow::Result<Groups> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(path)?)?;
    let names: Vec<String> = builder
        .parquet_schema()
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let find = |want: &str, prefix: bool| {
        names
            .iter()
            .find(|n| {
                if prefix {
                    n.starts_with(want)
                } else {
                    n.as_str() == want
                }
            })
            .cloned()
            .with_context(|| format!("the postcode file has no {want} column"))
    };
    let wanted = [
        find("POSTCODE", false)?,
        find("CNTR_ID", false)?,
        find("NUTS3_", true)?,
        find("GISCO_", true)?,
    ];
    let mask = ProjectionMask::columns(builder.parquet_schema(), wanted.iter().map(String::as_str));
    let mut groups = Groups::new();
    for batch in builder.with_projection(mask).build()? {
        let batch = batch?;
        let cols: Vec<StringArray> = wanted
            .iter()
            .map(|n| {
                let a = batch.column_by_name(n).context("missing column")?;
                let a = arrow::compute::cast(a, &DataType::Utf8)?;
                Ok(a.as_any()
                    .downcast_ref::<StringArray>()
                    .context("text column")?
                    .clone())
            })
            .collect::<anyhow::Result<_>>()?;
        for r in 0..batch.num_rows() {
            let get = |c: usize| (!cols[c].is_null(r)).then(|| cols[c].value(r).to_string());
            let (Some(pc), Some(cc)) = (
                get(0).and_then(|s| unquote(&s)),
                get(1).and_then(|s| unquote(&s)),
            ) else {
                continue;
            };
            let cc = iso2(&cc);
            let row: Row = vec![
                Some(cc.clone()),
                Some(pc.clone()),
                get(2).and_then(|s| unquote(&s)),
                get(3).and_then(|s| unquote(&s)).map(|s| strip_country(&s)),
            ];
            groups
                .entry(format!("{cc}|{}", norm_postcode(&pc)))
                .or_default()
                .push(row);
        }
    }
    Ok(groups)
}

pub fn parse_workbook(path: &Path) -> anyhow::Result<Groups> {
    use calamine::{Reader, Xlsx, open_workbook};
    // The download has no .xlsx extension, so name the reader.
    let mut wb: Xlsx<_> =
        open_workbook(path).with_context(|| format!("opening {}", path.display()))?;
    let mut groups = Groups::new();
    let sheets: Vec<String> = wb
        .sheet_names()
        .into_iter()
        .filter(|n| n.len() == 2 && n.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    for sheet in sheets {
        let range = wb.worksheet_range(&sheet)?;
        let mut rows = range.rows();
        let Some(header) = rows.find(|r| r.iter().any(|c| c.to_string().trim() == "NUTS3")) else {
            continue;
        };
        let at = |name: &str| header.iter().position(|c| c.to_string().trim() == name);
        let (Some(nuts), Some(code)) = (at("NUTS3"), at("EU LAU CODE")) else {
            continue;
        };
        let (national, latin) = (at("LAU NAME NATIONAL"), at("LAU NAME LATIN"));
        let cc = iso2(&sheet);
        for r in rows {
            let cell = |i: Option<usize>| {
                i.and_then(|i| r.get(i))
                    .and_then(|c| unquote(&c.to_string()))
            };
            let Some(id) = cell(Some(code)) else {
                continue;
            };
            let muni = strip_country(&id);
            // Stored only where it differs: most countries repeat the national name.
            let name = cell(national);
            let latin = cell(latin).filter(|l| name.as_ref() != Some(l));
            let row: Row = vec![
                Some(cc.clone()),
                Some(muni.clone()),
                name,
                latin,
                cell(Some(nuts)),
            ];
            groups.entry(format!("{cc}|{muni}")).or_default().push(row);
        }
    }
    Ok(groups)
}

pub fn parse_nuts(csv: impl Read) -> anyhow::Result<Groups> {
    let mut r = csv::Reader::from_reader(csv);
    let headers = r.headers()?.clone();
    let at = |n: &str| {
        headers
            .iter()
            .position(|h| h == n)
            .with_context(|| format!("no {n} column"))
    };
    let (id, name) = (at("NUTS_ID")?, at("NAME_LATN")?);
    let mut groups = Groups::new();
    for rec in r.records() {
        let rec = rec?;
        if let (Some(code), name) = (
            rec.get(id).and_then(unquote),
            rec.get(name).and_then(unquote),
        ) {
            groups.insert(code.clone(), vec![vec![Some(code), name]]);
        }
    }
    Ok(groups)
}

/// Download `address` into `path` through `parse`, merging with what is there.
fn one(
    path: &Path,
    address: &str,
    base: Option<&str>,
    names: &[&str],
    key_of: impl Fn(&Row) -> String,
    parse: impl FnOnce(&Path) -> anyhow::Result<Groups>,
    out: &mut Refreshed,
) -> anyhow::Result<()> {
    // A new year's file is a new address: download it whole, no validators.
    let known = match store::read_meta(path) {
        Ok(m) if m.get(store::META_SOURCE).map(String::as_str) == Some(address) => {
            validators_of(path)
        }
        _ => download::Validators::default(),
    };
    match download::fetch(&download::resolve_with(address, base), &known)? {
        Fetched::NotModified => {
            store::touch(path)?;
            out.unchanged += 1;
        }
        Fetched::Body { file, validators } => {
            let fresh = parse(file.path())?;
            if fresh.is_empty() {
                bail!("{address}: nothing usable in the download");
            }
            let old = if path.exists() {
                merge::table_to_groups(&store::read(path)?, &key_of)
            } else {
                Groups::new()
            };
            let merged = merge::merge_groups(&old, &fresh, &[0]);
            let kinds = vec![Kind::Text; names.len()];
            store::write(
                path,
                &merge::groups_to_table(
                    names,
                    &kinds,
                    &merged,
                    footer(Dataset::EuRegions, address, &validators),
                ),
            )?;
            out.downloaded += 1;
        }
    }
    Ok(())
}

impl ReferenceSource for Eurostat {
    fn dataset(&self) -> Dataset {
        Dataset::EuRegions
    }

    fn files(&self, dir: &Path) -> Vec<PathBuf> {
        vec![
            dir.join(POSTCODES),
            dir.join(MUNICIPALITIES),
            dir.join(NUTS),
        ]
    }

    fn refresh_with(
        &self,
        dir: &Path,
        _: &[String],
        base: Option<&str>,
    ) -> anyhow::Result<Refreshed> {
        let fetch_text = |url: &str| download::fetch_text(&download::resolve_with(url, base));
        let pc_year = newest_year(&fetch_text(PCODE_DATASETS)?, "pcode")
            .context("no postcode dataset listed by GISCO")?;
        let nuts_year = newest_year(&fetch_text(NUTS_DATASETS)?, "nuts")
            .context("no NUTS dataset listed by GISCO")?;
        let workbook = newest_workbook(&fetch_text(LAU_PAGE)?)
            .context("no municipality to NUTS workbook on Eurostat's page")?;
        let mut out = Refreshed::default();
        one(
            &dir.join(POSTCODES),
            &format!(
                "https://gisco-services.ec.europa.eu/distribution/v2/pcode/parquet/PCODE_PT_{pc_year}_4326.parquet"
            ),
            base,
            &["country", "postcode", "nuts3", "municipality"],
            |r| {
                format!(
                    "{}|{}",
                    r[0].as_deref().unwrap_or(""),
                    norm_postcode(r[1].as_deref().unwrap_or(""))
                )
            },
            parse_postcodes,
            &mut out,
        )?;
        one(
            &dir.join(MUNICIPALITIES),
            &format!("{WORKBOOK_DIR}{workbook}"),
            base,
            &["country", "municipality", "name", "name_latin", "nuts3"],
            |r| {
                format!(
                    "{}|{}",
                    r[0].as_deref().unwrap_or(""),
                    r[1].as_deref().unwrap_or("")
                )
            },
            parse_workbook,
            &mut out,
        )?;
        one(
            &dir.join(NUTS),
            &format!(
                "https://gisco-services.ec.europa.eu/distribution/v2/nuts/csv/NUTS_AT_{nuts_year}.csv"
            ),
            base,
            &["nuts", "name"],
            |r| r[0].clone().unwrap_or_default(),
            |p| parse_nuts(std::fs::File::open(p)?),
            &mut out,
        )?;
        Ok(out)
    }
}

#[cfg(test)]
#[path = "eurostat_tests.rs"]
mod tests;
