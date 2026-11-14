//! Map by country: which country a cell names. Case-insensitive and
//! accent-tolerant against ISO 3166 alpha-2 and alpha-3 codes, the English
//! short and formal names, every name field Natural Earth carries in other
//! languages (about 25), and a short alias list. The outlines are Natural
//! Earth admin-0 countries at 1:110m, bundled (`assets/countries-110m.json.gz`)
//! so the map works offline. Countries only, no states or provinces.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use super::chart::{Aggregation, cell_to_f64};
use super::{CellValue, DataTable};
use crate::formats::geojson_reader::MapFeature;

const BUNDLED: &[u8] = include_bytes!("../../assets/countries-110m.json.gz");

/// Spellings Natural Earth does not carry, by ISO alpha-3.
const ALIASES: &[(&str, &str)] = &[
    ("usa", "USA"),
    ("u.s.", "USA"),
    ("u.s.a.", "USA"),
    ("united states of america", "USA"),
    ("america", "USA"),
    ("uk", "GBR"),
    ("u.k.", "GBR"),
    ("great britain", "GBR"),
    ("britain", "GBR"),
    ("england", "GBR"),
    ("scotland", "GBR"),
    ("wales", "GBR"),
    ("northern ireland", "GBR"),
    ("holland", "NLD"),
    ("the netherlands", "NLD"),
    ("russia", "RUS"),
    ("russian federation", "RUS"),
    ("czech republic", "CZE"),
    ("czechia", "CZE"),
    ("south korea", "KOR"),
    ("korea", "KOR"),
    ("republic of korea", "KOR"),
    ("north korea", "PRK"),
    ("ivory coast", "CIV"),
    ("turkiye", "TUR"),
    ("burma", "MMR"),
    ("swaziland", "SWZ"),
    ("macedonia", "MKD"),
    ("uae", "ARE"),
    ("drc", "COD"),
    ("dr congo", "COD"),
    ("vietnam", "VNM"),
    ("laos", "LAO"),
    ("syria", "SYR"),
    ("iran", "IRN"),
    ("bolivia", "BOL"),
    ("venezuela", "VEN"),
    ("tanzania", "TZA"),
    ("moldova", "MDA"),
];

pub struct Countries {
    /// English short name per outline.
    pub names: Vec<String>,
    pub iso3: Vec<String>,
    /// ISO alpha-2 per outline, empty when Natural Earth has none.
    pub iso2: Vec<String>,
    pub features: Vec<MapFeature>,
    index: HashMap<String, usize>,
}

/// Lower case, accents folded, every non-alphanumeric character treated as a
/// space, whitespace trimmed and collapsed: `Côte  d'Ivoire` and
/// `cote d\u{2019}ivoire` meet. Word order is not folded, so `Korea, Republic
/// of` needs an alias.
pub fn normalise(s: &str) -> String {
    let folded: String = s
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .to_lowercase();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Countries {
    pub fn find(&self, text: &str) -> Option<usize> {
        self.index.get(&normalise(text)).copied()
    }

    fn load() -> Self {
        let mut raw = String::new();
        flate2::read::GzDecoder::new(BUNDLED)
            .read_to_string(&mut raw)
            .expect("the bundled outlines are valid gzip");
        let (table, extras) =
            crate::formats::geojson_reader::read_str_with_features(&raw, "countries-110m")
                .expect("the bundled outlines are valid GeoJSON");
        let col = |name: &str| table.columns.iter().position(|c| c.name == name);
        let text = |r: usize, c: Option<usize>| {
            c.and_then(|c| table.get(r, c))
                .map(|v| v.to_string())
                .filter(|s| !s.is_empty() && s != "-99")
        };
        let iso3_cols = [col("ISO_A3"), col("ISO_A3_EH"), col("ADM0_A3")];
        let iso2_cols = [col("ISO_A2"), col("ISO_A2_EH")];
        let name_col = col("NAME");
        let pass = |names: &[&str]| -> Vec<usize> { names.iter().filter_map(|n| col(n)).collect() };
        let codes = pass(&["ISO_A2", "ISO_A2_EH", "ISO_A3", "ISO_A3_EH", "ADM0_A3"]);
        let english = pass(&["NAME", "NAME_LONG", "FORMAL_EN"]);
        let translations: Vec<usize> = table
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.name.starts_with("NAME_") && c.name != "NAME_LEN")
            .filter(|(_, c)| c.name != "NAME_LONG")
            .map(|(i, _)| i)
            .collect();
        let mut out = Countries {
            names: Vec::new(),
            iso3: Vec::new(),
            iso2: Vec::new(),
            features: extras.features,
            index: HashMap::new(),
        };
        for r in 0..table.row_count() {
            let iso3 = iso3_cols
                .iter()
                .find_map(|&c| text(r, c))
                .unwrap_or_default();
            out.names
                .push(text(r, name_col).unwrap_or_else(|| iso3.clone()));
            out.iso2.push(
                iso2_cols
                    .iter()
                    .find_map(|&c| text(r, c))
                    .unwrap_or_default(),
            );
            out.iso3.push(iso3);
        }
        // Codes, then English names, then translations: a translation of an
        // earlier outline never beats a code or primary name of a later one.
        // Within a pass the first outline wins a spelling two of them carry.
        for cols in [&codes, &english, &translations] {
            for r in 0..table.row_count() {
                for &c in cols {
                    if let Some(t) = text(r, Some(c)) {
                        out.index.entry(normalise(&t)).or_insert(r);
                    }
                }
            }
        }
        for (alias, iso3) in ALIASES {
            if let Some(i) = out.iso3.iter().position(|c| c == iso3) {
                out.index.entry(normalise(alias)).or_insert(i);
            }
        }
        out
    }
}

/// Parsed once per process, on first use.
pub fn countries() -> &'static Countries {
    static C: OnceLock<Countries> = OnceLock::new();
    C.get_or_init(Countries::load)
}

/// The first text column whose first 50 filled values (looked for in the
/// first 1000 rows) name a country at least four times in five, with at least
/// 3 values naming at least 2 different countries (an all-"No" flag is not
/// Norway). Cheap enough for the toolbar, which asks every frame. ponytail: a
/// sample, so a column whose first rows are blank or junk is missed; the
/// Country picker still offers every column.
pub fn detect_country_column(table: &DataTable) -> Option<usize> {
    let c = countries();
    (0..table.col_count()).find(|&col| {
        if !crate::data::shapes::is_text_type(&table.columns[col].data_type) {
            return false;
        }
        let sample: Vec<&str> = (0..table.row_count().min(1000))
            .filter_map(|r| match table.get(r, col) {
                Some(CellValue::String(s)) if !s.trim().is_empty() => Some(s.as_str()),
                _ => None,
            })
            .take(50)
            .collect();
        let hits: Vec<usize> = sample.iter().filter_map(|s| c.find(s)).collect();
        let distinct = hits.iter().collect::<std::collections::HashSet<_>>().len();
        sample.len() >= 3 && distinct >= 2 && hits.len() * 5 >= sample.len() * 4
    })
}

/// What the map paints: per outline the combined value and its row count,
/// and the values that matched no country with their row counts (most rows
/// first).
#[derive(Debug, Clone, PartialEq)]
pub struct CountryValues {
    pub by_country: Vec<Option<(f64, usize)>>,
    pub unmatched: Vec<(String, usize)>,
}

pub fn aggregate(
    table: &DataTable,
    rows: &[usize],
    col: usize,
    value: Option<usize>,
    agg: Aggregation,
) -> CountryValues {
    let c = countries();
    let mut vals: Vec<(Vec<f64>, usize)> = vec![(Vec::new(), 0); c.names.len()];
    let mut unmatched: HashMap<String, usize> = HashMap::new();
    for &r in rows {
        let text = match table.get(r, col) {
            Some(CellValue::Null) | None => continue,
            Some(v) => v.to_string(),
        };
        if text.trim().is_empty() {
            continue;
        }
        match c.find(&text) {
            Some(i) => {
                vals[i].1 += 1;
                if let Some(v) = value.and_then(|vc| table.get(r, vc)).and_then(cell_to_f64) {
                    vals[i].0.push(v);
                }
            }
            None => *unmatched.entry(text.trim().to_string()).or_default() += 1,
        }
    }
    let by_country = vals
        .into_iter()
        .map(|(v, n)| {
            if n == 0 {
                return None;
            }
            let agg = if value.is_none() {
                Aggregation::Count
            } else {
                agg
            };
            let x = if agg == Aggregation::Count {
                Some(n as f64)
            } else {
                agg.fold(&v)
            };
            x.map(|x| (x, n))
        })
        .collect();
    let mut unmatched: Vec<(String, usize)> = unmatched.into_iter().collect();
    unmatched.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    CountryValues {
        by_country,
        unmatched,
    }
}

#[cfg(test)]
#[path = "country_match_tests.rs"]
mod tests;
