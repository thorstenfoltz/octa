//! Convert currency at the row's date with BIS rates (units per US dollar).
//! Weekend or holiday: the last daily rate before. A currency without a daily
//! rate then: that month's average. After the newest rate: the newest.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use chrono::{Datelike, NaiveDate};

use crate::data::decimal::{Exact, UNROUNDED_DECIMALS};
use crate::data::{CellValue, DataTable};
use crate::reference::merge::{self, SeriesSet};
use crate::reference::sources::bis_rates;
use crate::reference::store;

use super::{Computed, NewColumn, Pick, Rounding, amount, cell_text, date_of, minor_units};

#[derive(Debug, Clone, Default)]
pub struct Rates {
    daily: HashMap<String, BTreeMap<NaiveDate, Exact>>,
    monthly: HashMap<String, BTreeMap<NaiveDate, Exact>>,
    newest: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Used {
    Day(NaiveDate),
    Month(NaiveDate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Miss {
    Unknown(String),
    NoCurrency,
    Before(NaiveDate),
    NoDate,
    NoAmount,
    TooLarge,
}

impl Miss {
    /// Stable English, like the other value cells Octa writes.
    pub fn text(&self) -> String {
        match self {
            Miss::Unknown(c) => format!("unknown currency {c}"),
            Miss::NoCurrency => "no currency".into(),
            Miss::Before(d) => format!("no rate before {d}"),
            Miss::NoDate => "not a date".into(),
            Miss::NoAmount => "no amount".into(),
            Miss::TooLarge => "too large to convert exactly".into(),
        }
    }
}

fn day_key(k: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(k, "%Y-%m-%d").ok()
}

fn month_key(k: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{k}-01"), "%Y-%m-%d").ok()
}

fn dated(
    set: &SeriesSet,
    key: fn(&str) -> Option<NaiveDate>,
) -> HashMap<String, BTreeMap<NaiveDate, Exact>> {
    set.iter()
        .map(|(code, s)| {
            (
                code.clone(),
                s.iter().filter_map(|(k, v)| Some((key(k)?, *v))).collect(),
            )
        })
        .collect()
}

impl Rates {
    pub fn from_sets(daily: &SeriesSet, monthly: &SeriesSet) -> Rates {
        let daily = dated(daily, day_key);
        let monthly = dated(monthly, month_key);
        let newest = daily
            .values()
            .filter_map(|s| s.keys().next_back())
            .max()
            .copied();
        Rates {
            daily,
            monthly,
            newest,
        }
    }

    pub fn load(dir: &Path) -> anyhow::Result<Rates> {
        let read = |name: &str, key: &str| -> anyhow::Result<SeriesSet> {
            let p = dir.join(name);
            if p.exists() {
                merge::table_to_series(&store::read(&p)?, key)
            } else {
                Ok(SeriesSet::new())
            }
        };
        let rates = Rates::from_sets(
            &read(bis_rates::DAILY, "date")?,
            &read(bis_rates::MONTHLY, "month")?,
        );
        anyhow::ensure!(
            !rates.daily.is_empty() || !rates.monthly.is_empty(),
            "no exchange rates downloaded yet"
        );
        Ok(rates)
    }

    pub fn codes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .daily
            .keys()
            .chain(self.monthly.keys())
            .cloned()
            .chain(std::iter::once("USD".to_string()))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn knows(&self, code: &str) -> bool {
        code == "USD" || self.daily.contains_key(code) || self.monthly.contains_key(code)
    }

    /// Units of `code` per US dollar on `on` (`None` = the newest rate).
    pub fn rate(&self, code: &str, on: Option<NaiveDate>) -> Result<(Exact, Used), Miss> {
        if code == "USD" {
            let day = on.or(self.newest).unwrap_or_default();
            return Ok((Exact::from_raw(1, 0), Used::Day(day)));
        }
        let (daily, monthly) = (self.daily.get(code), self.monthly.get(code));
        if daily.is_none() && monthly.is_none() {
            return Err(Miss::Unknown(code.to_string()));
        }
        let day = daily.and_then(|d| match on {
            Some(on) => d.range(..=on).next_back(),
            None => d.iter().next_back(),
        });
        if let Some((d, v)) = day {
            return Ok((*v, Used::Day(*d)));
        }
        let month = monthly.and_then(|m| match on {
            Some(on) => m.range(..=on.with_day(1).unwrap_or(on)).next_back(),
            None => m.iter().next_back(),
        });
        if let Some((m, v)) = month {
            return Ok((*v, Used::Month(*m)));
        }
        let first = [
            daily.and_then(|d| d.keys().next()),
            monthly.and_then(|m| m.keys().next()),
        ]
        .into_iter()
        .flatten()
        .min()
        .copied()
        .unwrap_or_default();
        Err(Miss::Before(first))
    }
}

/// ISO code or a common symbol, if BIS has rates for it.
pub fn code_of(text: &str, rates: &Rates) -> Option<String> {
    let t = text.trim();
    let code = match t {
        "€" => "EUR",
        "$" | "US$" => "USD",
        "£" => "GBP",
        "¥" => "JPY",
        "₹" => "INR",
        "₩" => "KRW",
        "₺" => "TRY",
        "zł" => "PLN",
        "Fr." | "SFr." => "CHF",
        other => other,
    }
    .to_ascii_uppercase();
    if rates.knows(&code) {
        return Some(code);
    }
    // A currency written out, `Euro` or `US-Dollar`.
    super::names::currency_code(t).filter(|c| rates.knows(c))
}

/// How the currency column names the target currency (the user's choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    Code,
    Symbol,
}

impl Mark {
    pub fn parse(s: &str) -> Option<Mark> {
        match s.trim().to_ascii_lowercase().as_str() {
            "code" => Some(Mark::Code),
            "symbol" => Some(Mark::Symbol),
            _ => None,
        }
    }

    pub fn shown(self, code: &str) -> String {
        match self {
            Mark::Symbol => symbol(code).unwrap_or(code).to_string(),
            Mark::Code => code.to_string(),
        }
    }
}

/// Symbols that belong to exactly one currency and that Roboto, the table
/// font, can draw (checked 2026-11-13; it has no glyph for the baht's ฿).
/// `$`, `¥`, `kr`, `Fr.` and `₱` are shared, so those currencies keep their code.
pub fn symbol(code: &str) -> Option<&'static str> {
    Some(match code {
        "EUR" => "€",
        "GBP" => "£",
        "INR" => "₹",
        "ILS" => "₪",
        "KRW" => "₩",
        "NGN" => "₦",
        "RUB" => "₽",
        "TRY" => "₺",
        "UAH" => "₴",
        "VND" => "₫",
        "KZT" => "₸",
        "GEL" => "₾",
        "AZN" => "₼",
        "PYG" => "₲",
        "CRC" => "₡",
        "GHS" => "₵",
        "LAK" => "₭",
        "MNT" => "₮",
        "PLN" => "zł",
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurrencySpec {
    pub amount: usize,
    pub from: Pick,
    pub to: String,
    /// `None`: the newest rate for every row.
    pub date: Option<usize>,
    pub rounding: Rounding,
    pub mark: Mark,
}

fn label(a: Used, b: Used) -> String {
    match (a, b) {
        (Used::Month(m), _) | (_, Used::Month(m)) => {
            format!("{} (monthly average)", m.format("%Y-%m"))
        }
        (Used::Day(x), Used::Day(y)) => x.min(y).to_string(),
    }
}

fn convert(
    rates: &Rates,
    spec: &CurrencySpec,
    to: &str,
    cell: &dyn Fn(usize) -> CellValue,
) -> Result<(Exact, String), Miss> {
    let amt = amount(&cell(spec.amount)).ok_or(Miss::NoAmount)?;
    let raw_from = match &spec.from {
        Pick::Fixed(f) => Some(f.clone()),
        Pick::Column(c) => cell_text(&cell(*c)),
    }
    .ok_or(Miss::NoCurrency)?;
    let from =
        code_of(&raw_from, rates).ok_or_else(|| Miss::Unknown(raw_from.trim().to_string()))?;
    let on = match spec.date {
        None => None,
        Some(c) => Some(date_of(&cell(c)).ok_or(Miss::NoDate)?),
    };
    if from == to {
        return Ok((amt, "same currency".into()));
    }
    let (from_rate, from_used) = rates.rate(&from, on)?;
    let (to_rate, to_used) = rates.rate(to, on)?;
    let value = amt
        .checked_mul(to_rate)
        .and_then(|x| x.div_up_to(from_rate, UNROUNDED_DECIMALS))
        .ok_or(Miss::TooLarge)?;
    Ok((
        spec.rounding.apply(value, minor_units(to)),
        label(from_used, to_used),
    ))
}

pub fn currency_columns(
    table: &DataTable,
    rates: &Rates,
    spec: &CurrencySpec,
) -> anyhow::Result<Computed> {
    let to = code_of(&spec.to, rates)
        .ok_or_else(|| anyhow::anyhow!("unknown target currency {}", spec.to.trim()))?;
    if let Pick::Fixed(f) = &spec.from {
        anyhow::ensure!(code_of(f, rates).is_some(), "unknown currency {f}");
    }
    let n = table.row_count();
    let (mut values, mut marks, mut notes) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    let shown = CellValue::String(spec.mark.shown(&to));
    let (mut found, mut missed) = (0, 0);
    for r in 0..n {
        let cell = |c: usize| table.get(r, c).cloned().unwrap_or(CellValue::Null);
        match convert(rates, spec, &to, &cell) {
            Ok((v, note)) => {
                found += 1;
                values.push(v.to_cell());
                marks.push(shown.clone());
                notes.push(CellValue::String(note));
            }
            Err(Miss::NoAmount) => {
                values.push(CellValue::Null);
                marks.push(CellValue::Null);
                notes.push(CellValue::Null);
            }
            Err(m) => {
                missed += 1;
                values.push(CellValue::Null);
                marks.push(CellValue::Null);
                notes.push(CellValue::String(m.text()));
            }
        }
    }
    let base = table.columns[spec.amount].name.clone();
    Ok(Computed {
        after: spec.amount,
        columns: vec![
            NewColumn::numbers(format!("{base}_{to}"), values),
            NewColumn::text(format!("{base}_{to}_currency"), marks),
            NewColumn::text(format!("{base}_{to}_rate_date"), notes),
        ],
        found,
        missed,
    })
}

#[cfg(test)]
#[path = "currency_tests.rs"]
mod tests;
