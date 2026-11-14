//! GeoNames postcodes (CC BY 4.0), one file per country, fetched on first use.
//! Full postcodes for the UK, the Netherlands and Canada come from their
//! `_full` files.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::bail;

use crate::data::decimal::Exact;
use crate::reference::download::{self, Fetched};
use crate::reference::merge::{self, Groups, Kind};
use crate::reference::{
    Dataset, ReferenceSource, Refreshed, footer, store, validators_of, with_zip_entry,
};

/// Every country and territory GeoNames publishes postcodes for (2026-11-13).
pub const COUNTRIES: [&str; 124] = [
    "AD", "AE", "AI", "AL", "AM", "AR", "AS", "AT", "AU", "AX", "AZ", "BD", "BE", "BG", "BM", "BR",
    "BY", "CA", "CC", "CH", "CL", "CN", "CO", "CR", "CX", "CY", "CZ", "DE", "DK", "DO", "DZ", "EC",
    "EE", "ES", "FI", "FK", "FM", "FO", "FR", "GB", "GF", "GG", "GI", "GL", "GP", "GS", "GT", "GU",
    "HK", "HM", "HN", "HR", "HT", "HU", "ID", "IE", "IM", "IN", "IO", "IS", "IT", "JE", "JP", "KE",
    "KR", "LI", "LK", "LT", "LU", "LV", "MA", "MC", "MD", "MH", "MK", "MN", "MO", "MP", "MQ", "MT",
    "MW", "MX", "MY", "NC", "NF", "NL", "NO", "NR", "NU", "NZ", "PA", "PE", "PF", "PH", "PK", "PL",
    "PM", "PN", "PR", "PT", "PW", "PY", "RE", "RO", "RS", "RU", "SE", "SG", "SI", "SJ", "SK", "SM",
    "TC", "TH", "TR", "UA", "US", "UY", "VA", "VI", "WF", "WS", "YT", "ZA",
];
pub const FULL: [&str; 3] = ["CA", "GB", "NL"];
pub const NAMES: [&str; 8] = [
    "postcode",
    "place",
    "region",
    "admin2",
    "district_code",
    "district",
    "lat",
    "lon",
];
const KINDS: [Kind; 8] = [
    Kind::Text,
    Kind::Text,
    Kind::Text,
    Kind::Text,
    Kind::Text,
    Kind::Text,
    Kind::Int,
    Kind::Int,
];

pub struct GeoNames;

pub fn url(cc: &str) -> String {
    if FULL.contains(&cc) {
        format!("https://download.geonames.org/export/zip/{cc}_full.csv.zip")
    } else {
        format!("https://download.geonames.org/export/zip/{cc}.zip")
    }
}

pub fn file(dir: &Path, cc: &str) -> PathBuf {
    dir.join("postcodes").join(format!("{cc}.parquet"))
}

pub fn stored(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir.join("postcodes"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            Some(name.strip_suffix(".parquet")?.to_string())
        })
        .filter(|cc| COUNTRIES.contains(&cc.as_str()))
        .collect();
    out.sort();
    out
}

pub fn norm_postcode(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn e4(s: &str) -> Option<String> {
    let raw = Exact::parse(s.trim())?.round_to(4).raw_scaled(4)?;
    Some(i32::try_from(raw).ok()?.to_string())
}

fn some(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn parse(txt: impl Read) -> anyhow::Result<Groups> {
    let mut groups = Groups::new();
    for line in BufReader::new(txt).lines() {
        let line = line?;
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 11 || f[1].trim().is_empty() {
            continue;
        }
        let row = vec![
            some(f[1]),
            some(f[2]),
            some(f[3]),
            some(f[5]),
            some(f[8]),
            some(f[7]),
            e4(f[9]),
            e4(f[10]),
        ];
        groups.entry(norm_postcode(f[1])).or_default().push(row);
    }
    Ok(groups)
}

impl ReferenceSource for GeoNames {
    fn dataset(&self) -> Dataset {
        Dataset::Postcodes
    }

    fn files(&self, dir: &Path) -> Vec<PathBuf> {
        stored(dir).iter().map(|cc| file(dir, cc)).collect()
    }

    fn refresh_with(
        &self,
        dir: &Path,
        countries: &[String],
        base: Option<&str>,
    ) -> anyhow::Result<Refreshed> {
        let wanted: Vec<String> = if countries.is_empty() {
            stored(dir)
        } else {
            countries.to_vec()
        };
        if let Some(bad) = wanted.iter().find(|cc| !COUNTRIES.contains(&cc.as_str())) {
            bail!("GeoNames publishes no postcodes for {bad}");
        }
        let mut out = Refreshed::default();
        for cc in &wanted {
            let path = file(dir, cc);
            let address = url(cc);
            match download::fetch(
                &download::resolve_with(&address, base),
                &validators_of(&path),
            )? {
                Fetched::NotModified => {
                    store::touch(&path)?;
                    out.unchanged += 1;
                }
                Fetched::Body {
                    file: tmp,
                    validators,
                } => {
                    let fresh = with_zip_entry(
                        tmp.path(),
                        |n| n.ends_with(".txt") && !n.contains("readme"),
                        |r| parse(r),
                    )?;
                    if fresh.is_empty() {
                        bail!("{address}: no postcodes in the download");
                    }
                    let old = if path.exists() {
                        merge::table_to_groups(&store::read(&path)?, |r| {
                            norm_postcode(r[0].as_deref().unwrap_or(""))
                        })
                    } else {
                        Groups::new()
                    };
                    let merged = merge::merge_groups(&old, &fresh, &[1]);
                    store::write(
                        &path,
                        &merge::groups_to_table(
                            &NAMES,
                            &KINDS,
                            &merged,
                            footer(self.dataset(), &address, &validators),
                        ),
                    )?;
                    out.downloaded += 1;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "geonames_tests.rs"]
mod tests;
