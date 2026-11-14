//! Missing numbers: does a numbering run without holes?
//!
//! Invoice and order numbers must often have no gaps, and a missing one looks
//! fine in every per-column statistic. Modelled on `calendar_coverage`: a
//! verdict per column plus a section listing the holes.
//!
//! Two gates keep it quiet where it would only raise false alarms: the values
//! must be almost all different (keeps out ages and quantities), and most
//! steps between neighbours must be exactly 1 (keeps out sparse IDs). Text IDs
//! count on their last run of digits, one series per prefix and suffix, so
//! `INV-2025-0001` starting a new year is not a hole.

use std::collections::BTreeMap;

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Below this many distinct values there is no numbering to speak of.
pub const MIN_POINTS: usize = 3;
/// Share of the values that must be distinct.
pub const DISTINCT: f64 = 0.95;
/// Share of the steps between neighbouring distinct values that must be 1.
/// The calendar check's rule.
pub const DOMINANCE: f64 = 0.6;
/// How many holes to list per column.
pub const MAX_GAPS: usize = 200;

const COMPLETE: &str = "complete";
const GAPS: &str = "gaps";
const REPEAT: &str = "not tested: values repeat";
const NOT_SEQUENCE: &str = "not tested: not a counting sequence";
const TOO_FEW: &str = "not tested: too few values";
const NOT_NUMBERS: &str = "not tested: not numbers or IDs";

/// Every value `number_verdict` can hold, with the i18n key explaining it.
pub const VALUE_HINTS: &[(&str, &str)] = &[
    (COMPLETE, "quality.verdict_numbers_complete"),
    (GAPS, "quality.verdict_numbers_gaps"),
    (REPEAT, "quality.verdict_numbers_repeat"),
    (NOT_SEQUENCE, "quality.verdict_numbers_not_sequence"),
    (TOO_FEW, "quality.verdict_numbers_too_few"),
    (NOT_NUMBERS, "quality.verdict_numbers_not_numbers"),
];

/// A value split into the part before its last run of digits, that run as a
/// number and its width, and the part after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counter {
    pub prefix: String,
    pub number: u64,
    pub width: usize,
    pub suffix: String,
}

impl Counter {
    /// `n` written in this counter's form, padded to its width.
    pub fn format(&self, n: u64) -> String {
        format!("{}{:0w$}{}", self.prefix, n, self.suffix, w = self.width)
    }
}

/// Split at the last run of ASCII digits. `None` without digits, or when the
/// run does not fit a `u64` (nobody types a 20-digit counter).
pub fn split_counter(text: &str) -> Option<Counter> {
    let bytes = text.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    // Digits are ASCII, so both cut points sit on char boundaries.
    let start = bytes[..end]
        .iter()
        .rposition(|b| !b.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let digits = &text[start..end];
    Some(Counter {
        prefix: text[..start].to_string(),
        number: digits.parse().ok()?,
        width: digits.len(),
        suffix: text[end..].to_string(),
    })
}

/// One hole: the first and last missing value, written like the column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hole {
    /// The series, `INV-2024-#` (`#` is the counter); empty for numbers.
    pub series: String,
    pub first: String,
    pub last: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Skipped(&'static str),
    Checked {
        /// Capped at [`MAX_GAPS`].
        holes: Vec<Hole>,
        /// Every missing number, not capped.
        missing: u64,
    },
}

impl Outcome {
    /// The verdict cell, English and stable like `calendar_verdict`.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Skipped(reason) => reason,
            Self::Checked { missing, .. } if *missing > 0 => GAPS,
            Self::Checked { .. } => COMPLETE,
        }
    }
}

fn is_whole_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("int") && !t.contains("interval")
}

#[derive(Default)]
struct Series {
    values: Vec<u64>,
    /// Width of zero-padded values, 0 when none are padded.
    pad: usize,
}

/// The holes of one series, or why it is not a numbering.
fn holes_in(values: &mut Vec<u64>) -> Result<Vec<(u64, u64)>, &'static str> {
    let total = values.len();
    values.sort_unstable();
    values.dedup();
    if values.len() < MIN_POINTS {
        return Err(TOO_FEW);
    }
    if (values.len() as f64) < DISTINCT * total as f64 {
        return Err(REPEAT);
    }
    let steps = values.len() - 1;
    let ones = values.windows(2).filter(|w| w[1] - w[0] == 1).count();
    if (ones as f64) < DOMINANCE * steps as f64 {
        return Err(NOT_SEQUENCE);
    }
    Ok(values
        .windows(2)
        .filter(|w| w[1] - w[0] > 1)
        .map(|w| (w[0] + 1, w[1] - 1))
        .collect())
}

/// Check one column. `cells` are its non-null values.
pub fn analyse(cells: &[&CellValue], data_type: &str) -> Outcome {
    let whole = is_whole_type(data_type);
    if !whole && !crate::data::shapes::is_text_type(data_type) {
        return Outcome::Skipped(NOT_NUMBERS);
    }
    let mut series: BTreeMap<(String, String), Series> = BTreeMap::new();
    for cell in cells {
        match cell {
            CellValue::Int(i) if whole && *i >= 0 => series
                .entry((String::new(), String::new()))
                .or_default()
                .values
                .push(*i as u64),
            CellValue::String(s) if !whole => {
                if let Some(Counter {
                    prefix,
                    number,
                    width,
                    suffix,
                }) = split_counter(s.trim())
                {
                    let e = series.entry((prefix, suffix)).or_default();
                    e.values.push(number);
                    if width > number.to_string().len() {
                        e.pad = e.pad.max(width);
                    }
                }
            }
            _ => {}
        }
    }
    if series.is_empty() {
        return Outcome::Skipped(NOT_NUMBERS);
    }
    let mut holes = Vec::new();
    let mut missing = 0u64;
    let mut checked = false;
    // The reason of the largest series, when none of them is a numbering.
    let mut largest: Option<(usize, &'static str)> = None;
    for ((prefix, suffix), mut s) in series {
        let total = s.values.len();
        match holes_in(&mut s.values) {
            Ok(gaps) => {
                checked = true;
                let form = Counter {
                    prefix: prefix.clone(),
                    number: 0,
                    width: s.pad,
                    suffix: suffix.clone(),
                };
                let name = if whole {
                    String::new()
                } else {
                    format!("{prefix}#{suffix}")
                };
                for (a, b) in gaps {
                    missing += b - a + 1;
                    if holes.len() < MAX_GAPS {
                        holes.push(Hole {
                            series: name.clone(),
                            first: form.format(a),
                            last: form.format(b),
                            count: b - a + 1,
                        });
                    }
                }
            }
            Err(reason) => {
                if largest.is_none_or(|(n, _)| total > n) {
                    largest = Some((total, reason));
                }
            }
        }
    }
    if checked {
        Outcome::Checked { holes, missing }
    } else {
        Outcome::Skipped(largest.map_or(NOT_NUMBERS, |(_, r)| r))
    }
}

/// The "Missing numbers" tab: one row per hole, `None` when there is none.
pub fn section_table(table: &DataTable) -> Option<DataTable> {
    let row_count = table.row_count();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for (col, info) in table.columns.iter().enumerate() {
        let cells: Vec<&CellValue> = (0..row_count)
            .filter_map(|r| table.get(r, col))
            .filter(|c| !matches!(c, CellValue::Null))
            .collect();
        let Outcome::Checked { holes, .. } = analyse(&cells, &info.data_type) else {
            continue;
        };
        for h in holes {
            rows.push(vec![
                CellValue::String(info.name.clone()),
                CellValue::String(h.series),
                CellValue::String(h.first),
                CellValue::String(h.last),
                CellValue::Int(h.count as i64),
            ]);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let mut out = DataTable::empty();
    out.columns = [
        ("column", "Utf8"),
        ("series", "Utf8"),
        ("first_missing", "Utf8"),
        ("last_missing", "Utf8"),
        ("missing_count", "Int64"),
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
#[path = "number_gaps_tests.rs"]
mod tests;
