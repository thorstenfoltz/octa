//! Fill with a series: numbers by a step, an ID's counter, dates by a unit,
//! or a repeated pattern. Pure; the dialog (`app::dialogs::fill_series`)
//! writes the strings through the paste path so column types still apply.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, Weekday};

use crate::data::CellValue;
use crate::data::number_gaps::split_counter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillKind {
    #[default]
    Numbers,
    Counter,
    Dates,
    Repeat,
}

impl FillKind {
    pub const ALL: [FillKind; 4] = [Self::Numbers, Self::Counter, Self::Dates, Self::Repeat];
    pub fn key(self) -> &'static str {
        match self {
            Self::Numbers => "fill.kind_numbers",
            Self::Counter => "fill.kind_counter",
            Self::Dates => "fill.kind_dates",
            Self::Repeat => "fill.kind_repeat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateUnit {
    #[default]
    Day,
    Week,
    Workday,
    Month,
    Year,
}

impl DateUnit {
    pub const ALL: [DateUnit; 5] = [
        Self::Day,
        Self::Week,
        Self::Workday,
        Self::Month,
        Self::Year,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Day => "fill.unit_day",
            Self::Week => "fill.unit_week",
            Self::Workday => "fill.unit_workday",
            Self::Month => "fill.unit_month",
            Self::Year => "fill.unit_year",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillSpec {
    pub kind: FillKind,
    pub start: String,
    pub step: String,
    pub unit: DateUnit,
    /// Repeat: how many leading cells form the pattern.
    pub pattern: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillError {
    BadStart,
    BadStep,
    BelowZero,
    NoPattern,
    /// The series runs past the last date or number that can be written.
    TooFar,
}

impl FillError {
    pub fn key(self) -> &'static str {
        match self {
            Self::BadStart => "fill.err_start",
            Self::BadStep => "fill.err_step",
            Self::BelowZero => "fill.err_below_zero",
            Self::NoPattern => "fill.err_pattern",
            Self::TooFar => "fill.err_too_far",
        }
    }
}

fn blank(c: &CellValue) -> bool {
    match c {
        CellValue::Null => true,
        CellValue::String(s) => s.trim().is_empty(),
        _ => false,
    }
}

fn decimals(s: &str) -> usize {
    s.split_once(['.', ',']).map_or(0, |(_, d)| d.len())
}

/// A number and how many decimals it was written with.
fn parse_num(s: &str) -> Option<(f64, usize)> {
    let t = s.trim();
    let v = t
        .parse::<f64>()
        .or_else(|_| t.replace(',', ".").parse::<f64>())
        .ok()?;
    Some((v, decimals(t)))
}

fn cell_num(c: &CellValue) -> Option<(f64, usize)> {
    match c {
        CellValue::Int(i) => Some((*i as f64, 0)),
        CellValue::Float(f) => Some((*f, decimals(&f.to_string()))),
        _ => None,
    }
}

fn fmt_num(v: f64, dec: usize) -> String {
    let s = format!("{v:.dec$}");
    if s.trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        format!("{:.dec$}", 0.0)
    } else {
        s
    }
}

/// A date or timestamp in ISO form, and whether it carried a time.
fn date_of(c: &CellValue) -> Option<(NaiveDateTime, bool)> {
    let s = match c {
        CellValue::Date(s) | CellValue::DateTime(s) | CellValue::String(s) => s.trim(),
        _ => return None,
    };
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some((d.and_hms_opt(0, 0, 0)?, false));
    }
    for f in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, f) {
            return Some((t, true));
        }
    }
    None
}

fn fmt_date(t: NaiveDateTime, with_time: bool) -> String {
    if with_time {
        t.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        t.format("%Y-%m-%d").to_string()
    }
}

/// Pre-fill the dialog from the first selected cells of a column.
pub fn guess(cells: &[CellValue]) -> FillSpec {
    let lead: Vec<&CellValue> = cells.iter().take_while(|c| !blank(c)).collect();
    // Source cells followed by empty ones: they are the pattern. A fully
    // filled selection is being overwritten, so the pattern is its first cell.
    let pattern = if lead.len() < cells.len() {
        lead.len().max(1)
    } else {
        1
    };
    let base = FillSpec {
        pattern,
        step: "1".into(),
        ..FillSpec::default()
    };
    let Some(a) = lead.first().copied() else {
        return FillSpec {
            start: "1".into(),
            ..base
        };
    };
    let b = lead.get(1).copied();
    if let Some((x, xd)) = cell_num(a) {
        let step = match b.and_then(cell_num) {
            Some((y, yd)) => fmt_num(y - x, xd.max(yd)),
            None => "1".into(),
        };
        return FillSpec {
            kind: FillKind::Numbers,
            start: a.to_string(),
            step,
            ..base
        };
    }
    if let Some((x, _)) = date_of(a) {
        let (step, unit) = match b.and_then(date_of) {
            None => (1, DateUnit::Day),
            Some((y, _)) => {
                let months = (y.year() - x.year()) * 12 + y.month() as i32 - x.month() as i32;
                let days = (y - x).num_days();
                if x.day() == y.day() && x.time() == y.time() && months != 0 {
                    if months % 12 == 0 {
                        (i64::from(months / 12), DateUnit::Year)
                    } else {
                        (i64::from(months), DateUnit::Month)
                    }
                } else if days != 0 && days % 7 == 0 {
                    (days / 7, DateUnit::Week)
                } else {
                    (if days == 0 { 1 } else { days }, DateUnit::Day)
                }
            }
        };
        return FillSpec {
            kind: FillKind::Dates,
            start: a.to_string(),
            step: step.to_string(),
            unit,
            ..base
        };
    }
    if let CellValue::String(text) = a {
        let t = text.trim();
        if let Some(c) = split_counter(t) {
            let decorated = !c.prefix.is_empty()
                || !c.suffix.is_empty()
                || c.width > c.number.to_string().len();
            if decorated {
                let step = match b {
                    Some(CellValue::String(n)) => split_counter(n.trim())
                        .filter(|d| d.prefix == c.prefix && d.suffix == c.suffix)
                        .map_or(1, |d| d.number as i64 - c.number as i64),
                    _ => 1,
                };
                return FillSpec {
                    kind: FillKind::Counter,
                    start: t.to_string(),
                    step: step.to_string(),
                    ..base
                };
            }
        }
        if let Some((x, xd)) = parse_num(t) {
            let step = match b {
                Some(CellValue::String(n)) => {
                    parse_num(n).map_or("1".into(), |(y, yd)| fmt_num(y - x, xd.max(yd)))
                }
                _ => "1".into(),
            };
            return FillSpec {
                kind: FillKind::Numbers,
                start: t.to_string(),
                step,
                ..base
            };
        }
    }
    FillSpec {
        kind: FillKind::Repeat,
        start: a.to_string(),
        ..base
    }
}

fn add_months(t: NaiveDateTime, k: i64) -> Option<NaiveDateTime> {
    let m = Months::new(u32::try_from(k.unsigned_abs()).ok()?);
    if k >= 0 {
        t.checked_add_months(m)
    } else {
        t.checked_sub_months(m)
    }
}

/// `k` working days (Monday to Friday) on from `d`, back when negative.
/// Whole weeks are jumped, since any seven days in a row hold five working
/// days; the last one to five are walked, so the answer is a working day.
/// `None` past the last date chrono holds.
fn next_workdays(mut d: NaiveDateTime, k: i64) -> Option<NaiveDateTime> {
    let dir = k.signum();
    let mut left = k.unsigned_abs();
    let weeks = left.saturating_sub(1) / 5;
    d = d.checked_add_signed(Duration::try_weeks(i64::try_from(weeks).ok()? * dir)?)?;
    left -= weeks * 5;
    while left > 0 {
        d = d.checked_add_signed(Duration::days(dir))?;
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            left -= 1;
        }
    }
    Some(d)
}

/// The `n` values for one column, as text. `own` is that column's current
/// cells in fill order (Repeat reads its pattern from them).
pub fn values(spec: &FillSpec, own: &[CellValue], n: usize) -> Result<Vec<String>, FillError> {
    match spec.kind {
        FillKind::Numbers => {
            let (start, sd) = parse_num(&spec.start).ok_or(FillError::BadStart)?;
            let (step, td) = parse_num(&spec.step).ok_or(FillError::BadStep)?;
            let dec = sd.max(td);
            Ok((0..n)
                .map(|i| fmt_num(start + step * i as f64, dec))
                .collect())
        }
        FillKind::Counter => {
            let c = split_counter(spec.start.trim()).ok_or(FillError::BadStart)?;
            let step: i64 = spec.step.trim().parse().map_err(|_| FillError::BadStep)?;
            // i128: a counter may use the whole u64 range, a step all of i64.
            (0..n)
                .map(|i| {
                    let v = i128::from(step)
                        .checked_mul(i as i128)
                        .and_then(|k| k.checked_add(i128::from(c.number)));
                    match v {
                        Some(v) if v < 0 => Err(FillError::BelowZero),
                        Some(v) => u64::try_from(v)
                            .map(|v| c.format(v))
                            .map_err(|_| FillError::TooFar),
                        None => Err(FillError::TooFar),
                    }
                })
                .collect()
        }
        FillKind::Dates => {
            let (start, with_time) =
                date_of(&CellValue::String(spec.start.clone())).ok_or(FillError::BadStart)?;
            let step: i64 = spec.step.trim().parse().map_err(|_| FillError::BadStep)?;
            let mut out = Vec::with_capacity(n);
            let mut work = start;
            for i in 0..n as i64 {
                let k = step.checked_mul(i);
                let t = match spec.unit {
                    DateUnit::Day => k
                        .and_then(Duration::try_days)
                        .and_then(|d| start.checked_add_signed(d)),
                    DateUnit::Week => k
                        .and_then(Duration::try_weeks)
                        .and_then(|d| start.checked_add_signed(d)),
                    // Step on from the previous value, a week at a time.
                    DateUnit::Workday => {
                        if i > 0 {
                            work = next_workdays(work, step).ok_or(FillError::TooFar)?;
                        }
                        Some(work)
                    }
                    DateUnit::Month => k.and_then(|k| add_months(start, k)),
                    DateUnit::Year => k
                        .and_then(|k| k.checked_mul(12))
                        .and_then(|k| add_months(start, k)),
                }
                .ok_or(FillError::TooFar)?;
                out.push(fmt_date(t, with_time));
            }
            Ok(out)
        }
        FillKind::Repeat => {
            let p = spec.pattern.min(own.len());
            if p == 0 {
                return Err(FillError::NoPattern);
            }
            Ok((0..n)
                .map(|i| match &own[i % p] {
                    CellValue::Null => String::new(),
                    c => c.to_string(),
                })
                .collect())
        }
    }
}

/// The cells to fill, per column, in display order. A selected column means
/// every row the filter shows; otherwise the selected cells, minus rows the
/// filter hides. Columns with fewer than two cells are left out.
pub fn targets(
    selected_cols: &HashSet<usize>,
    selected_cells: &HashSet<(usize, usize)>,
    filtered_rows: &[usize],
) -> Vec<(usize, Vec<usize>)> {
    if !selected_cols.is_empty() {
        let mut cols: Vec<usize> = selected_cols.iter().copied().collect();
        cols.sort_unstable();
        return cols
            .into_iter()
            .map(|c| (c, filtered_rows.to_vec()))
            .collect();
    }
    let pos: HashMap<usize, usize> = filtered_rows
        .iter()
        .enumerate()
        .map(|(i, &r)| (r, i))
        .collect();
    let mut by_col: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(r, c) in selected_cells {
        if pos.contains_key(&r) {
            by_col.entry(c).or_default().push(r);
        }
    }
    by_col
        .into_iter()
        .map(|(c, mut rows)| {
            rows.sort_by_key(|r| pos[r]);
            (c, rows)
        })
        .filter(|(_, rows)| rows.len() >= 2)
        .collect()
}

#[cfg(test)]
#[path = "fill_series_tests.rs"]
mod tests;
