//! Number bands: numbers into labelled ranges (`18-34`, `65+`, `<18`), and
//! labelled ranges back to numbers. Shared by the dialog, `--bands` and the
//! MCP tool `number_bands`.
//!
//! **To bands**: equal widths, equal counts (quartiles, deciles) or edges of
//! your own. A band holds its lower edge and not its upper one, except the
//! last band of equal widths or counts, which ends at the largest value. A
//! column of whole numbers gets whole-number labels (`18-34` holds 34), any
//! other column interval labels (`[1.5, 3)`).
//!
//! **Back to numbers**: the labels can come from anywhere, so they are read
//! loosely. Two numbers are a range, whatever stands between them (`18-24`,
//! `18 to 24`, `18 bis 24`, `[18, 25)`); `10,000` and `20k` read as numbers;
//! `65+`, `>65` and `<18` are open. A label with one number and words
//! (`65 und älter`, `unter 18`) is placed by the other labels: at or above
//! the highest band it is the open top, at or below the lowest the open
//! bottom. An open band takes the width of the band next to it, so `65+`
//! beside `55-64` reads as 65 to 74.

use crate::data::{CellValue, DataTable};

/// How numbers are cut into bands.
#[derive(Debug, Clone, PartialEq)]
pub enum BandBy {
    /// This many bands of the same width.
    Width(usize),
    /// This many bands with about as many values each (4: quartiles).
    Count(usize),
    /// Edges of your own: below the first, between each two, from the last.
    Edges(Vec<f64>),
}

impl BandBy {
    pub const IDS: [&'static str; 3] = ["width", "count", "edges"];
}

/// Which number a band gives back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Back {
    #[default]
    Middle,
    Lower,
    Upper,
}

impl Back {
    pub const ALL: [Back; 3] = [Back::Middle, Back::Lower, Back::Upper];

    pub fn id(self) -> &'static str {
        match self {
            Back::Middle => "middle",
            Back::Lower => "lower",
            Back::Upper => "upper",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.id().eq_ignore_ascii_case(s.trim()))
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Back::Middle => "bands.back_middle",
            Back::Lower => "bands.back_lower",
            Back::Upper => "bands.back_upper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandError {
    /// The column holds no numbers.
    NoNumbers,
    /// Fewer than one band, or no edges.
    NoBands,
}

impl BandError {
    pub fn key(self) -> &'static str {
        match self {
            Self::NoNumbers => "bands.err_no_numbers",
            Self::NoBands => "bands.err_no_bands",
        }
    }
}

/// The most bands Width and Count make.
pub const MAX_BANDS: usize = 1000;

/// A cell as a number: numbers, and text that reads as one.
fn number(c: &CellValue) -> Option<f64> {
    match c {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => f.is_finite().then_some(*f),
        CellValue::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// One band: `lo <= v < hi`, or `<= hi` when `closed`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Band {
    lo: Option<f64>,
    hi: Option<f64>,
    closed: bool,
}

impl Band {
    fn label(self, whole: bool) -> String {
        match (self.lo, self.hi) {
            (None, Some(hi)) => format!("<{}", fmt(hi)),
            (Some(lo), None) => format!("{}+", fmt(lo)),
            (Some(lo), Some(hi)) if whole => {
                let top = if self.closed { hi } else { hi - 1.0 };
                let top = top.max(lo);
                if lo == top {
                    fmt(lo)
                } else if lo < 0.0 {
                    format!("{} to {}", fmt(lo), fmt(top))
                } else {
                    format!("{}-{}", fmt(lo), fmt(top))
                }
            }
            (Some(lo), Some(hi)) => {
                let close = if self.closed { ']' } else { ')' };
                format!("[{}, {}{close}", fmt(lo), fmt(hi))
            }
            (None, None) => String::new(),
        }
    }
}

/// The band labels of `values`; a cell that is no number stays empty.
pub fn to_bands(values: &[CellValue], by: &BandBy) -> Result<Vec<CellValue>, BandError> {
    let nums: Vec<Option<f64>> = values.iter().map(number).collect();
    let mut sorted: Vec<f64> = nums.iter().flatten().copied().collect();
    if sorted.is_empty() {
        return Err(BandError::NoNumbers);
    }
    sorted.sort_by(f64::total_cmp);
    let (min, max) = (sorted[0], sorted[sorted.len() - 1]);
    let whole_values = sorted.iter().all(|v| v.fract() == 0.0);
    // Inner cut points: a value at or above cut k is in band k + 1.
    let (cuts, first_open, whole): (Vec<f64>, bool, bool) = match by {
        BandBy::Width(n) | BandBy::Count(n) if *n == 0 || *n > MAX_BANDS => {
            return Err(BandError::NoBands);
        }
        BandBy::Width(n) => {
            // Whole numbers get a whole width, so the labels stay whole.
            let w = if whole_values {
                ((max - min + 1.0) / *n as f64).ceil().max(1.0)
            } else {
                (max - min) / *n as f64
            };
            let cuts = (1..*n)
                .map(|k| min + k as f64 * w)
                .filter(|c| *c > min && *c <= max)
                .collect();
            (cuts, false, whole_values)
        }
        BandBy::Count(n) => {
            let len = sorted.len();
            let mut cuts: Vec<f64> = (1..*n)
                .map(|k| sorted[(k * len / *n).min(len - 1)])
                .filter(|c| *c > min)
                .collect();
            cuts.dedup();
            (cuts, false, whole_values)
        }
        BandBy::Edges(edges) => {
            let mut cuts: Vec<f64> = edges.iter().copied().filter(|e| e.is_finite()).collect();
            if cuts.is_empty() {
                return Err(BandError::NoBands);
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup();
            let whole = whole_values && cuts.iter().all(|c| c.fract() == 0.0);
            (cuts, true, whole)
        }
    };
    let band = |k: usize| -> Band {
        if first_open {
            Band {
                lo: k.checked_sub(1).map(|i| cuts[i]),
                hi: cuts.get(k).copied(),
                closed: false,
            }
        } else {
            let lo = if k == 0 { min } else { cuts[k - 1] };
            match cuts.get(k) {
                Some(hi) => Band {
                    lo: Some(lo),
                    hi: Some(*hi),
                    closed: false,
                },
                None => Band {
                    lo: Some(lo),
                    hi: Some(max),
                    closed: true,
                },
            }
        }
    };
    Ok(nums
        .into_iter()
        .map(|v| match v {
            Some(v) => CellValue::String(band(cuts.partition_point(|c| *c <= v)).label(whole)),
            None => CellValue::Null,
        })
        .collect())
}

/// What one label says: its numbers and whether an end is marked open.
#[derive(Debug, Clone, PartialEq)]
struct Read {
    numbers: Vec<f64>,
    open_top: bool,
    open_bottom: bool,
    /// Letters besides a `k` or `m` after a number (`65 und älter`).
    words: bool,
}

/// The numbers in a label. A minus is a sign unless a number stands right
/// before it (`18-24` and `20k-30k` are ranges, `-10 to -5` two negatives); a comma before exactly
/// three digits groups thousands; `k` and `m` multiply when no letter
/// follows them (`20k`, not `18 months`).
fn read_label(s: &str) -> Read {
    let chars: Vec<char> = s.chars().collect();
    let mut numbers = Vec::new();
    let mut open_top = false;
    let mut suffixes = Vec::new();
    // Where the last number (and its k or m) ended.
    let mut last_end: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let right_after_a_number =
            last_end.is_some_and(|e| chars[e..i].iter().all(|c| c.is_whitespace()));
        let signed =
            c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && !right_after_a_number;
        if !c.is_ascii_digit() && !signed {
            i += 1;
            continue;
        }
        let mut text = String::new();
        if signed {
            text.push('-');
            i += 1;
        }
        while i < chars.len() {
            let c = chars[i];
            let digits_after = |n: usize| {
                (1..=n).all(|k| chars.get(i + k).is_some_and(char::is_ascii_digit))
                    && !chars.get(i + n + 1).is_some_and(char::is_ascii_digit)
            };
            if c.is_ascii_digit() {
                text.push(c);
            } else if c == ',' && digits_after(3) {
                // A thousands group.
            } else if c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                text.push('.');
            } else {
                break;
            }
            i += 1;
        }
        let Ok(mut v) = text.parse::<f64>() else {
            continue;
        };
        let next_is_letter = |k: usize| chars.get(k).is_some_and(|c| c.is_alphabetic());
        match chars.get(i) {
            Some('k' | 'K') if !next_is_letter(i + 1) => {
                v *= 1e3;
                suffixes.push(i);
                i += 1;
            }
            Some('m' | 'M') if !next_is_letter(i + 1) => {
                v *= 1e6;
                suffixes.push(i);
                i += 1;
            }
            _ => {}
        }
        if chars[i..].iter().find(|c| !c.is_whitespace()) == Some(&'+') {
            open_top = true;
        }
        last_end = Some(i);
        numbers.push(v);
    }
    let first = chars.iter().find(|c| !c.is_whitespace());
    let words = chars
        .iter()
        .enumerate()
        .any(|(k, c)| c.is_alphabetic() && !suffixes.contains(&k));
    Read {
        open_top: open_top || matches!(first, Some('>' | '≥')),
        open_bottom: matches!(first, Some('<' | '≤')),
        numbers,
        words,
    }
}

/// The number each label gives, and how many labels could not be read
/// (no number in them); those cells stay empty.
pub fn from_bands(labels: &[CellValue], back: Back) -> (Vec<CellValue>, usize) {
    let reads: Vec<Option<Read>> = labels
        .iter()
        .map(|c| match c {
            CellValue::Null => None,
            CellValue::Int(_) | CellValue::Float(_) => Some(Read {
                numbers: number(c).into_iter().collect(),
                open_top: false,
                open_bottom: false,
                words: false,
            }),
            other => Some(read_label(&other.to_string())).filter(|r| !r.numbers.is_empty()),
        })
        .collect();
    // The closed bands decide where a lone number sits and how wide an
    // open band is.
    let closed: Vec<(f64, f64)> = reads
        .iter()
        .flatten()
        .filter(|r| r.numbers.len() >= 2)
        .map(|r| {
            let (a, b) = (r.numbers[0], r.numbers[1]);
            (a.min(b), a.max(b))
        })
        .collect();
    let top = closed.iter().max_by(|a, b| a.0.total_cmp(&b.0)).copied();
    let bottom = closed.iter().min_by(|a, b| a.0.total_cmp(&b.0)).copied();
    let mut unread = 0;
    let out = labels
        .iter()
        .zip(&reads)
        .map(|(cell, read)| {
            let Some(r) = read else {
                if !matches!(cell, CellValue::Null) && !cell.to_string().trim().is_empty() {
                    unread += 1;
                }
                return CellValue::Null;
            };
            let (lo, hi) = if r.numbers.len() >= 2 {
                let (a, b) = (r.numbers[0], r.numbers[1]);
                (a.min(b), a.max(b))
            } else {
                let n = r.numbers[0];
                // A lone number with words sits at an end of the other bands.
                let placed = r.words && !r.open_top && !r.open_bottom;
                let up = r.open_top || (placed && top.is_some_and(|(_, hi)| n >= hi));
                let down = r.open_bottom || (placed && bottom.is_some_and(|(lo, _)| n <= lo));
                if up {
                    (n, n + top.map_or(0.0, |(lo, hi)| hi - lo))
                } else if down {
                    (n - bottom.map_or(0.0, |(lo, hi)| hi - lo), n)
                } else {
                    (n, n)
                }
            };
            let v = match back {
                Back::Middle => (lo + hi) / 2.0,
                Back::Lower => lo,
                Back::Upper => hi,
            };
            CellValue::Float(v)
        })
        .collect();
    (out, unread)
}

/// Whole-valued results as `Int64`, else `Float64`.
fn number_column(cells: Vec<CellValue>) -> (Vec<CellValue>, &'static str) {
    let whole = cells.iter().all(|c| match c {
        CellValue::Float(f) => f.fract() == 0.0 && f.abs() < 9.0e15,
        _ => true,
    });
    if !whole {
        return (cells, "Float64");
    }
    let ints = cells
        .into_iter()
        .map(|c| match c {
            CellValue::Float(f) => CellValue::Int(f as i64),
            other => other,
        })
        .collect();
    (ints, "Int64")
}

/// What to add, and how.
#[derive(Debug, Clone, PartialEq)]
pub enum BandSpec {
    ToBands(BandBy),
    Back(Back),
}

/// The new column's name: `<column>_band`, or `<column>_middle` / `_lower` /
/// `_upper`, numbered when taken.
pub fn column_name(table: &DataTable, col: usize, spec: &BandSpec) -> String {
    let base = &table.columns[col].name;
    let wanted = match spec {
        BandSpec::ToBands(_) => format!("{base}_band"),
        BandSpec::Back(b) => format!("{base}_{}", b.id()),
    };
    crate::data::unique_name(&wanted, |n| table.columns.iter().any(|c| c.name == n))
}

/// Add the column right after `col`, through the undoable cell writes. The
/// caller folds the undo entries. Returns its name and the labels that
/// could not be read (always 0 to bands).
pub fn add_band_column(
    table: &mut DataTable,
    col: usize,
    spec: &BandSpec,
) -> Result<(String, usize), BandError> {
    let values: Vec<CellValue> = (0..table.row_count())
        .map(|r| table.get(r, col).cloned().unwrap_or(CellValue::Null))
        .collect();
    let (cells, data_type, unread) = match spec {
        BandSpec::ToBands(by) => (to_bands(&values, by)?, "Utf8", 0),
        BandSpec::Back(back) => {
            let (cells, unread) = from_bands(&values, *back);
            let (cells, ty) = number_column(cells);
            (cells, ty, unread)
        }
    };
    let name = column_name(table, col, spec);
    table.insert_column(col + 1, name.clone(), data_type.to_string());
    for (r, cell) in cells.into_iter().enumerate() {
        if !matches!(cell, CellValue::Null) {
            table.set(r, col + 1, cell);
        }
    }
    Ok((name, unread))
}

/// Edges typed as a list: separated by commas and spaces, or by semicolons
/// when the numbers have decimal commas (`1,5; 3`). `Err` is the first entry
/// that is no number.
pub fn parse_edges(text: &str) -> Result<Vec<f64>, String> {
    let parts: Vec<&str> = if text.contains(';') {
        text.split([';', '\n']).collect()
    } else {
        text.split([',', ' ', '\t', '\n']).collect()
    };
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .ok_or_else(|| p.to_string())
        })
        .collect()
}

#[cfg(test)]
#[path = "bands_tests.rs"]
mod tests;
