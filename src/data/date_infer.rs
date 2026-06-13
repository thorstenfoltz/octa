//! Column-wide date / datetime inference.
//!
//! Many readers (CSV in particular, but also JSON/JSONL/Excel/XML/etc.) hand
//! us string columns whose values are clearly dates in a non-ISO layout - the
//! common European `DD.MM.YYYY`, the US `MM/DD/YYYY`, slash-separated ISO,
//! and so on. The reader-level cell inference in `csv_reader.rs` only
//! recognizes ISO `YYYY-MM-DD`, so those columns load as plain strings and
//! never get the typed-date affordances (sort-as-date, formatting, etc.).
//!
//! This module runs a *post-load* pass over a `DataTable`: for each string
//! column, it tests whether **every** non-null value parses successfully
//! under a single layout. The constraint that the month component must be
//! `1..=12` (and day `1..=31`) eliminates wrong layouts naturally as soon as
//! any row has a first-component greater than 12 (so DD/MM/YYYY drops out
//! of consideration on a value like `13/04/2024`).
//!
//! Outcomes per column:
//! - **Single layout passes** -> promote: rewrite cells in-place to typed
//!   `CellValue::Date` / `CellValue::DateTime` in canonical ISO form.
//! - **No layout passes** -> leave as string.
//! - **Multiple layouts pass** (e.g., `02/03/2024` is consistent with both
//!   DMY and MDY) -> return an `Ambiguous` outcome and let the UI ask the user
//!   how to interpret the column.

use crate::data::{CellValue, DataTable};

/// One concrete "every component in this position" parse layout. The string
/// representation in [`DateLayout::label`] is what the UI shows the user when
/// asking to disambiguate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateLayout {
    /// `YYYY-MM-DD`
    YmdDash,
    /// `YYYY/MM/DD`
    YmdSlash,
    /// `DD.MM.YYYY` (European, dot)
    DmyDot,
    /// `DD-MM-YYYY` (European, dash)
    DmyDash,
    /// `DD/MM/YYYY` (European, slash)
    DmySlash,
    /// `MM-DD-YYYY` (US, dash)
    MdyDash,
    /// `MM/DD/YYYY` (US, slash)
    MdySlash,
}

impl DateLayout {
    /// All candidate date layouts considered by the inference pass.
    pub const ALL: &'static [DateLayout] = &[
        Self::YmdDash,
        Self::YmdSlash,
        Self::DmyDot,
        Self::DmyDash,
        Self::DmySlash,
        Self::MdyDash,
        Self::MdySlash,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::YmdDash => "YYYY-MM-DD",
            Self::YmdSlash => "YYYY/MM/DD",
            Self::DmyDot => "DD.MM.YYYY (European)",
            Self::DmyDash => "DD-MM-YYYY (European)",
            Self::DmySlash => "DD/MM/YYYY (European)",
            Self::MdyDash => "MM-DD-YYYY (US)",
            Self::MdySlash => "MM/DD/YYYY (US)",
        }
    }

    /// Whether this layout's source representation matches the canonical
    /// ISO display format (`YYYY-MM-DD`). Used to decide whether a promotion
    /// of a column under this layout is a visible format change worth
    /// surfacing to the user.
    pub fn is_canonical(self) -> bool {
        matches!(self, Self::YmdDash)
    }

    fn fmt_str(self) -> &'static str {
        match self {
            Self::YmdDash => "%Y-%m-%d",
            Self::YmdSlash => "%Y/%m/%d",
            Self::DmyDot => "%d.%m.%Y",
            Self::DmyDash => "%d-%m-%Y",
            Self::DmySlash => "%d/%m/%Y",
            Self::MdyDash => "%m-%d-%Y",
            Self::MdySlash => "%m/%d/%Y",
        }
    }

    /// Try to parse a single value under this layout. Returns the canonical
    /// ISO `YYYY-MM-DD` form on success.
    pub fn parse(self, s: &str) -> Option<String> {
        let trimmed = s.trim();
        chrono::NaiveDate::parse_from_str(trimmed, self.fmt_str())
            .ok()
            .map(|d| d.format("%Y-%m-%d").to_string())
    }
}

/// Datetime layout = a `DateLayout` plus a separator and time precision.
///
/// Timezone-aware variants (`*Tz`) accept ISO offsets (`Z`, `+02:00`, `+0200`,
/// `-05:00`, ...) and **normalize every value to UTC** before storing it. This
/// keeps cross-row comparison consistent (all instants land in the same
/// timeline) at the price of silently shifting wall-clock times; the format
/// banner surfaces that to the user the same way it does for European date
/// promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeLayout {
    /// `YYYY-MM-DD HH:MM[:SS]`
    YmdDashSpace,
    /// `YYYY-MM-DDTHH:MM[:SS]`
    YmdDashT,
    /// `DD.MM.YYYY HH:MM[:SS]`
    DmyDotSpace,
    /// `DD/MM/YYYY HH:MM[:SS]`
    DmySlashSpace,
    /// `MM/DD/YYYY HH:MM[:SS]`
    MdySlashSpace,
    /// `YYYY-MM-DD HH:MM[:SS]<tz>` - ISO with space separator + offset.
    YmdDashSpaceTz,
    /// `YYYY-MM-DDTHH:MM[:SS]<tz>` - ISO with `T` separator + offset.
    YmdDashTTz,
}

impl DateTimeLayout {
    pub const ALL: &'static [DateTimeLayout] = &[
        Self::YmdDashSpace,
        Self::YmdDashT,
        Self::DmyDotSpace,
        Self::DmySlashSpace,
        Self::MdySlashSpace,
        Self::YmdDashSpaceTz,
        Self::YmdDashTTz,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::YmdDashSpace => "YYYY-MM-DD HH:MM:SS",
            Self::YmdDashT => "YYYY-MM-DDTHH:MM:SS",
            Self::DmyDotSpace => "DD.MM.YYYY HH:MM:SS (European)",
            Self::DmySlashSpace => "DD/MM/YYYY HH:MM:SS (European)",
            Self::MdySlashSpace => "MM/DD/YYYY HH:MM:SS (US)",
            Self::YmdDashSpaceTz => "YYYY-MM-DD HH:MM:SS+/-HH:MM (-> UTC)",
            Self::YmdDashTTz => "YYYY-MM-DDTHH:MM:SS+/-HH:MM (-> UTC)",
        }
    }

    /// Whether this layout matches the canonical ISO display
    /// (`YYYY-MM-DD HH:MM:SS`). The `T` separator and any tz-aware variant
    /// count as different because the displayed cell uses a space and the
    /// original wall-clock time may have been shifted.
    pub fn is_canonical(self) -> bool {
        matches!(self, Self::YmdDashSpace)
    }

    /// Whether this layout consumes a timezone offset. Tz-aware layouts
    /// branch in [`parse`] to use `DateTime::parse_from_str` and normalize
    /// to UTC.
    fn has_timezone(self) -> bool {
        matches!(self, Self::YmdDashSpaceTz | Self::YmdDashTTz)
    }

    /// Try to parse the value under this layout, allowing `HH:MM`, `HH:MM:SS`,
    /// and arbitrary-precision fractional seconds (`HH:MM:SS.fffffffff`).
    /// Returns the canonical ISO form `YYYY-MM-DD HH:MM:SS` when the source
    /// had no fractional component, and `YYYY-MM-DD HH:MM:SS.<fraction>`
    /// when it did. Chrono's `%.f` formatter emits the fractional suffix
    /// only when nanoseconds > 0, so whole-second timestamps still render
    /// without a trailing dot.
    ///
    /// For timezone-aware layouts the source value is shifted to UTC before
    /// formatting; the offset itself is not preserved in the canonical
    /// string because the underlying `Timestamp(Microsecond, None)` cell
    /// type has no slot for it.
    pub fn parse(self, s: &str) -> Option<String> {
        let trimmed = s.trim();
        if self.has_timezone() {
            // Chrono doesn't accept the bare `Z` (Zulu/UTC) suffix when
            // parsing through a `%:z` directive; rewrite it to `+00:00` so
            // a single offset pattern handles both ISO conventions.
            let candidate: std::borrow::Cow<str> =
                if let Some(rest) = trimmed.strip_suffix(['Z', 'z']) {
                    std::borrow::Cow::Owned(format!("{rest}+00:00"))
                } else {
                    std::borrow::Cow::Borrowed(trimmed)
                };
            for fmt in self.candidate_formats() {
                if let Ok(dt) = chrono::DateTime::parse_from_str(&candidate, fmt) {
                    return Some(dt.naive_utc().format("%Y-%m-%d %H:%M:%S%.f").to_string());
                }
            }
            None
        } else {
            for fmt in self.candidate_formats() {
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
                    return Some(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string());
                }
            }
            None
        }
    }

    fn candidate_formats(self) -> &'static [&'static str] {
        match self {
            Self::YmdDashSpace => &[
                "%Y-%m-%d %H:%M:%S%.f",
                "%Y-%m-%d %H:%M:%S",
                "%Y-%m-%d %H:%M",
            ],
            Self::YmdDashT => &[
                "%Y-%m-%dT%H:%M:%S%.f",
                "%Y-%m-%dT%H:%M:%S",
                "%Y-%m-%dT%H:%M",
            ],
            Self::DmyDotSpace => &[
                "%d.%m.%Y %H:%M:%S%.f",
                "%d.%m.%Y %H:%M:%S",
                "%d.%m.%Y %H:%M",
            ],
            Self::DmySlashSpace => &[
                "%d/%m/%Y %H:%M:%S%.f",
                "%d/%m/%Y %H:%M:%S",
                "%d/%m/%Y %H:%M",
            ],
            Self::MdySlashSpace => &[
                "%m/%d/%Y %H:%M:%S%.f",
                "%m/%d/%Y %H:%M:%S",
                "%m/%d/%Y %H:%M",
            ],
            // `%:z` accepts both `+02:00` and `+0200`; chrono's parser is
            // permissive about the colon. The `Z` suffix is rewritten to
            // `+00:00` in `parse()` before reaching these patterns.
            Self::YmdDashSpaceTz => &[
                "%Y-%m-%d %H:%M:%S%.f%:z",
                "%Y-%m-%d %H:%M:%S%:z",
                "%Y-%m-%d %H:%M%:z",
            ],
            Self::YmdDashTTz => &[
                "%Y-%m-%dT%H:%M:%S%.f%:z",
                "%Y-%m-%dT%H:%M:%S%:z",
                "%Y-%m-%dT%H:%M%:z",
            ],
        }
    }
}

/// Result of a column-wide inference pass.
#[derive(Debug, Clone)]
pub enum InferOutcome {
    /// No layout matches every non-null value.
    Skip,
    /// Exactly one date layout matches every non-null value.
    PromotedDate(DateLayout),
    /// Exactly one datetime layout matches every non-null value.
    PromotedDateTime(DateTimeLayout),
    /// Multiple date layouts match (e.g. DD/MM and MM/DD both consistent).
    /// The UI must ask the user which to use. Sample values are included so
    /// the dialog can show concrete examples.
    AmbiguousDate {
        candidates: Vec<DateLayout>,
        samples: Vec<String>,
    },
    /// Multiple datetime layouts match.
    AmbiguousDateTime {
        candidates: Vec<DateTimeLayout>,
        samples: Vec<String>,
    },
    /// No single layout matches every value, but the closest date/datetime
    /// layout matches *most* of them - the column looks date-shaped yet some
    /// values cannot be parsed, so it stays text. Carries the closest layout's
    /// label, how many of `total` non-null values parsed, and a few offending
    /// raw values so the UI can explain why promotion was skipped.
    Failed {
        label: &'static str,
        parsed: usize,
        total: usize,
        failures: Vec<String>,
    },
}

/// Inspect a column of strings and report what date/datetime layout - if any
/// - every non-null value matches.
pub fn infer_column(values: &[Option<&str>]) -> InferOutcome {
    let non_null: Vec<&str> = values
        .iter()
        .copied()
        .filter_map(|v| v.filter(|s| !s.trim().is_empty()))
        .collect();
    if non_null.is_empty() {
        return InferOutcome::Skip;
    }

    let date_passing: Vec<DateLayout> = DateLayout::ALL
        .iter()
        .copied()
        .filter(|layout| non_null.iter().all(|v| layout.parse(v).is_some()))
        .collect();

    if !date_passing.is_empty() {
        return match date_passing.len() {
            1 => InferOutcome::PromotedDate(date_passing[0]),
            _ => InferOutcome::AmbiguousDate {
                candidates: date_passing,
                samples: sample_values(&non_null),
            },
        };
    }

    let dt_passing: Vec<DateTimeLayout> = DateTimeLayout::ALL
        .iter()
        .copied()
        .filter(|layout| non_null.iter().all(|v| layout.parse(v).is_some()))
        .collect();

    match dt_passing.len() {
        0 => best_near_miss(&non_null),
        1 => InferOutcome::PromotedDateTime(dt_passing[0]),
        _ => InferOutcome::AmbiguousDateTime {
            candidates: dt_passing,
            samples: sample_values(&non_null),
        },
    }
}

/// When no layout matches every value, find the layout that matches the most
/// and decide whether the column is a "near miss" worth flagging. Returns
/// [`InferOutcome::Failed`] when the best layout parses a majority (> 50%) but
/// not all of the non-null values; otherwise [`InferOutcome::Skip`] (the column
/// just isn't a date). Up to five offending values are captured for the notice.
fn best_near_miss(non_null: &[&str]) -> InferOutcome {
    let total = non_null.len();
    if total == 0 {
        return InferOutcome::Skip;
    }
    let mut best_label = "";
    let mut best_parsed = 0usize;
    let mut best_failures: Vec<String> = Vec::new();

    {
        let mut consider = |label: &'static str, parses: &dyn Fn(&str) -> bool| {
            let mut parsed = 0usize;
            let mut failures: Vec<String> = Vec::new();
            for v in non_null {
                if parses(v) {
                    parsed += 1;
                } else if failures.len() < 5 {
                    failures.push((*v).to_string());
                }
            }
            if parsed > best_parsed {
                best_parsed = parsed;
                best_label = label;
                best_failures = failures;
            }
        };

        for layout in DateLayout::ALL {
            consider(layout.label(), &|s| layout.parse(s).is_some());
        }
        for layout in DateTimeLayout::ALL {
            consider(layout.label(), &|s| layout.parse(s).is_some());
        }
    }

    // Majority match but not unanimous: looks like dates, some values bad.
    if best_parsed * 2 > total && best_parsed < total {
        InferOutcome::Failed {
            label: best_label,
            parsed: best_parsed,
            total,
            failures: best_failures,
        }
    } else {
        InferOutcome::Skip
    }
}

fn sample_values(non_null: &[&str]) -> Vec<String> {
    non_null.iter().take(5).map(|s| s.to_string()).collect()
}

/// Apply a single date layout to every value in `col_idx`, rewriting
/// `CellValue::String` cells in-place to `CellValue::Date` and updating the
/// column's data_type. Cells that fail to parse become `CellValue::Null` -
/// callers should only invoke this with a layout produced by
/// [`infer_column`], where every non-null value is known to parse.
pub fn apply_date(table: &mut DataTable, col_idx: usize, layout: DateLayout) {
    if col_idx >= table.columns.len() {
        return;
    }
    let n = table.row_count();
    for row in 0..n {
        let new_cell = match table.get(row, col_idx) {
            Some(CellValue::String(s)) => match layout.parse(s) {
                Some(canonical) => Some(CellValue::Date(canonical)),
                None => Some(CellValue::Null),
            },
            Some(CellValue::Null) => None,
            _ => None,
        };
        if let Some(v) = new_cell {
            table.rows[row][col_idx] = v;
        }
    }
    table.columns[col_idx].data_type = "Date32".to_string();
}

/// Mirror of [`apply_date`] for datetime layouts.
pub fn apply_datetime(table: &mut DataTable, col_idx: usize, layout: DateTimeLayout) {
    if col_idx >= table.columns.len() {
        return;
    }
    let n = table.row_count();
    for row in 0..n {
        let new_cell = match table.get(row, col_idx) {
            Some(CellValue::String(s)) => match layout.parse(s) {
                Some(canonical) => Some(CellValue::DateTime(canonical)),
                None => Some(CellValue::Null),
            },
            Some(CellValue::Null) => None,
            _ => None,
        };
        if let Some(v) = new_cell {
            table.rows[row][col_idx] = v;
        }
    }
    table.columns[col_idx].data_type = "Timestamp(Microsecond, None)".to_string();
}

/// Whether a column is a candidate for the inference pass: must be string-
/// typed (`Utf8`) and contain `CellValue::String`-shaped data. Already-typed
/// columns are skipped - readers that produce typed dates (Parquet, Arrow,
/// SQLite, etc.) are authoritative.
pub fn column_is_candidate(table: &DataTable, col_idx: usize) -> bool {
    let Some(col) = table.columns.get(col_idx) else {
        return false;
    };
    if col.data_type != "Utf8" && col.data_type != "LargeUtf8" {
        return false;
    }
    let n = table.row_count();
    if n == 0 {
        return false;
    }
    let mut has_string = false;
    for row in 0..n {
        match table.get(row, col_idx) {
            Some(CellValue::String(_)) => has_string = true,
            Some(CellValue::Null) => {}
            _ => return false,
        }
    }
    has_string
}

/// Collect a column's values as `Option<&str>` for [`infer_column`]. Strings
/// pass through, nulls become `None`, anything else flips the column out of
/// the candidate pool by returning an empty vec.
pub fn collect_column_strings(table: &DataTable, col_idx: usize) -> Vec<Option<&str>> {
    let n = table.row_count();
    let mut out = Vec::with_capacity(n);
    for row in 0..n {
        match table.get(row, col_idx) {
            Some(CellValue::String(s)) => out.push(Some(s.as_str())),
            Some(CellValue::Null) => out.push(None),
            _ => return Vec::new(),
        }
    }
    out
}

#[cfg(test)]
#[path = "date_infer_tests.rs"]
mod tests;
