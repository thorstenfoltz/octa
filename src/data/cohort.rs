//! Cohort table: who first appeared when, and how many of them (or how much
//! of a number column) came back in each later period. Shared by the dialog,
//! `--cohort`, the MCP tool `cohort_table` and the database path
//! (`db::pushdown::cohort` fills [`Cells`] and calls [`wide`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::data::timeseries::Interval;
use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    Day,
    Week,
    #[default]
    Month,
    Quarter,
    Year,
}

impl Period {
    pub const ALL: [Period; 5] = [
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Year => "year",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.id().eq_ignore_ascii_case(s.trim()))
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Day => "cohort.period_day",
            Self::Week => "cohort.period_week",
            Self::Month => "cohort.period_month",
            Self::Quarter => "cohort.period_quarter",
            Self::Year => "cohort.period_year",
        }
    }
    pub fn interval(self) -> Interval {
        match self {
            Self::Day => Interval::Day,
            Self::Week => Interval::Week,
            Self::Month => Interval::Month,
            Self::Quarter => Interval::Quarter,
            Self::Year => Interval::Year,
        }
    }
    /// The first day of the period holding `d`. Weeks start on Monday.
    pub fn start(self, d: NaiveDate) -> NaiveDate {
        let first_of = |m: u32| NaiveDate::from_ymd_opt(d.year(), m, 1).unwrap_or(d);
        match self {
            Self::Day => d,
            Self::Week => d - Duration::days(i64::from(d.weekday().num_days_from_monday())),
            Self::Month => first_of(d.month()),
            Self::Quarter => first_of((d.month() - 1) / 3 * 3 + 1),
            Self::Year => first_of(1),
        }
    }
    /// Whole periods from `a` to `b`, both period starts.
    pub fn between(self, a: NaiveDate, b: NaiveDate) -> i64 {
        let months =
            i64::from(b.year() - a.year()) * 12 + i64::from(b.month()) - i64::from(a.month());
        match self {
            Self::Day => (b - a).num_days(),
            Self::Week => (b - a).num_days() / 7,
            Self::Month => months,
            Self::Quarter => months / 3,
            Self::Year => i64::from(b.year() - a.year()),
        }
    }
    /// The start `k` periods after `c`.
    pub fn offset(self, c: NaiveDate, k: i64) -> NaiveDate {
        let months = |m: i64| {
            u32::try_from(m)
                .ok()
                .and_then(|m| c.checked_add_months(Months::new(m)))
                .unwrap_or(c)
        };
        match self {
            Self::Day => c + Duration::days(k),
            Self::Week => c + Duration::weeks(k),
            Self::Month => months(k),
            Self::Quarter => months(k * 3),
            Self::Year => months(k * 12),
        }
    }
    pub fn label(self, d: NaiveDate) -> String {
        match self {
            Self::Day => d.format("%Y-%m-%d").to_string(),
            Self::Week => {
                let w = d.iso_week();
                format!("{}-W{:02}", w.year(), w.week())
            }
            Self::Month => d.format("%Y-%m").to_string(),
            Self::Quarter => format!("{}-Q{}", d.year(), (d.month() - 1) / 3 + 1),
            Self::Year => d.year().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Show {
    #[default]
    Share,
    Count,
    Sum,
}

impl Show {
    pub const ALL: [Show; 3] = [Self::Share, Self::Count, Self::Sum];
    pub fn id(self) -> &'static str {
        match self {
            Self::Share => "share",
            Self::Count => "count",
            Self::Sum => "sum",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|x| x.id().eq_ignore_ascii_case(s.trim()))
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Share => "cohort.show_share",
            Self::Count => "cohort.show_count",
            Self::Sum => "cohort.show_sum",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CohortSpec {
    pub who: usize,
    pub when: usize,
    pub period: Period,
    pub show: Show,
    pub sum_col: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CohortError {
    NoRows,
    NoSumColumn,
}

impl CohortError {
    pub fn key(self) -> &'static str {
        match self {
            Self::NoRows => "cohort.err_no_rows",
            Self::NoSumColumn => "cohort.err_no_sum",
        }
    }
}

/// What the table is built from: cohort sizes and one value per
/// (cohort start, activity period start).
#[derive(Debug, Clone, PartialEq)]
pub struct Cells {
    pub period: Period,
    pub show: Show,
    pub sizes: BTreeMap<NaiveDate, i64>,
    pub values: HashMap<(NaiveDate, NaiveDate), f64>,
    /// The latest activity period in the data: later cells are empty.
    pub last: NaiveDate,
    pub skipped: usize,
}

/// An ISO date or timestamp, from a date cell or text.
pub fn date_of(cell: &CellValue) -> Option<NaiveDate> {
    let s = match cell {
        CellValue::Date(s) | CellValue::DateTime(s) | CellValue::String(s) => s.trim(),
        _ => return None,
    };
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn number(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn collect(table: &DataTable, spec: &CohortSpec) -> Result<Cells, CohortError> {
    if spec.show == Show::Sum && spec.sum_col.is_none() {
        return Err(CohortError::NoSumColumn);
    }
    let mut first: HashMap<String, NaiveDate> = HashMap::new();
    let mut rows: Vec<(String, NaiveDate, f64)> = Vec::new();
    let mut skipped = 0;
    for r in 0..table.row_count() {
        let who = table
            .get(r, spec.who)
            .map(ToString::to_string)
            .unwrap_or_default();
        let when = table.get(r, spec.when).and_then(date_of);
        let Some(when) = when.filter(|_| !who.trim().is_empty()) else {
            skipped += 1;
            continue;
        };
        let v = spec
            .sum_col
            .and_then(|c| table.get(r, c))
            .and_then(number)
            .unwrap_or(0.0);
        let b = spec.period.start(when);
        first
            .entry(who.clone())
            .and_modify(|f| *f = (*f).min(b))
            .or_insert(b);
        rows.push((who, b, v));
    }
    let last = rows.iter().map(|r| r.1).max().ok_or(CohortError::NoRows)?;
    let mut sizes: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for f in first.values() {
        *sizes.entry(*f).or_default() += 1;
    }
    let mut values: HashMap<(NaiveDate, NaiveDate), f64> = HashMap::new();
    if spec.show == Show::Sum {
        // By the digits each value shows, so cents add up to cents.
        let mut sums: HashMap<(NaiveDate, NaiveDate), crate::data::decimal::Sum> = HashMap::new();
        for (w, b, v) in &rows {
            sums.entry((first[w], *b)).or_default().add_f64(*v);
        }
        values.extend(sums.into_iter().map(|(k, s)| (k, s.value())));
    } else {
        let mut seen: HashSet<(&str, NaiveDate)> = HashSet::new();
        for (w, b, _) in &rows {
            if seen.insert((w.as_str(), *b)) {
                *values.entry((first[w], *b)).or_default() += 1.0;
            }
        }
    }
    Ok(Cells {
        period: spec.period,
        show: spec.show,
        sizes,
        values,
        last,
        skipped,
    })
}

/// One row per cohort: `cohort`, `size`, `<period>_0` ... Cells after the
/// data's last period stay empty.
pub fn wide(cells: &Cells) -> DataTable {
    let p = cells.period;
    let max_k = cells
        .sizes
        .keys()
        .next()
        .map_or(0, |c0| p.between(*c0, cells.last))
        .max(0);
    let value_type = if cells.show == Show::Count {
        "Int64"
    } else {
        "Float64"
    };
    let mut columns = vec![
        ColumnInfo {
            name: "cohort".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "size".into(),
            data_type: "Int64".into(),
        },
    ];
    columns.extend((0..=max_k).map(|k| ColumnInfo {
        name: format!("{}_{k}", p.id()),
        data_type: value_type.into(),
    }));
    let rows = cells
        .sizes
        .iter()
        .map(|(c, size)| {
            let horizon = p.between(*c, cells.last);
            let mut row = vec![CellValue::String(p.label(*c)), CellValue::Int(*size)];
            row.extend((0..=max_k).map(|k| {
                if k > horizon {
                    return CellValue::Null;
                }
                let v = cells
                    .values
                    .get(&(*c, p.offset(*c, k)))
                    .copied()
                    .unwrap_or(0.0);
                match cells.show {
                    Show::Share => CellValue::Float((v / *size as f64 * 1000.0).round() / 10.0),
                    Show::Count => CellValue::Int(v as i64),
                    Show::Sum => CellValue::Float(v),
                }
            }));
            row
        })
        .collect();
    DataTable {
        columns,
        rows,
        ..DataTable::empty()
    }
}

/// The wide table as (cohort, period number, value), one row per filled
/// cell: the shape the Heatmap chart reads.
pub fn long_from_wide(wide: &DataTable) -> DataTable {
    let mut rows = Vec::new();
    for r in 0..wide.row_count() {
        let cohort = wide.get(r, 0).cloned().unwrap_or(CellValue::Null);
        for (k, c) in (2..wide.col_count()).enumerate() {
            let v = match wide.get(r, c) {
                Some(CellValue::Int(i)) => *i as f64,
                Some(CellValue::Float(f)) => *f,
                _ => continue,
            };
            rows.push(vec![
                cohort.clone(),
                CellValue::Int(k as i64),
                CellValue::Float(v),
            ]);
        }
    }
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "cohort".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "period".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "value".into(),
                data_type: "Float64".into(),
            },
        ],
        rows,
        ..DataTable::empty()
    }
}

#[cfg(test)]
#[path = "cohort_tests.rs"]
mod tests;
