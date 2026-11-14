//! Funnel: how many people get through a series of steps in order, and where
//! they drop out. Shared by the dialog, `--funnel`, the MCP tool `funnel` and
//! the database path (`db::pushdown::funnel` fills [`Counts`]).
//!
//! A person reaches step k when they have that event at or after the moment
//! they reached step k-1. Without a When column the row order is the time.

use std::collections::HashMap;

use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FunnelSpec {
    pub who: usize,
    pub event: usize,
    /// `None`: the rows count in table order.
    pub when: Option<usize>,
    /// Event values, in funnel order. Any number from two up.
    pub steps: Vec<String>,
    /// A later step counts only this many days or fewer after the first.
    pub within_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunnelError {
    TooFewSteps,
    RepeatedStep,
    WithinNeedsWhen,
}

impl FunnelError {
    pub fn key(self) -> &'static str {
        match self {
            Self::TooFewSteps => "funnel.err_steps",
            Self::RepeatedStep => "funnel.err_repeat",
            Self::WithinNeedsWhen => "funnel.err_within",
        }
    }
}

/// People per step, in step order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Counts {
    pub steps: Vec<String>,
    pub users: Vec<i64>,
    /// Rows without a who, or without a readable time when one is asked for.
    pub skipped: usize,
}

/// What every surface refuses before any work. A repeated step is refused
/// because "at or after" would let one row satisfy both.
pub fn check(
    steps: &[String],
    has_when: bool,
    within_days: Option<u32>,
) -> Result<(), FunnelError> {
    if steps.len() < 2 {
        return Err(FunnelError::TooFewSteps);
    }
    if (1..steps.len()).any(|i| steps[..i].contains(&steps[i])) {
        return Err(FunnelError::RepeatedStep);
    }
    if within_days.is_some() && !has_when {
        return Err(FunnelError::WithinNeedsWhen);
    }
    Ok(())
}

pub fn collect(table: &DataTable, spec: &FunnelSpec) -> Result<Counts, FunnelError> {
    check(&spec.steps, spec.when.is_some(), spec.within_days)?;
    let step_of: HashMap<&str, usize> = spec
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    // Per person: (time, step) of every row that is one of the steps.
    let mut events: HashMap<String, Vec<(f64, usize)>> = HashMap::new();
    let mut skipped = 0;
    for r in 0..table.row_count() {
        let who = table
            .get(r, spec.who)
            .map(ToString::to_string)
            .unwrap_or_default();
        let t = match spec.when {
            Some(c) => table.get(r, c).and_then(super::calculated::seconds),
            None => Some(r as f64),
        };
        let Some(t) = t.filter(|_| !who.trim().is_empty()) else {
            skipped += 1;
            continue;
        };
        let event = table
            .get(r, spec.event)
            .map(ToString::to_string)
            .unwrap_or_default();
        if let Some(&k) = step_of.get(event.as_str()) {
            events.entry(who).or_default().push((t, k));
        }
    }
    let window = spec.within_days.map(|d| f64::from(d) * 86_400.0);
    let mut users = vec![0i64; spec.steps.len()];
    for seen in events.values() {
        let (mut reached, mut first) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (k, n) in users.iter_mut().enumerate() {
            let limit = window
                .filter(|_| k > 0)
                .map_or(f64::INFINITY, |w| first + w);
            let at = seen
                .iter()
                .filter(|(t, s)| *s == k && *t >= reached && *t <= limit)
                .map(|(t, _)| *t)
                .fold(f64::INFINITY, f64::min);
            if at.is_infinite() {
                break;
            }
            if k == 0 {
                first = at;
            }
            reached = at;
            *n += 1;
        }
    }
    Ok(Counts {
        steps: spec.steps.clone(),
        users,
        skipped,
    })
}

/// Column ids of the result, shared with the header tooltips.
pub const COLUMNS: [&str; 5] = [
    "step",
    "users",
    "share_of_first",
    "share_of_previous",
    "dropped",
];

/// One row per step: `step`, `users`, `share_of_first`, `share_of_previous`
/// (percent, one decimal; empty when there is nobody to take a share of) and
/// `dropped` (people lost since the step before).
pub fn table(counts: &Counts) -> DataTable {
    let share = |n: i64, of: i64| {
        if of == 0 {
            CellValue::Null
        } else {
            CellValue::Float((n as f64 / of as f64 * 1000.0).round() / 10.0)
        }
    };
    let first = counts.users.first().copied().unwrap_or(0);
    let rows = counts
        .steps
        .iter()
        .zip(&counts.users)
        .enumerate()
        .map(|(k, (step, &n))| {
            let prev = if k == 0 { n } else { counts.users[k - 1] };
            vec![
                CellValue::String(step.clone()),
                CellValue::Int(n),
                share(n, first),
                share(n, prev),
                CellValue::Int(prev - n),
            ]
        })
        .collect();
    let types = ["Utf8", "Int64", "Float64", "Float64", "Int64"];
    DataTable {
        columns: COLUMNS
            .iter()
            .zip(types)
            .map(|(n, t)| ColumnInfo {
                name: (*n).into(),
                data_type: t.into(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    }
}

/// The distinct values of the event column, in order of first appearance,
/// at most `cap`: what the dialog offers as steps. The flag says more exist.
pub fn event_values(table: &DataTable, event: usize, cap: usize) -> (Vec<String>, bool) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for r in 0..table.row_count() {
        let Some(v) = table
            .get(r, event)
            .filter(|v| !matches!(v, CellValue::Null))
        else {
            continue;
        };
        let v = v.to_string();
        if v.trim().is_empty() || !seen.insert(v.clone()) {
            continue;
        }
        if out.len() == cap {
            return (out, true);
        }
        out.push(v);
    }
    (out, false)
}

/// Step names from the CLI's and MCP's comma list or array, trimmed.
pub fn steps_from(parts: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<String> {
    parts
        .into_iter()
        .map(|s| s.as_ref().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "funnel_tests.rs"]
mod tests;
