//! OpenPLZ (openplzapi.org, ODbL 1.0, from OpenStreetMap): every German street
//! with its postcode and municipality key. Only the distinct pairs are kept,
//! which is what makes postcode <-> municipality exact both ways.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::reference::download::{self, Fetched};
use crate::reference::merge::{self, Groups, Kind};
use crate::reference::{Dataset, ReferenceSource, Refreshed, footer, store, validators_of};

pub const URL: &str =
    "https://raw.githubusercontent.com/openpotato/openplzapi.data/main/src/de/osm/streets.csv";
pub const FILE: &str = "de_postcode_municipality.parquet";

pub struct OpenPlz;

fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn parse(csv: impl Read) -> anyhow::Result<Groups> {
    let mut r = csv::Reader::from_reader(csv);
    let headers = r.headers()?.clone();
    let at = |n: &str| {
        headers
            .iter()
            .position(|h| h == n)
            .with_context(|| format!("no {n} column"))
    };
    let (pc, key) = (at("PostalCode")?, at("RegionalKey")?);
    let mut pairs = BTreeSet::new();
    for rec in r.records() {
        let rec = rec?;
        let (Some(p), Some(k)) = (rec.get(pc).map(str::trim), rec.get(key).map(str::trim)) else {
            continue;
        };
        if digits(p, 5) && digits(k, 8) {
            pairs.insert((p.to_string(), k.to_string()));
        }
    }
    let mut groups = Groups::new();
    for (p, k) in pairs {
        groups
            .entry(p.clone())
            .or_default()
            .push(vec![Some(p), Some(k)]);
    }
    Ok(groups)
}

impl ReferenceSource for OpenPlz {
    fn dataset(&self) -> Dataset {
        Dataset::DePostcodeMunicipality
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
                let fresh = parse(std::fs::File::open(file.path())?)?;
                if fresh.is_empty() {
                    bail!("{URL}: no postcode and municipality pairs in the download");
                }
                let old = if path.exists() {
                    merge::table_to_groups(&store::read(&path)?, |r| {
                        r[0].clone().unwrap_or_default()
                    })
                } else {
                    Groups::new()
                };
                let merged = merge::merge_groups(&old, &fresh, &[1]);
                store::write(
                    &path,
                    &merge::groups_to_table(
                        &["postcode", "municipality"],
                        &[Kind::Text, Kind::Text],
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
#[path = "openplz_tests.rs"]
mod tests;
