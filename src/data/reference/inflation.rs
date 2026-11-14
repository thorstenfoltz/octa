//! In today's money: an amount from one month in the prices of another,
//! by BIS's monthly consumer price index (2010 = 100).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use chrono::{Datelike, NaiveDate};

use crate::data::decimal::Exact;
use crate::data::decimal::UNROUNDED_DECIMALS;
use crate::data::{CellValue, DataTable};
use crate::reference::merge::{self, SeriesSet};
use crate::reference::sources::bis_inflation;
use crate::reference::store;

use super::{Computed, NewColumn, Pick, Rounding, amount, cell_text, country_code, date_of};

#[derive(Debug, Clone, Default)]
pub struct Indices {
    by_area: HashMap<String, BTreeMap<NaiveDate, Exact>>,
}

fn month(k: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{k}-01"), "%Y-%m-%d").ok()
}

fn ym(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

impl Indices {
    pub fn from_set(set: &SeriesSet) -> Indices {
        Indices {
            by_area: set
                .iter()
                .map(|(a, s)| {
                    (
                        a.clone(),
                        s.iter()
                            .filter_map(|(k, v)| Some((month(k)?, *v)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    pub fn load(dir: &Path) -> anyhow::Result<Indices> {
        let p = dir.join(bis_inflation::FILE);
        anyhow::ensure!(p.exists(), "no price index downloaded yet");
        Ok(Indices::from_set(&merge::table_to_series(
            &store::read(&p)?,
            "month",
        )?))
    }

    pub fn areas(&self) -> Vec<String> {
        let mut v: Vec<String> = self.by_area.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn newest(&self, area: &str) -> Option<NaiveDate> {
        self.by_area.get(area)?.keys().next_back().copied()
    }

    /// The index at or before `on`, with the month it is for.
    fn at(&self, area: &str, on: NaiveDate) -> Option<(NaiveDate, Exact)> {
        self.by_area
            .get(area)?
            .range(..=on)
            .next_back()
            .map(|(d, v)| (*d, *v))
    }

    fn first(&self, area: &str) -> Option<NaiveDate> {
        self.by_area.get(area)?.keys().next().copied()
    }

    /// Every month any economy has, newest first (the target month list).
    pub fn months(&self) -> Vec<NaiveDate> {
        let set: std::collections::BTreeSet<NaiveDate> = self
            .by_area
            .values()
            .flat_map(|s| s.keys().copied())
            .collect();
        set.into_iter().rev().collect()
    }
}

/// A fixed date as the user gives it: a date, or a month as `YYYY-MM`.
pub fn fixed_date(text: &str) -> Option<NaiveDate> {
    let t = text.trim();
    month(t).or_else(|| crate::data::time_calc::parse_date(t))
}

pub fn area_of(text: &str) -> Option<String> {
    let t = text.trim().to_lowercase();
    if ["euro area", "eurozone", "euro zone", "xm", "ea"].contains(&t.as_str()) {
        return Some("XM".into());
    }
    country_code(text)
}

#[derive(Debug, Clone, PartialEq)]
pub struct InflationSpec {
    pub amount: usize,
    /// A column of dates, or one month or date for every row.
    pub date: Pick,
    pub country: Pick,
    /// First of the target month; `None` = [`default_target`].
    pub target: Option<NaiveDate>,
    pub rounding: Rounding,
}

fn area_in_row(spec: &InflationSpec, cell: &dyn Fn(usize) -> CellValue) -> Option<String> {
    match &spec.country {
        Pick::Fixed(f) => area_of(f),
        Pick::Column(c) => cell_text(&cell(*c)).and_then(|t| area_of(&t)),
    }
}

/// The newest month every economy in the table has.
pub fn default_target(table: &DataTable, idx: &Indices, spec: &InflationSpec) -> Option<NaiveDate> {
    (0..table.row_count())
        .filter_map(|r| {
            let cell = |c: usize| table.get(r, c).cloned().unwrap_or(CellValue::Null);
            idx.newest(&area_in_row(spec, &cell)?)
        })
        .min()
}

pub fn inflation_columns(
    table: &DataTable,
    idx: &Indices,
    spec: &InflationSpec,
) -> anyhow::Result<Computed> {
    let target = spec
        .target
        .or_else(|| default_target(table, idx, spec))
        .ok_or_else(|| anyhow::anyhow!("no row has a country with a price index"))?;
    let decimals = match spec.rounding {
        Rounding::Usual => Rounding::Decimals(2),
        other => other,
    };
    let fixed = match &spec.date {
        Pick::Column(_) => None,
        Pick::Fixed(f) => Some(fixed_date(f).ok_or_else(|| {
            anyhow::anyhow!("{f} is neither a column nor a date (a month is YYYY-MM)")
        })?),
    };
    let n = table.row_count();
    let (mut values, mut notes) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut found, mut missed) = (0, 0);
    for r in 0..n {
        let cell = |c: usize| table.get(r, c).cloned().unwrap_or(CellValue::Null);
        let Some(amt) = amount(&cell(spec.amount)) else {
            values.push(CellValue::Null);
            notes.push(CellValue::Null);
            continue;
        };
        let outcome = (|| -> Result<(Exact, String), String> {
            let area = area_in_row(spec, &cell).ok_or_else(|| {
                let raw = match &spec.country {
                    Pick::Fixed(f) => f.clone(),
                    Pick::Column(c) => cell_text(&cell(*c)).unwrap_or_default(),
                };
                format!("no index for {raw}")
            })?;
            let on = match (&spec.date, fixed) {
                (Pick::Column(c), _) => date_of(&cell(*c)).ok_or("not a date")?,
                (Pick::Fixed(_), on) => on.ok_or("not a date")?,
            };
            let on = on.with_day(1).unwrap_or(on);
            let (from_month, from) = idx.at(&area, on).ok_or_else(|| {
                format!(
                    "no index before {}",
                    idx.first(&area).map(ym).unwrap_or_default()
                )
            })?;
            let goal = target.min(idx.newest(&area).unwrap_or(target));
            let (to_month, to) = idx.at(&area, goal).ok_or("no index for the target month")?;
            let v = amt
                .checked_mul(to)
                .and_then(|x| x.div_up_to(from, UNROUNDED_DECIMALS))
                .ok_or("too large to convert exactly")?;
            Ok((
                decimals.apply(v, 2),
                format!("{} to {}", ym(from_month), ym(to_month)),
            ))
        })();
        match outcome {
            Ok((v, note)) => {
                found += 1;
                values.push(v.to_cell());
                notes.push(CellValue::String(note));
            }
            Err(why) => {
                missed += 1;
                values.push(CellValue::Null);
                notes.push(CellValue::String(why));
            }
        }
    }
    let base = table.columns[spec.amount].name.clone();
    Ok(Computed {
        after: spec.amount,
        columns: vec![
            NewColumn::numbers(format!("{base}_in_{}", ym(target)), values),
            NewColumn::text(format!("{base}_index_month"), notes),
        ],
        found,
        missed,
    })
}

#[cfg(test)]
#[path = "inflation_tests.rs"]
mod tests;
