//! Pure date / time / duration column calculations.
//!
//! Backs the "Date/Time calculation" dialog (`src/app/dialogs/time_calc.rs`),
//! which materialises a new column by running one [`TimeCalcOp`] over each
//! row. Kept egui-free so it's integration-testable.
//!
//! Date / datetime cells are stored as canonical ISO strings
//! (`CellValue::Date` = `YYYY-MM-DD`, `CellValue::DateTime` =
//! `YYYY-MM-DD HH:MM:SS[.f]`). Parsing also accepts the non-ISO layouts
//! understood by [`crate::data::date_infer`] so string columns that haven't
//! been promoted still work.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Utc};

use crate::data::CellValue;
use crate::data::date_infer::{DateLayout, DateTimeLayout};

/// A unit of time. Used both for durations (differences, conversions) and for
/// the amount in an add/subtract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Milliseconds,
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}

impl TimeUnit {
    pub fn label(self) -> &'static str {
        match self {
            TimeUnit::Milliseconds => "Milliseconds",
            TimeUnit::Seconds => "Seconds",
            TimeUnit::Minutes => "Minutes",
            TimeUnit::Hours => "Hours",
            TimeUnit::Days => "Days",
            TimeUnit::Months => "Months",
            TimeUnit::Years => "Years",
        }
    }

    /// Translated display name for the active UI language. `label()` stays as
    /// the English `&'static str` for non-UI uses.
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            TimeUnit::Milliseconds => "time_unit.ms",
            TimeUnit::Seconds => "time_unit.sec",
            TimeUnit::Minutes => "time_unit.min",
            TimeUnit::Hours => "time_unit.hour",
            TimeUnit::Days => "time_unit.day",
            TimeUnit::Months => "time_unit.month",
            TimeUnit::Years => "time_unit.year",
        })
    }

    /// Fixed-length units convertible to a millisecond factor. Months / Years
    /// have no fixed length, so they return `None` and are excluded from
    /// duration conversion.
    fn to_millis(self) -> Option<f64> {
        Some(match self {
            TimeUnit::Milliseconds => 1.0,
            TimeUnit::Seconds => 1_000.0,
            TimeUnit::Minutes => 60_000.0,
            TimeUnit::Hours => 3_600_000.0,
            TimeUnit::Days => 86_400_000.0,
            TimeUnit::Months | TimeUnit::Years => return None,
        })
    }
}

/// A single date / time component to pull out of a date column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateComponent {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    /// ISO weekday, 1 = Monday .. 7 = Sunday.
    Weekday,
}

impl DateComponent {
    pub fn label(self) -> &'static str {
        match self {
            DateComponent::Year => "Year",
            DateComponent::Month => "Month",
            DateComponent::Day => "Day",
            DateComponent::Hour => "Hour",
            DateComponent::Minute => "Minute",
            DateComponent::Second => "Second",
            DateComponent::Weekday => "Weekday (1=Mon..7=Sun)",
        }
    }

    /// Translated display name for the active UI language. `label()` stays as
    /// the English `&'static str` for non-UI uses.
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            DateComponent::Year => "date_component.year",
            DateComponent::Month => "date_component.month",
            DateComponent::Day => "date_component.day",
            DateComponent::Hour => "date_component.hour",
            DateComponent::Minute => "date_component.minute",
            DateComponent::Second => "date_component.second",
            DateComponent::Weekday => "date_component.weekday",
        })
    }
}

/// Epoch precision for Unix-timestamp conversion (`TimeCalcOp::UnixConvert`).
/// The epoch is 1970-01-01 00:00:00 UTC, interpreted without a timezone
/// (matching how the rest of this module treats naive datetimes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixUnit {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
}

impl UnixUnit {
    pub fn label(self) -> &'static str {
        match self {
            UnixUnit::Seconds => "Seconds",
            UnixUnit::Milliseconds => "Milliseconds",
            UnixUnit::Microseconds => "Microseconds",
            UnixUnit::Nanoseconds => "Nanoseconds",
        }
    }

    /// Translated display name for the active UI language. Reuses the shared
    /// `time_unit.*` keys, adding `us` / `ns` for the sub-millisecond units.
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            UnixUnit::Seconds => "time_unit.sec",
            UnixUnit::Milliseconds => "time_unit.ms",
            UnixUnit::Microseconds => "time_unit.us",
            UnixUnit::Nanoseconds => "time_unit.ns",
        })
    }

    /// Nanoseconds in one unit of this precision.
    fn factor_nanos(self) -> i128 {
        match self {
            UnixUnit::Seconds => 1_000_000_000,
            UnixUnit::Milliseconds => 1_000_000,
            UnixUnit::Microseconds => 1_000,
            UnixUnit::Nanoseconds => 1,
        }
    }
}

/// Direction for Unix-timestamp conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixDirection {
    /// Numeric epoch value -> date/time.
    ToDateTime,
    /// Date/time -> numeric epoch value.
    FromDateTime,
}

/// The calculation to run per row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeCalcOp {
    /// `second - first`, expressed in `unit`. Needs two date/datetime cells.
    Difference { unit: TimeUnit },
    /// Shift a date/datetime by `amount` (may be negative) of `unit`.
    AddSubtract { unit: TimeUnit, amount: i64 },
    /// Reinterpret a numeric duration from `from` units into `to` units.
    ConvertDuration { from: TimeUnit, to: TimeUnit },
    /// Pull one component out of a date/datetime.
    Extract { component: DateComponent },
    /// Convert between a Unix epoch timestamp and a date/datetime, in either
    /// direction, at the chosen epoch precision.
    UnixConvert {
        direction: UnixDirection,
        unit: UnixUnit,
    },
}

impl TimeCalcOp {
    /// Whether this op consumes a second input column (only `Difference`).
    pub fn needs_second_input(self) -> bool {
        matches!(self, TimeCalcOp::Difference { .. })
    }
}

/// Arrow-style column type string for the result of `op`. For `AddSubtract`
/// the type depends on the row (date vs datetime), so the caller refines it
/// from the produced values; this returns a sensible default.
pub fn result_type_name(op: TimeCalcOp) -> &'static str {
    match op {
        TimeCalcOp::Difference { .. } | TimeCalcOp::ConvertDuration { .. } => "Float64",
        TimeCalcOp::Extract { .. } => "Int64",
        TimeCalcOp::AddSubtract { unit, .. } => {
            if is_time_unit(unit) {
                "Timestamp(Microsecond, None)"
            } else {
                "Date32"
            }
        }
        TimeCalcOp::UnixConvert { direction, .. } => match direction {
            UnixDirection::ToDateTime => "Timestamp(Microsecond, None)",
            UnixDirection::FromDateTime => "Int64",
        },
    }
}

/// Arrow type string matching a produced cell, so the column type can be set
/// precisely after the values are computed.
pub fn cell_arrow_type(v: &CellValue) -> &'static str {
    match v {
        CellValue::Date(_) => "Date32",
        CellValue::DateTime(_) => "Timestamp(Microsecond, None)",
        CellValue::Int(_) => "Int64",
        CellValue::Float(_) => "Float64",
        _ => "Utf8",
    }
}

/// Run `op` on one row. `a` is the primary input; `b` is the second input for
/// `Difference`. Returns `None` for any cell that can't be interpreted (e.g. a
/// non-date in a date op), so the caller can count and report skipped rows.
pub fn evaluate_cell(op: TimeCalcOp, a: &CellValue, b: Option<&CellValue>) -> Option<CellValue> {
    match op {
        TimeCalcOp::Difference { unit } => {
            let (start, _) = cell_to_datetime(a)?;
            let (end, _) = cell_to_datetime(b?)?;
            Some(CellValue::Float(datetime_diff(start, end, unit)))
        }
        TimeCalcOp::AddSubtract { unit, amount } => {
            let (dt, had_time) = cell_to_datetime(a)?;
            let shifted = shift_datetime(dt, unit, amount)?;
            if had_time || is_time_unit(unit) {
                Some(CellValue::DateTime(format_datetime(shifted)))
            } else {
                Some(CellValue::Date(
                    shifted.date().format("%Y-%m-%d").to_string(),
                ))
            }
        }
        TimeCalcOp::ConvertDuration { from, to } => {
            let value = cell_to_f64(a)?;
            let millis = value * from.to_millis()?;
            Some(CellValue::Float(millis / to.to_millis()?))
        }
        TimeCalcOp::Extract { component } => {
            let (dt, _) = cell_to_datetime(a)?;
            Some(CellValue::Int(extract_component(dt, component)))
        }
        TimeCalcOp::UnixConvert { direction, unit } => match direction {
            UnixDirection::ToDateTime => {
                let total_nanos = cell_to_epoch(a, unit)?;
                let secs = i64::try_from(total_nanos.div_euclid(1_000_000_000)).ok()?;
                let nanos = total_nanos.rem_euclid(1_000_000_000) as u32;
                let dt = DateTime::<Utc>::from_timestamp(secs, nanos)?.naive_utc();
                Some(CellValue::DateTime(format_datetime(dt)))
            }
            UnixDirection::FromDateTime => {
                let (dt, _) = cell_to_datetime(a)?;
                let utc = dt.and_utc();
                let value = match unit {
                    UnixUnit::Seconds => utc.timestamp(),
                    UnixUnit::Milliseconds => utc.timestamp_millis(),
                    UnixUnit::Microseconds => utc.timestamp_micros(),
                    UnixUnit::Nanoseconds => utc.timestamp_nanos_opt()?,
                };
                Some(CellValue::Int(value))
            }
        },
    }
}

/// Interpret a numeric cell as a Unix epoch value in `unit`, returning the
/// total nanoseconds since the epoch. Accepts integers, floats (fractional
/// epoch values), and numeric strings. Integer inputs go through `i128` so a
/// nanosecond epoch (~1.7e18) keeps full precision an `f64` would lose.
///
/// `Date` / `DateTime` cells are accepted too: a source can carry an epoch
/// column already typed as `Timestamp(...)` while the cell still holds the
/// raw number as text, so we parse their string content rather than rejecting
/// them. A genuinely formatted date string (non-numeric) fails the parse and
/// is skipped, which is correct - it isn't an epoch number.
fn cell_to_epoch(v: &CellValue, unit: UnixUnit) -> Option<i128> {
    let factor = unit.factor_nanos();
    match v {
        CellValue::Int(i) => Some(*i as i128 * factor),
        CellValue::Float(f) => Some((*f * factor as f64) as i128),
        CellValue::String(s) | CellValue::DateTime(s) | CellValue::Date(s) => {
            let t = s.trim();
            if let Ok(i) = t.parse::<i64>() {
                Some(i as i128 * factor)
            } else {
                Some((t.parse::<f64>().ok()? * factor as f64) as i128)
            }
        }
        _ => None,
    }
}

fn is_time_unit(unit: TimeUnit) -> bool {
    matches!(
        unit,
        TimeUnit::Milliseconds | TimeUnit::Seconds | TimeUnit::Minutes | TimeUnit::Hours
    )
}

/// `end - start` in the requested unit. Month / Year differences use calendar
/// arithmetic; all other units use the elapsed millisecond span.
fn datetime_diff(start: NaiveDateTime, end: NaiveDateTime, unit: TimeUnit) -> f64 {
    match unit {
        TimeUnit::Months => months_between(start, end) as f64,
        TimeUnit::Years => months_between(start, end) as f64 / 12.0,
        _ => {
            let millis = (end - start).num_milliseconds() as f64;
            // Fixed units always have a millisecond factor.
            millis / unit.to_millis().unwrap_or(1.0)
        }
    }
}

/// Signed count of whole calendar months from `a` to `b`.
fn months_between(a: NaiveDateTime, b: NaiveDateTime) -> i64 {
    let mut months =
        (b.year() as i64 - a.year() as i64) * 12 + (b.month() as i64 - a.month() as i64);
    if b.day() < a.day() {
        months -= 1;
    }
    months
}

fn shift_datetime(dt: NaiveDateTime, unit: TimeUnit, amount: i64) -> Option<NaiveDateTime> {
    match unit {
        TimeUnit::Milliseconds => dt.checked_add_signed(Duration::milliseconds(amount)),
        TimeUnit::Seconds => dt.checked_add_signed(Duration::seconds(amount)),
        TimeUnit::Minutes => dt.checked_add_signed(Duration::minutes(amount)),
        TimeUnit::Hours => dt.checked_add_signed(Duration::hours(amount)),
        TimeUnit::Days => dt.checked_add_signed(Duration::days(amount)),
        TimeUnit::Months => add_months(dt, amount),
        TimeUnit::Years => add_months(dt, amount.checked_mul(12)?),
    }
}

/// Add a signed number of calendar months, clamping the day to the target
/// month's length (so Jan 31 + 1 month = Feb 28/29).
fn add_months(dt: NaiveDateTime, months: i64) -> Option<NaiveDateTime> {
    let d = dt.date();
    let total = (d.year() as i64) * 12 + (d.month0() as i64) + months;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = total.rem_euclid(12) as u32 + 1;
    let day = d.day().min(last_day_of_month(year, month));
    let new_date = NaiveDate::from_ymd_opt(year, month, day)?;
    Some(new_date.and_time(dt.time()))
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    // First day of next month, minus one day.
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn extract_component(dt: NaiveDateTime, component: DateComponent) -> i64 {
    match component {
        DateComponent::Year => dt.year() as i64,
        DateComponent::Month => dt.month() as i64,
        DateComponent::Day => dt.day() as i64,
        DateComponent::Hour => dt.hour() as i64,
        DateComponent::Minute => dt.minute() as i64,
        DateComponent::Second => dt.second() as i64,
        DateComponent::Weekday => dt.weekday().number_from_monday() as i64,
    }
}

/// Canonical datetime string. `%.f` only appends a fraction when nanoseconds
/// are non-zero, so whole seconds render without a trailing dot.
fn format_datetime(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M:%S%.f").to_string()
}

/// Parse a cell into a `NaiveDateTime`, returning whether the source carried a
/// time-of-day (so add/subtract can keep date-only columns date-only).
fn cell_to_datetime(v: &CellValue) -> Option<(NaiveDateTime, bool)> {
    match v {
        CellValue::DateTime(s) | CellValue::Date(s) | CellValue::String(s) => parse_any(s),
        _ => None,
    }
}

fn parse_any(s: &str) -> Option<(NaiveDateTime, bool)> {
    let t = s.trim();
    const DT_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in DT_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(t, fmt) {
            return Some((dt, true));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some((d.and_hms_opt(0, 0, 0)?, false));
    }
    // Non-ISO layouts: reuse date_infer's parsers, which return canonical ISO.
    for layout in DateTimeLayout::ALL {
        if let Some(canon) = layout.parse(t) {
            for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
                if let Ok(dt) = NaiveDateTime::parse_from_str(&canon, fmt) {
                    return Some((dt, true));
                }
            }
        }
    }
    for layout in DateLayout::ALL {
        if let Some(canon) = layout.parse(t)
            && let Ok(d) = NaiveDate::parse_from_str(&canon, "%Y-%m-%d")
        {
            return Some((d.and_hms_opt(0, 0, 0)?, false));
        }
    }
    None
}

fn cell_to_f64(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "time_calc_tests.rs"]
mod tests;
