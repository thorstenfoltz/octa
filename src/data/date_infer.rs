//! Column-wide date / datetime inference.
//!
//! Many readers (CSV in particular, but also JSON/JSONL/Excel/XML/etc.) hand
//! us string columns whose values are clearly dates in a non-ISO layout — the
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
//! - **Single layout passes** → promote: rewrite cells in-place to typed
//!   `CellValue::Date` / `CellValue::DateTime` in canonical ISO form.
//! - **No layout passes** → leave as string.
//! - **Multiple layouts pass** (e.g., `02/03/2024` is consistent with both
//!   DMY and MDY) → return an `Ambiguous` outcome and let the UI ask the user
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
}

impl DateTimeLayout {
    pub const ALL: &'static [DateTimeLayout] = &[
        Self::YmdDashSpace,
        Self::YmdDashT,
        Self::DmyDotSpace,
        Self::DmySlashSpace,
        Self::MdySlashSpace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::YmdDashSpace => "YYYY-MM-DD HH:MM:SS",
            Self::YmdDashT => "YYYY-MM-DDTHH:MM:SS",
            Self::DmyDotSpace => "DD.MM.YYYY HH:MM:SS (European)",
            Self::DmySlashSpace => "DD/MM/YYYY HH:MM:SS (European)",
            Self::MdySlashSpace => "MM/DD/YYYY HH:MM:SS (US)",
        }
    }

    /// Whether this layout matches the canonical ISO display
    /// (`YYYY-MM-DD HH:MM:SS`). The `T` separator counts as different
    /// because the displayed cell uses a space.
    pub fn is_canonical(self) -> bool {
        matches!(self, Self::YmdDashSpace)
    }

    /// Try to parse the value under this layout, allowing `HH:MM`, `HH:MM:SS`,
    /// and `HH:MM:SS.fff`. Returns the canonical ISO form
    /// `YYYY-MM-DD HH:MM:SS[.fff]` on success.
    pub fn parse(self, s: &str) -> Option<String> {
        let trimmed = s.trim();
        for fmt in self.candidate_formats() {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
                return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
            }
        }
        None
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
}

/// Inspect a column of strings and report what date/datetime layout — if any
/// — every non-null value matches.
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
        0 => InferOutcome::Skip,
        1 => InferOutcome::PromotedDateTime(dt_passing[0]),
        _ => InferOutcome::AmbiguousDateTime {
            candidates: dt_passing,
            samples: sample_values(&non_null),
        },
    }
}

fn sample_values(non_null: &[&str]) -> Vec<String> {
    non_null.iter().take(5).map(|s| s.to_string()).collect()
}

/// Apply a single date layout to every value in `col_idx`, rewriting
/// `CellValue::String` cells in-place to `CellValue::Date` and updating the
/// column's data_type. Cells that fail to parse become `CellValue::Null` —
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
/// columns are skipped — readers that produce typed dates (Parquet, Arrow,
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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;
    use std::collections::HashMap;

    fn table(values: Vec<&str>) -> DataTable {
        let rows: Vec<Vec<CellValue>> = values
            .into_iter()
            .map(|s| {
                vec![if s.is_empty() {
                    CellValue::Null
                } else {
                    CellValue::String(s.to_string())
                }]
            })
            .collect();
        DataTable {
            columns: vec![ColumnInfo {
                name: "d".to_string(),
                data_type: "Utf8".to_string(),
            }],
            rows,
            edits: HashMap::new(),
            ..DataTable::empty()
        }
    }

    #[test]
    fn iso_dash_promoted() {
        let strings = vec![Some("2024-03-15"), Some("2024-04-01"), None];
        match infer_column(&strings) {
            InferOutcome::PromotedDate(DateLayout::YmdDash) => {}
            other => panic!("expected YmdDash, got {other:?}"),
        }
    }

    #[test]
    fn unambiguous_european_when_first_part_over_12() {
        let strings = vec![Some("15/04/2024"), Some("03/01/2025")];
        match infer_column(&strings) {
            InferOutcome::PromotedDate(DateLayout::DmySlash) => {}
            other => panic!("expected DmySlash, got {other:?}"),
        }
    }

    #[test]
    fn unambiguous_us_when_second_part_over_12() {
        let strings = vec![Some("04/15/2024"), Some("12/31/2025")];
        match infer_column(&strings) {
            InferOutcome::PromotedDate(DateLayout::MdySlash) => {}
            other => panic!("expected MdySlash, got {other:?}"),
        }
    }

    #[test]
    fn ambiguous_when_all_components_below_13() {
        let strings = vec![Some("02/03/2024"), Some("04/05/2025")];
        match infer_column(&strings) {
            InferOutcome::AmbiguousDate { candidates, .. } => {
                assert!(candidates.contains(&DateLayout::DmySlash));
                assert!(candidates.contains(&DateLayout::MdySlash));
            }
            other => panic!("expected AmbiguousDate, got {other:?}"),
        }
    }

    #[test]
    fn dot_separated_european() {
        let strings = vec![Some("15.04.2024"), Some("01.12.2025")];
        match infer_column(&strings) {
            InferOutcome::PromotedDate(DateLayout::DmyDot) => {}
            other => panic!("expected DmyDot, got {other:?}"),
        }
    }

    #[test]
    fn mixed_strings_skip() {
        let strings = vec![Some("hello"), Some("2024-03-15")];
        assert!(matches!(infer_column(&strings), InferOutcome::Skip));
    }

    #[test]
    fn null_only_skip() {
        let strings: Vec<Option<&str>> = vec![None, None];
        assert!(matches!(infer_column(&strings), InferOutcome::Skip));
    }

    #[test]
    fn datetime_iso_t() {
        let strings = vec![Some("2024-03-15T13:45:00"), Some("2024-04-01T08:00:00")];
        match infer_column(&strings) {
            InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashT) => {}
            other => panic!("expected YmdDashT, got {other:?}"),
        }
    }

    #[test]
    fn apply_promotes_cells() {
        let mut t = table(vec!["15.04.2024", "01.12.2025", ""]);
        apply_date(&mut t, 0, DateLayout::DmyDot);
        assert_eq!(t.columns[0].data_type, "Date32");
        match t.rows[0][0] {
            CellValue::Date(ref s) => assert_eq!(s, "2024-04-15"),
            ref other => panic!("expected Date, got {other:?}"),
        }
        assert!(matches!(t.rows[2][0], CellValue::Null));
    }

    #[test]
    fn apply_datetime_promotes_cells() {
        let strings = vec![Some("2024-03-15T13:45:00"), Some("2024-04-01T08:00:00")];
        match infer_column(&strings) {
            InferOutcome::PromotedDateTime(layout) => {
                let mut t = table(vec!["2024-03-15T13:45:00", "2024-04-01T08:00:00"]);
                apply_datetime(&mut t, 0, layout);
                assert_eq!(t.columns[0].data_type, "Timestamp(Microsecond, None)");
                match t.rows[0][0] {
                    CellValue::DateTime(ref s) => {
                        assert_eq!(s, "2024-03-15 13:45:00");
                    }
                    ref other => panic!("expected DateTime, got {other:?}"),
                }
            }
            other => panic!("expected PromotedDateTime, got {other:?}"),
        }
    }
}
