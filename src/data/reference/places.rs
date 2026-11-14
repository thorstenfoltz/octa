//! Places and region keys: postcode, municipality (the Gemeindeschlüssel in
//! Germany), Kreisschlüssel and NUTS code, each one a way in and a way out.
//! Many-to-many both ways: a Kreis holds many postcodes, a postcode can lie in
//! several municipalities. In Germany OpenPLZ pairs postcodes and
//! municipalities exactly; elsewhere Eurostat gives each postcode the one
//! municipality its point lies in.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::data::decimal::Exact;
use crate::data::{CellValue, DataTable};
use crate::reference::Dataset;
use crate::reference::merge;
use crate::reference::sources::{eurostat, geonames, openplz};
use crate::reference::store::{self, RefTable};

use super::{Computed, NewColumn, Pick, Several, cell_text, country_code, join};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Postcode,
    Municipality,
    Kreis,
    Nuts,
}

impl KeyKind {
    pub const ALL: [KeyKind; 4] = [
        KeyKind::Postcode,
        KeyKind::Municipality,
        KeyKind::Kreis,
        KeyKind::Nuts,
    ];

    pub fn id(self) -> &'static str {
        match self {
            KeyKind::Postcode => "postcode",
            KeyKind::Municipality => "municipality",
            KeyKind::Kreis => "kreis",
            KeyKind::Nuts => "nuts",
        }
    }

    pub fn parse(s: &str) -> Option<KeyKind> {
        Self::ALL
            .into_iter()
            .find(|k| k.id() == s.trim().to_ascii_lowercase())
    }

    pub fn label_key(self) -> String {
        format!("refdata.key_{}", self.id())
    }

    /// Datasets that can answer this kind of key; greyed while all are off.
    pub fn sources(self) -> &'static [Dataset] {
        match self {
            KeyKind::Postcode => &[
                Dataset::Postcodes,
                Dataset::EuRegions,
                Dataset::DePostcodeMunicipality,
            ],
            KeyKind::Municipality | KeyKind::Kreis => {
                &[Dataset::EuRegions, Dataset::DePostcodeMunicipality]
            }
            KeyKind::Nuts => &[Dataset::EuRegions],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Place,
    Region,
    Latitude,
    Longitude,
    Postcode,
    Municipality,
    MunicipalityName,
    Kreis,
    KreisName,
    Nuts1,
    Nuts1Name,
    Nuts2,
    Nuts2Name,
    Nuts3,
    Nuts3Name,
}

impl Output {
    pub const ALL: [Output; 15] = [
        Output::Place,
        Output::Region,
        Output::Latitude,
        Output::Longitude,
        Output::Postcode,
        Output::Municipality,
        Output::MunicipalityName,
        Output::Kreis,
        Output::KreisName,
        Output::Nuts1,
        Output::Nuts1Name,
        Output::Nuts2,
        Output::Nuts2Name,
        Output::Nuts3,
        Output::Nuts3Name,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Output::Place => "place",
            Output::Region => "region",
            Output::Latitude => "latitude",
            Output::Longitude => "longitude",
            Output::Postcode => "postcode",
            Output::Municipality => "municipality",
            Output::MunicipalityName => "municipality_name",
            Output::Kreis => "kreis",
            Output::KreisName => "kreis_name",
            Output::Nuts1 => "nuts1",
            Output::Nuts1Name => "nuts1_name",
            Output::Nuts2 => "nuts2",
            Output::Nuts2Name => "nuts2_name",
            Output::Nuts3 => "nuts3",
            Output::Nuts3Name => "nuts3_name",
        }
    }

    pub fn parse(s: &str) -> Option<Output> {
        Self::ALL
            .into_iter()
            .find(|o| o.id() == s.trim().to_ascii_lowercase())
    }

    pub fn label_key(self) -> String {
        format!("refdata.out_{}", self.id())
    }

    pub fn sources(self) -> &'static [Dataset] {
        match self {
            Output::Place
            | Output::Region
            | Output::Latitude
            | Output::Longitude
            | Output::KreisName => &[Dataset::Postcodes],
            Output::Postcode | Output::Municipality | Output::Kreis => {
                &[Dataset::DePostcodeMunicipality, Dataset::EuRegions]
            }
            _ => &[Dataset::EuRegions],
        }
    }

    fn nuts_len(self) -> Option<usize> {
        match self {
            Output::Nuts1 | Output::Nuts1Name => Some(3),
            Output::Nuts2 | Output::Nuts2Name => Some(4),
            Output::Nuts3 | Output::Nuts3Name => Some(5),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeoRow {
    postcode: String,
    place: String,
    region: String,
    kreis: String,
    lat: Option<i32>,
    lon: Option<i32>,
}

/// One Eurostat postcode point: (postcode as written, NUTS 3, municipality).
type EuPoint = (String, String, String);

#[derive(Debug, Default)]
pub struct Places {
    geo: HashMap<String, HashMap<String, Vec<GeoRow>>>,
    pc_width: HashMap<String, usize>,
    /// cc -> normalised postcode -> (postcode, nuts3, municipality)
    eu_pc: HashMap<String, HashMap<String, Vec<EuPoint>>>,
    /// cc -> municipality -> postcodes
    eu_by_muni: HashMap<String, HashMap<String, BTreeSet<String>>>,
    /// cc -> municipality -> (name, nuts3)
    munis: HashMap<String, BTreeMap<String, (String, String)>>,
    nuts: HashMap<String, String>,
    de_pc: HashMap<String, BTreeSet<String>>,
    de_muni: BTreeMap<String, BTreeSet<String>>,
    kreis_names: HashMap<String, String>,
}

fn rows_of(t: &RefTable) -> Vec<Vec<String>> {
    merge::table_to_groups(t, |_| String::new())
        .into_values()
        .flatten()
        .map(|r| r.into_iter().map(Option::unwrap_or_default).collect())
        .collect()
}

fn read_if(path: &Path) -> anyhow::Result<Vec<Vec<String>>> {
    if path.exists() {
        Ok(rows_of(&store::read(path)?))
    } else {
        Ok(Vec::new())
    }
}

fn uniform_digits<'a>(keys: impl Iterator<Item = &'a String>) -> Option<usize> {
    let mut width = None;
    for k in keys {
        if !k.bytes().all(|b| b.is_ascii_digit()) || width.is_some_and(|w| w != k.len()) {
            return None;
        }
        width = Some(k.len());
    }
    width
}

fn pad(s: &str, width: Option<usize>) -> String {
    match width {
        Some(w) if s.len() < w && s.bytes().all(|b| b.is_ascii_digit()) => format!("{s:0>w$}"),
        _ => s.to_string(),
    }
}

#[derive(Debug, Default)]
struct Hit<'a> {
    postcodes: BTreeSet<String>,
    places: Vec<&'a GeoRow>,
    munis: BTreeSet<String>,
    nuts3: BTreeSet<String>,
    kreise: BTreeSet<String>,
}

impl Places {
    /// Load the stored files: GeoNames for `countries` (those stored), and
    /// the European and German tables when present.
    pub fn load(dir: &Path, countries: &[String]) -> anyhow::Result<Places> {
        let mut p = Places::default();
        for cc in countries {
            let mut by: HashMap<String, Vec<GeoRow>> = HashMap::new();
            for r in read_if(&geonames::file(dir, cc))? {
                let g = GeoRow {
                    postcode: r[0].clone(),
                    place: r[1].clone(),
                    region: r[2].clone(),
                    kreis: if cc == "DE" {
                        r[4].clone()
                    } else {
                        String::new()
                    },
                    lat: r[6].parse().ok(),
                    lon: r[7].parse().ok(),
                };
                if cc == "DE" && !g.kreis.is_empty() {
                    p.kreis_names
                        .entry(g.kreis.clone())
                        .or_insert_with(|| r[5].clone());
                }
                by.entry(geonames::norm_postcode(&g.postcode))
                    .or_default()
                    .push(g);
            }
            if let Some(w) = uniform_digits(by.keys()) {
                p.pc_width.insert(cc.clone(), w);
            }
            p.geo.insert(cc.clone(), by);
        }
        for r in read_if(&dir.join(eurostat::POSTCODES))? {
            let (cc, pc, n3, m) = (r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone());
            if !m.is_empty() {
                p.eu_by_muni
                    .entry(cc.clone())
                    .or_default()
                    .entry(m.clone())
                    .or_default()
                    .insert(pc.clone());
            }
            p.eu_pc
                .entry(cc)
                .or_default()
                .entry(geonames::norm_postcode(&pc))
                .or_default()
                .push((pc, n3, m));
        }
        for (cc, by) in &p.eu_pc {
            if !p.pc_width.contains_key(cc)
                && let Some(w) = uniform_digits(by.keys())
            {
                p.pc_width.insert(cc.clone(), w);
            }
        }
        for r in read_if(&dir.join(eurostat::MUNICIPALITIES))? {
            let name = if r[2].is_empty() {
                r[3].clone()
            } else {
                r[2].clone()
            };
            p.munis
                .entry(r[0].clone())
                .or_default()
                .insert(r[1].clone(), (name, r[4].clone()));
        }
        for r in read_if(&dir.join(eurostat::NUTS))? {
            p.nuts.insert(r[0].clone(), r[1].clone());
        }
        for r in read_if(&dir.join(openplz::FILE))? {
            p.de_pc
                .entry(r[0].clone())
                .or_default()
                .insert(r[1].clone());
            p.de_muni
                .entry(r[1].clone())
                .or_default()
                .insert(r[0].clone());
        }
        Ok(p)
    }

    fn muni_width(&self, cc: &str) -> Option<usize> {
        if cc == "DE" {
            return Some(8);
        }
        self.munis.get(cc).and_then(|m| uniform_digits(m.keys()))
    }

    fn finish<'a>(&'a self, cc: &str, mut h: Hit<'a>) -> Hit<'a> {
        for m in &h.munis {
            if let Some((_, n3)) = self.munis.get(cc).and_then(|x| x.get(m))
                && !n3.is_empty()
            {
                h.nuts3.insert(n3.clone());
            }
            if cc == "DE" && m.len() >= 5 {
                h.kreise.insert(m[..5].to_string());
            }
        }
        h
    }

    fn places_of<'a>(&'a self, cc: &str, h: &mut Hit<'a>) {
        for pc in &h.postcodes {
            if let Some(rows) = self
                .geo
                .get(cc)
                .and_then(|m| m.get(&geonames::norm_postcode(pc)))
            {
                h.places.extend(rows);
            }
        }
    }

    fn by_postcode<'a>(&'a self, cc: &str, raw: &str) -> Hit<'a> {
        let norm = pad(
            &geonames::norm_postcode(raw),
            self.pc_width.get(cc).copied(),
        );
        let mut h = Hit::default();
        if let Some(rows) = self.geo.get(cc).and_then(|m| m.get(&norm)) {
            h.places.extend(rows);
            h.postcodes.extend(rows.iter().map(|g| g.postcode.clone()));
            h.kreise.extend(
                rows.iter()
                    .map(|g| g.kreis.clone())
                    .filter(|k| !k.is_empty()),
            );
        }
        let exact_de = cc == "DE" && self.de_pc.contains_key(&norm);
        if let Some(ms) = self.de_pc.get(&norm).filter(|_| cc == "DE") {
            h.munis.extend(ms.iter().cloned());
            h.postcodes.insert(norm.clone());
        }
        if let Some(rows) = self.eu_pc.get(cc).and_then(|m| m.get(&norm)) {
            for (pc, n3, m) in rows {
                h.postcodes.insert(pc.clone());
                if !n3.is_empty() {
                    h.nuts3.insert(n3.clone());
                }
                if !exact_de && !m.is_empty() {
                    h.munis.insert(m.clone());
                }
            }
        }
        self.finish(cc, h)
    }

    fn by_municipalities<'a>(&'a self, cc: &str, munis: BTreeSet<String>) -> Hit<'a> {
        let mut h = Hit {
            munis,
            ..Hit::default()
        };
        for m in &h.munis {
            if cc == "DE"
                && let Some(pcs) = self.de_muni.get(m)
            {
                h.postcodes.extend(pcs.iter().cloned());
            }
            if (cc != "DE" || self.de_muni.is_empty())
                && let Some(pcs) = self.eu_by_muni.get(cc).and_then(|x| x.get(m))
            {
                h.postcodes.extend(pcs.iter().cloned());
            }
        }
        // A country without the workbook (Iceland): NUTS 3 from the postcode points.
        if h.munis
            .iter()
            .all(|m| self.munis.get(cc).and_then(|x| x.get(m)).is_none())
        {
            for pc in &h.postcodes {
                if let Some(rows) = self
                    .eu_pc
                    .get(cc)
                    .and_then(|x| x.get(&geonames::norm_postcode(pc)))
                {
                    h.nuts3
                        .extend(rows.iter().map(|r| r.1.clone()).filter(|s| !s.is_empty()));
                }
            }
        }
        self.places_of(cc, &mut h);
        self.finish(cc, h)
    }

    fn by_kreis(&self, raw: &str) -> Hit<'_> {
        let k = pad(raw.trim(), Some(5));
        let mut munis: BTreeSet<String> = self
            .munis
            .get("DE")
            .map(|x| {
                x.range(k.clone()..)
                    .take_while(|(c, _)| c.starts_with(&k))
                    .map(|(c, _)| c.clone())
                    .collect()
            })
            .unwrap_or_default();
        munis.extend(
            self.de_muni
                .range(k.clone()..)
                .take_while(|(c, _)| c.starts_with(&k))
                .map(|(c, _)| c.clone()),
        );
        self.by_municipalities("DE", munis)
    }

    fn by_nuts(&self, raw: &str) -> (String, Hit<'_>) {
        let code = raw.trim().to_ascii_uppercase();
        let Some(prefix) = code.get(..2) else {
            return (String::new(), Hit::default());
        };
        let cc = eurostat::iso2(prefix);
        let munis = self
            .munis
            .get(&cc)
            .map(|x| {
                x.iter()
                    .filter(|(_, (_, n3))| n3.starts_with(&code))
                    .map(|(c, _)| c.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut h = self.by_municipalities(&cc, munis);
        h.nuts3.retain(|n| n.starts_with(&code));
        (cc, h)
    }

    /// Names in the order of their codes, one per code.
    fn names(
        &self,
        codes: Vec<String>,
        name: impl Fn(&str) -> Option<String>,
        several: Several,
    ) -> Option<String> {
        let codes: Vec<String> = match several {
            Several::All => codes,
            Several::First => codes.into_iter().take(1).collect(),
        };
        let names: Vec<String> = codes
            .iter()
            .filter_map(|c| name(c))
            .filter(|n| !n.is_empty())
            .collect();
        (!names.is_empty()).then(|| names.join(", "))
    }

    fn value(&self, out: Output, cc: &str, h: &Hit, several: Several) -> CellValue {
        let text = |v: Option<String>| v.map_or(CellValue::Null, CellValue::String);
        let nuts_codes = |len: usize| -> Vec<String> {
            h.nuts3
                .iter()
                .filter(|n| n.len() >= len)
                .map(|n| n[..len].to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        match out {
            Output::Place => text(join(h.places.iter().map(|g| g.place.clone()), several)),
            Output::Region => text(join(h.places.iter().map(|g| g.region.clone()), several)),
            Output::Latitude | Output::Longitude => {
                let mut by_place: BTreeMap<&str, i32> = BTreeMap::new();
                for g in &h.places {
                    let v = if out == Output::Latitude {
                        g.lat
                    } else {
                        g.lon
                    };
                    if let Some(v) = v {
                        by_place.entry(g.place.as_str()).or_insert(v);
                    }
                }
                let vals: Vec<i64> = match several {
                    Several::All => by_place.values().map(|v| i64::from(*v)).collect(),
                    Several::First => by_place.values().take(1).map(|v| i64::from(*v)).collect(),
                };
                if vals.is_empty() {
                    return CellValue::Null;
                }
                let (sum, n) = (vals.iter().sum::<i64>(), vals.len() as i64);
                let mean = (2 * sum + n * sum.signum()) / (2 * n); // half away from zero
                Exact::from_raw(i128::from(mean), 4).to_cell()
            }
            Output::Postcode => text(join(h.postcodes.iter().cloned(), several)),
            Output::Municipality => text(join(h.munis.iter().cloned(), several)),
            Output::MunicipalityName => text(self.names(
                h.munis.iter().cloned().collect(),
                |m| {
                    self.munis
                        .get(cc)
                        .and_then(|x| x.get(m))
                        .map(|(n, _)| n.clone())
                },
                several,
            )),
            Output::Kreis => text(join(h.kreise.iter().cloned(), several)),
            Output::KreisName => text(self.names(
                h.kreise.iter().cloned().collect(),
                |k| self.kreis_names.get(k).cloned(),
                several,
            )),
            o if o.id().ends_with("_name") => {
                let len = o.nuts_len().unwrap_or(5);
                text(self.names(nuts_codes(len), |c| self.nuts.get(c).cloned(), several))
            }
            o => text(join(nuts_codes(o.nuts_len().unwrap_or(5)), several)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacesSpec {
    pub key: usize,
    pub kind: KeyKind,
    /// Ignored for Kreis (Germany) and NUTS (the code carries its country).
    pub country: Pick,
    pub outputs: Vec<Output>,
    pub several: Several,
}

fn row_country(spec: &PlacesSpec, cell: &dyn Fn(usize) -> CellValue) -> Option<String> {
    match (&spec.kind, &spec.country) {
        (KeyKind::Kreis, _) => Some("DE".into()),
        (KeyKind::Nuts, _) => cell_text(&cell(spec.key))
            .and_then(|k| k.get(..2).map(|p| eurostat::iso2(&p.to_ascii_uppercase()))),
        (_, Pick::Fixed(c)) => country_code(c),
        (_, Pick::Column(c)) => cell_text(&cell(*c)).and_then(|t| country_code(&t)),
    }
}

/// The countries the rows need GeoNames data for.
pub fn countries_in(table: &DataTable, spec: &PlacesSpec) -> Vec<String> {
    let set: BTreeSet<String> = (0..table.row_count())
        .filter_map(|r| {
            row_country(spec, &|c| {
                table.get(r, c).cloned().unwrap_or(CellValue::Null)
            })
        })
        .filter(|cc| geonames::COUNTRIES.contains(&cc.as_str()))
        .collect();
    set.into_iter().collect()
}

pub fn place_columns(
    table: &DataTable,
    places: &Places,
    spec: &PlacesSpec,
) -> anyhow::Result<Computed> {
    anyhow::ensure!(!spec.outputs.is_empty(), "no outputs chosen");
    let n = table.row_count();
    let mut cols: Vec<Vec<CellValue>> = vec![Vec::with_capacity(n); spec.outputs.len()];
    let (mut found, mut missed) = (0, 0);
    for r in 0..n {
        let cell = |c: usize| table.get(r, c).cloned().unwrap_or(CellValue::Null);
        let Some(key) = cell_text(&cell(spec.key)) else {
            cols.iter_mut().for_each(|c| c.push(CellValue::Null));
            continue;
        };
        let (cc, hit) = match spec.kind {
            KeyKind::Nuts => places.by_nuts(&key),
            KeyKind::Kreis => ("DE".to_string(), places.by_kreis(&key)),
            KeyKind::Postcode | KeyKind::Municipality => match row_country(spec, &cell) {
                Some(cc) if spec.kind == KeyKind::Postcode => {
                    let h = places.by_postcode(&cc, &key);
                    (cc, h)
                }
                Some(cc) => {
                    let code = pad(key.trim(), places.muni_width(&cc));
                    let h = places.by_municipalities(&cc, [code].into());
                    (cc, h)
                }
                None => (String::new(), Hit::default()),
            },
        };
        let row: Vec<CellValue> = spec
            .outputs
            .iter()
            .map(|o| places.value(*o, &cc, &hit, spec.several))
            .collect();
        if row.iter().any(|v| !matches!(v, CellValue::Null)) {
            found += 1;
        } else {
            missed += 1;
        }
        for (c, v) in cols.iter_mut().zip(row) {
            c.push(v);
        }
    }
    let base = table.columns[spec.key].name.clone();
    let columns = spec
        .outputs
        .iter()
        .zip(cols)
        .map(|(o, values)| {
            let name = format!("{base}_{}", o.id());
            if matches!(o, Output::Latitude | Output::Longitude) {
                NewColumn::numbers(name, values)
            } else {
                NewColumn::text(name, values)
            }
        })
        .collect();
    Ok(Computed {
        after: spec.key,
        columns,
        found,
        missed,
    })
}

/// Is any source of this output switched on?
pub fn available(r: &crate::reference::ReferenceSettings, o: Output) -> bool {
    o.sources().iter().any(|d| r.get(*d).enabled)
}

/// The switched-on datasets this lookup uses; `Err` names one to switch on
/// when the key kind or an output has none.
pub fn needs(
    r: &crate::reference::ReferenceSettings,
    kind: KeyKind,
    outputs: &[Output],
) -> Result<Vec<Dataset>, Dataset> {
    let mut out = BTreeSet::new();
    for group in std::iter::once(kind.sources()).chain(outputs.iter().map(|o| o.sources())) {
        let on: Vec<Dataset> = group
            .iter()
            .copied()
            .filter(|d| r.get(*d).enabled)
            .collect();
        if on.is_empty() {
            return Err(group[0]);
        }
        out.extend(on);
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
