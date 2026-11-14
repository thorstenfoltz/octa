//! The lookups behind Data > Reference data..., `octa --reference` and the MCP
//! tool `reference_data`: a table and loaded reference data in, new columns
//! out. Nothing here downloads; `crate::reference` does that.

pub mod currency;
pub mod inflation;
pub mod names;
pub mod places;

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::data::decimal::Exact;
use crate::data::{CellValue, DataTable};

/// How a result is rounded (the user's choice, 2026-11-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rounding {
    /// The target currency's usual decimals (currency only).
    Usual,
    Decimals(u32),
    /// At most [`crate::data::decimal::UNROUNDED_DECIMALS`], trailing zeros dropped.
    None,
}

impl Rounding {
    pub fn parse(s: &str) -> Option<Rounding> {
        match s.trim().to_ascii_lowercase().as_str() {
            "usual" => Some(Rounding::Usual),
            "none" => Some(Rounding::None),
            n => n.parse().ok().map(Rounding::Decimals),
        }
    }

    pub fn apply(self, v: Exact, usual: u32) -> Exact {
        match self {
            Rounding::Usual => v.round_to(usual),
            Rounding::Decimals(n) => v.round_to(n),
            Rounding::None => v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Several {
    #[default]
    All,
    First,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Column(usize),
    Fixed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewColumn {
    pub name: String,
    pub data_type: String,
    pub values: Vec<CellValue>,
}

impl NewColumn {
    pub fn numbers(name: String, values: Vec<CellValue>) -> NewColumn {
        let data_type = crate::data::column_type_from_cells(values.iter());
        NewColumn {
            name,
            data_type,
            values,
        }
    }

    pub fn text(name: String, values: Vec<CellValue>) -> NewColumn {
        NewColumn {
            name,
            data_type: "Utf8".into(),
            values,
        }
    }
}

/// New columns, to go right after column `after`.
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub after: usize,
    pub columns: Vec<NewColumn>,
    /// Rows that got a value.
    pub found: usize,
    /// Rows with a key or amount that got none.
    pub missed: usize,
}

/// Insert through the undoable writes; the caller folds them into one step
/// with `coalesce_undo_since`. Returns the names used.
pub fn insert_columns(table: &mut DataTable, c: &Computed) -> Vec<String> {
    let mut names = Vec::with_capacity(c.columns.len());
    for (i, col) in c.columns.iter().enumerate() {
        let at = c.after + 1 + i;
        let name =
            crate::data::unique_name(&col.name, |n| table.columns.iter().any(|x| x.name == n));
        table.insert_column(at, name.clone(), col.data_type.clone());
        for (r, v) in col.values.iter().enumerate() {
            if !matches!(v, CellValue::Null) {
                table.set(r, at, v.clone());
            }
        }
        names.push(name);
    }
    names
}

const ZERO_DECIMALS: [&str; 17] = [
    "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "UYI", "VND",
    "VUV", "XAF", "XOF", "XPF",
];
const THREE_DECIMALS: [&str; 7] = ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];

/// ISO 4217 minor units for the currencies BIS covers; 2 for the rest.
pub fn minor_units(code: &str) -> u32 {
    if ZERO_DECIMALS.contains(&code) {
        0
    } else if THREE_DECIMALS.contains(&code) {
        3
    } else {
        2
    }
}

pub fn cell_text(c: &CellValue) -> Option<String> {
    let s = match c {
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => f.to_string(),
        CellValue::String(s) | CellValue::Date(s) | CellValue::DateTime(s) => s.trim().to_string(),
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

pub fn amount(c: &CellValue) -> Option<Exact> {
    Exact::of_cell(c).or_else(|| match c {
        CellValue::String(s) => {
            crate::data::num_parse::parse_number_relaxed(s.trim()).and_then(Exact::of_f64)
        }
        _ => None,
    })
}

pub fn date_of(c: &CellValue) -> Option<NaiveDate> {
    match c {
        CellValue::Date(s) | CellValue::DateTime(s) | CellValue::String(s) => {
            crate::data::time_calc::parse_date(s.trim())
        }
        _ => None,
    }
}

/// ISO alpha-2 for a code or a country name in any language the country
/// matcher knows. Eurostat's `EL` and `UK` become `GR` and `GB`.
pub fn country_code(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() == 2 && t.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some(crate::reference::sources::eurostat::iso2(
            &t.to_ascii_uppercase(),
        ));
    }
    let all = crate::data::country_match::countries();
    let i = all.find(t)?;
    let iso2 = &all.iso2[i];
    if iso2.len() == 2 && iso2.chars().all(|c| c.is_ascii_uppercase()) {
        return Some(iso2.clone());
    }
    // Natural Earth leaves ISO_A2 empty for a few outlines (France, Norway).
    match all.iso3[i].as_str() {
        "FRA" => Some("FR".into()),
        "NOR" => Some("NO".into()),
        _ => None,
    }
}

/// Distinct non-empty values, sorted: all joined with `, ` or the first.
pub fn join(values: impl IntoIterator<Item = String>, several: Several) -> Option<String> {
    let set: BTreeSet<String> = values.into_iter().filter(|v| !v.is_empty()).collect();
    match several {
        Several::All => (!set.is_empty()).then(|| set.into_iter().collect::<Vec<_>>().join(", ")),
        Several::First => set.into_iter().next(),
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
