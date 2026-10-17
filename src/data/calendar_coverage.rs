//! Calendar coverage: is a time column actually continuous?
//!
//! A daily series with four days missing in March looks perfectly healthy in
//! every per-column statistic there is - no nulls, no outliers, a sensible
//! range. The only way to see it is to walk the calendar.
//!
//! Two false alarms would make this useless, and both are ruled out rather
//! than reported:
//!
//! - **A weekday-only series is not broken.** Business data skips Saturdays and
//!   Sundays by design, and a report that flags 104 gaps a year for it is a
//!   report nobody reads twice.
//! - **A daylight-saving change is not missing data.** Octa's timestamps are
//!   naive local times, so a spring-forward reads as a one-step hole. There is
//!   no timezone to check against, so the shape is used instead: one step
//!   missing, on a Sunday, in the small hours. That is a heuristic and is
//!   labelled as one.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Weekday};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Below this many distinct timestamps there is no series to speak of.
pub const MIN_POINTS: usize = 3;

/// The modal gap has to account for at least this share of the intervals
/// before it can be called the series' step. Below it, the series is
/// irregular and every "gap" would be an artefact of that.
pub const DOMINANCE: f64 = 0.6;

/// How many gaps to list. A column with thousands is telling you one thing,
/// and it is not the list.
pub const MAX_GAPS: usize = 200;

/// What one gap is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// Time that should have a row and does not.
    Missing,
    /// A weekend skipped by a daily series. Expected, not reported as missing.
    Weekend,
    /// The shape of a daylight-saving change. A guess, not a fact.
    DaylightSaving,
}

impl GapKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Weekend => "weekend",
            Self::DaylightSaving => "daylight_saving",
        }
    }
}

/// One hole in the series.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub after: NaiveDateTime,
    pub before: NaiveDateTime,
    /// How many steps of the dominant interval are absent.
    pub missing_steps: u64,
    pub kind: GapKind,
}

/// What the column has to say.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The test does not apply. `reason` is the report cell, one of the
    /// `not tested: ...` constants below.
    Skipped(&'static str),
    Checked {
        /// The series' step, as a `chrono::Duration`.
        step: Duration,
        /// Every hole, capped at [`MAX_GAPS`].
        gaps: Vec<Gap>,
        /// How many of them are real absences.
        missing: usize,
    },
}

impl Outcome {
    /// The verdict cell: English and stable, like `benford_verdict`'s, and
    /// written to be read rather than decoded. The localized explanation is
    /// the cell's own hover text, from [`VALUE_HINTS`].
    pub fn id(&self) -> &'static str {
        match self {
            Self::Skipped(reason) => reason,
            Self::Checked { gaps, missing, .. } => {
                if *missing > 0 {
                    GAPS
                } else if gaps.iter().any(|g| g.kind == GapKind::Weekend) {
                    WEEKDAYS_ONLY
                } else if gaps.iter().any(|g| g.kind == GapKind::DaylightSaving) {
                    COMPLETE_DST
                } else {
                    COMPLETE
                }
            }
        }
    }
}

/// Every value `calendar_verdict` can hold, each with the i18n key that says
/// what it means. Same contract as [`crate::data::benford::VALUE_HINTS`], and
/// kept exhaustive by `every_calendar_value_is_explained`.
pub const VALUE_HINTS: &[(&str, &str)] = &[
    (COMPLETE, "quality.verdict_calendar_complete"),
    (WEEKDAYS_ONLY, "quality.verdict_calendar_weekdays"),
    (COMPLETE_DST, "quality.verdict_calendar_dst"),
    (GAPS, "quality.verdict_calendar_gaps"),
    (NOT_A_DATE, "quality.verdict_calendar_not_dates"),
    (TOO_FEW_POINTS, "quality.verdict_calendar_too_few"),
    (IRREGULAR, "quality.verdict_calendar_irregular"),
];

/// The seven cells this column can hold. Named rather than written out at each
/// site, so [`VALUE_HINTS`] and [`Outcome::id`] cannot drift apart in spelling.
/// That is a real hazard here: `not tested: too few points` sits one word away
/// from Benford's `not tested: too few values`.
const COMPLETE: &str = "complete";
const WEEKDAYS_ONLY: &str = "weekdays only";
const COMPLETE_DST: &str = "complete (clock change)";
const GAPS: &str = "gaps";
const NOT_A_DATE: &str = "not tested: not dates";
const TOO_FEW_POINTS: &str = "not tested: too few points";
const IRREGULAR: &str = "not tested: no regular step";

/// Walk one column's calendar.
pub fn analyse(cells: &[&CellValue], data_type: &str) -> Outcome {
    if !is_time_type(data_type) {
        return Outcome::Skipped(NOT_A_DATE);
    }
    let mut points: Vec<NaiveDateTime> = cells.iter().filter_map(|c| parse(c)).collect();
    points.sort_unstable();
    points.dedup();
    if points.len() < MIN_POINTS {
        return Outcome::Skipped(TOO_FEW_POINTS);
    }

    let deltas: Vec<i64> = points
        .windows(2)
        .map(|w| (w[1] - w[0]).num_seconds())
        .filter(|d| *d > 0)
        .collect();
    let Some(step_secs) = dominant(&deltas) else {
        return Outcome::Skipped(IRREGULAR);
    };
    let step = Duration::seconds(step_secs);

    let mut gaps = Vec::new();
    let mut missing = 0usize;
    for w in points.windows(2) {
        let delta = (w[1] - w[0]).num_seconds();
        if delta <= step_secs {
            continue;
        }
        // A delta of 3 steps means 2 absent points between the two present
        // ones, so the count is steps-minus-one.
        let steps = (delta / step_secs).saturating_sub(1) as u64;
        if steps == 0 {
            continue;
        }
        let kind = classify(w[0], w[1], step_secs, steps);
        if kind == GapKind::Missing {
            missing += 1;
        }
        if gaps.len() < MAX_GAPS {
            gaps.push(Gap {
                after: w[0],
                before: w[1],
                missing_steps: steps,
                kind,
            });
        }
    }

    Outcome::Checked {
        step,
        gaps,
        missing,
    }
}

/// The most common delta, if it is common enough to be called the step.
fn dominant(deltas: &[i64]) -> Option<i64> {
    if deltas.is_empty() {
        return None;
    }
    let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for d in deltas {
        *counts.entry(*d).or_insert(0) += 1;
    }
    // Ties go to the shorter delta: it is the one that makes the series denser,
    // and a HashMap's order would otherwise decide.
    let (&step, &count) = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))?;
    (count as f64 / deltas.len() as f64 >= DOMINANCE).then_some(step)
}

/// Decide whether a hole is a real absence or an expected one.
fn classify(after: NaiveDateTime, before: NaiveDateTime, step_secs: i64, steps: u64) -> GapKind {
    const DAY: i64 = 86_400;
    // A daily series that runs Friday to Monday has skipped the weekend, which
    // is what business data does. Both endpoints have to agree, so a Friday to
    // Wednesday hole is still missing data.
    if step_secs == DAY
        && steps == 2
        && after.weekday() == Weekday::Fri
        && before.weekday() == Weekday::Mon
    {
        return GapKind::Weekend;
    }
    // One step missing, on a Sunday, in the small hours, in a sub-daily series:
    // the shape of a spring-forward. Octa's timestamps carry no timezone, so
    // this is inference and is labelled as such.
    if step_secs < DAY && steps == 1 && after.weekday() == Weekday::Sun && after.hour() <= 3 {
        return GapKind::DaylightSaving;
    }
    GapKind::Missing
}

fn is_time_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp")
}

/// Parse the two shapes Octa's readers normalise to. Anything else is not a
/// point on a calendar as far as this is concerned.
fn parse(cell: &CellValue) -> Option<NaiveDateTime> {
    let s = match cell {
        CellValue::Date(s) | CellValue::DateTime(s) | CellValue::String(s) => s.as_str(),
        _ => return None,
    };
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    None
}

/// The gaps of every time column, as the quality report's section table, or
/// `None` when no column has one worth reporting.
///
/// **Weekend gaps are deliberately not listed.** A five-year weekday series has
/// 260 of them and every one is expected; the verdict column already says
/// `weekdays only`, which is the whole finding. What lands here is what a
/// person would have to go and look into.
pub fn section_table(table: &DataTable) -> Option<DataTable> {
    let row_count = table.row_count();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for (col, info) in table.columns.iter().enumerate() {
        let cells: Vec<&CellValue> = (0..row_count).filter_map(|r| table.get(r, col)).collect();
        let Outcome::Checked { gaps, .. } = analyse(&cells, &info.data_type) else {
            continue;
        };
        for gap in gaps.iter().filter(|g| g.kind != GapKind::Weekend) {
            rows.push(vec![
                CellValue::String(info.name.clone()),
                CellValue::DateTime(gap.after.format("%Y-%m-%d %H:%M:%S").to_string()),
                CellValue::DateTime(gap.before.format("%Y-%m-%d %H:%M:%S").to_string()),
                CellValue::Int(gap.missing_steps as i64),
                CellValue::String(gap.kind.id().to_string()),
            ]);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let mut out = DataTable::empty();
    out.columns = [
        ("column", "Utf8"),
        ("gap_after", "DateTime"),
        ("gap_before", "DateTime"),
        ("missing_steps", "Int64"),
        ("kind", "Utf8"),
    ]
    .iter()
    .map(|(name, ty)| ColumnInfo {
        name: (*name).to_string(),
        data_type: (*ty).to_string(),
    })
    .collect();
    out.rows = rows;
    Some(out)
}

#[cfg(test)]
#[path = "calendar_coverage_tests.rs"]
mod tests;
