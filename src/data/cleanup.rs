//! Clean-up suggestions: what is wrong with this table, as a ranked list of
//! concrete fixes.
//!
//! This module adds **no detection logic of its own**. Every suggestion is a
//! translation of something an existing engine already computes:
//! [`scan_pii`](crate::data::pii::scan_pii),
//! [`detect_outliers`](crate::data::outliers::detect_outliers),
//! [`find_duplicate_rows`](crate::data::duplicates::find_duplicate_rows), the
//! whitespace check `trim` performs, and
//! [`DataTable::can_convert_column`]. Keep it that way: a second detector here
//! would drift from the one the Apply button actually runs.
//!
//! Pure and UI-free, so the GUI panel (`src/app/cleanup_panel.rs`) can run it on
//! a worker thread against a table snapshot.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::duplicates::find_duplicate_rows;
use crate::data::outliers::{OutlierMethod, detect_outliers};
use crate::data::pii::{PiiKind, scan_pii};
use crate::data::{CellValue, DataTable};

/// How prominently a suggestion is presented. Ordering matters: results sort
/// by severity descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
}

/// What is wrong, and therefore which existing operation fixes it.
#[derive(Debug, Clone, PartialEq)]
pub enum CleanupKind {
    /// Leading or trailing whitespace in a string column. Fixed directly.
    TrimWhitespace,
    /// A text column whose values all parse as `target`. Fixed directly.
    TypeMismatch { target: String },
    /// Whole-row duplicates. Fixed directly.
    DuplicateRows,
    /// A column with a meaningful share of nulls. Opens the impute dialog.
    MissingValues,
    /// Numeric outliers. Opens the outliers dialog.
    Outliers,
    /// A column that looks like personal data. Opens the anonymise dialog.
    PersonalData { kind: PiiKind },
    /// A column that is entirely null. Fixed directly (drop the column).
    EmptyColumn,
    /// Column titles that are not tidy identifiers. Fixed directly.
    UntidyHeaders,
    /// Text decoded with the wrong character set and stored as valid UTF-8,
    /// so `Ã¤` appears where `ä` belongs. Fixed directly.
    Mojibake,
}

/// One suggested fix.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub kind: CleanupKind,
    /// The column this concerns, or `None` for a whole-table suggestion.
    pub column: Option<usize>,
    /// How many rows or cells the fix would touch. Used for ranking and shown
    /// to the user, so it must be a real count, never an estimate.
    pub affected: usize,
    pub severity: Severity,
    /// Pre-formatted numeric detail (a percentage, a count). The panel supplies
    /// the localized sentence around it; this is not a translatable string.
    pub detail: String,
    /// A few real offending values, so the user can see what the suggestion is
    /// actually talking about before applying anything. Empty for kinds where
    /// a value example says nothing (a missing value has no value to show, an
    /// empty column has nothing in it); the panel hides its Show button then.
    ///
    /// Deterministic: same table in, same examples out, so a rescan of
    /// unchanged data does not shuffle them.
    pub examples: Vec<String>,
}

/// Thresholds keeping the panel free of noise, and a row cap keeping a scan of
/// a five-million-row table finite.
#[derive(Debug, Clone, Copy)]
pub struct CleanupLimits {
    /// Rows examined.
    pub max_rows: usize,
    /// Fraction of non-empty values that must parse as the target type before
    /// a cast is suggested.
    pub min_type_mismatch_ratio: f64,
    /// Null fraction below which a column is not worth reporting.
    pub min_null_ratio: f64,
    /// PII confidence below which a column is not reported.
    pub min_pii_confidence: f64,
}

impl Default for CleanupLimits {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            min_type_mismatch_ratio: 0.90,
            min_null_ratio: 0.05,
            min_pii_confidence: 0.5,
        }
    }
}

/// Rows sampled for PII detection.
const PII_SAMPLE_ROWS: usize = 500;

/// Interquartile-range multiplier for the outlier pass. The value the outliers
/// dialog defaults to, so the suggested count matches what the dialog shows.
const OUTLIER_K: f64 = 1.5;

/// How many offending values each suggestion carries, and how long each may be.
const EXAMPLE_LIMIT: usize = 3;
const EXAMPLE_MAX_CHARS: usize = 60;

/// Clamp one example to `EXAMPLE_MAX_CHARS`, by characters rather than bytes so
/// a multi-byte value is never cut mid-character.
fn short(s: &str) -> String {
    if s.chars().count() <= EXAMPLE_MAX_CHARS {
        return s.to_string();
    }
    let head: String = s.chars().take(EXAMPLE_MAX_CHARS).collect();
    format!("{head}...")
}

/// One row rendered as `a | b | c`, for the duplicate-rows examples.
fn render_row(table: &DataTable, row: usize, cols: usize) -> String {
    let joined = (0..cols)
        .map(|c| table.get(row, c).map(|v| v.to_string()).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" | ");
    short(&joined)
}

fn is_null(v: &CellValue) -> bool {
    matches!(v, CellValue::Null) || matches!(v, CellValue::String(s) if s.is_empty())
}

/// Whether a column title is already a tidy identifier: trimmed, lowercase,
/// and made only of alphanumerics and underscores. The same shape
/// [`crate::data::trim::clean_headers`] produces, so the Apply button has
/// something to do for every title this rejects.
fn header_is_untidy(name: &str) -> bool {
    name.trim() != name
        || name.chars().any(|ch| ch.is_uppercase())
        || name.chars().any(|ch| !ch.is_alphanumeric() && ch != '_')
}

/// Scan `table` and return the fixes worth offering, ranked.
///
/// Checks `cancel` between kinds and returns what it has (or nothing) when it
/// is set. Examines at most `limits.max_rows` rows.
pub fn suggest_cleanups(
    table: &DataTable,
    limits: &CleanupLimits,
    cancel: &AtomicBool,
) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let rows = table.row_count().min(limits.max_rows);
    let cols = table.col_count();
    if rows == 0 || cols == 0 || cancel.load(Ordering::Relaxed) {
        return out;
    }

    // One outlier pass over every column, not one pass per column: the
    // detector walks the whole table each call, so asking it column by column
    // would re-read the table `cols` times.
    let all_cols: Vec<usize> = (0..cols).collect();
    let mut outlier_counts = vec![0_usize; cols];
    let mut outlier_examples: Vec<Vec<String>> = vec![Vec::new(); cols];
    // The detector returns a `HashSet`, whose iteration order varies run to
    // run. Sort by coordinate first so the examples are the same every scan.
    let mut flagged: Vec<(usize, usize)> =
        detect_outliers(table, &all_cols, OutlierMethod::Iqr, OUTLIER_K)
            .into_iter()
            .collect();
    flagged.sort_unstable();
    for (row, col) in flagged {
        if let Some(slot) = outlier_counts.get_mut(col) {
            *slot += 1;
        }
        if let (Some(bucket), Some(value)) = (outlier_examples.get_mut(col), table.get(row, col))
            && bucket.len() < EXAMPLE_LIMIT
        {
            bucket.push(short(&value.to_string()));
        }
    }

    // --- Per-column checks -------------------------------------------------
    for (col, &outliers) in outlier_counts.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return out;
        }
        let cells: Vec<&CellValue> = (0..rows).filter_map(|r| table.get(r, col)).collect();
        if cells.is_empty() {
            continue;
        }
        let null_count = cells.iter().filter(|c| is_null(c)).count();
        let non_null: Vec<&CellValue> = cells.iter().copied().filter(|c| !is_null(c)).collect();

        // Fully empty column. Supersedes the missing-values suggestion, since
        // there is nothing to impute from.
        if non_null.is_empty() {
            out.push(Suggestion {
                kind: CleanupKind::EmptyColumn,
                column: Some(col),
                affected: cells.len(),
                severity: Severity::Medium,
                detail: String::new(),
                // Nothing in the column, so nothing to show.
                examples: Vec::new(),
            });
            continue;
        }

        // Whitespace. Examples are quoted so the offending spaces are visible;
        // an unquoted `Tokyo ` would look identical to a clean value.
        let mut ws = 0;
        let mut ws_examples = Vec::new();
        for cell in &non_null {
            if let CellValue::String(s) = cell
                && s.trim() != s.as_str()
            {
                ws += 1;
                if ws_examples.len() < EXAMPLE_LIMIT {
                    ws_examples.push(format!("\"{}\"", short(s)));
                }
            }
        }
        if ws > 0 {
            out.push(Suggestion {
                kind: CleanupKind::TrimWhitespace,
                column: Some(col),
                affected: ws,
                severity: Severity::High,
                detail: ws.to_string(),
                examples: ws_examples,
            });
        }

        // Garbled characters. Delegated to `mojibake`, which only reports a
        // cell when it can prove the reversal, so a suggestion here can never
        // propose a repair that would then mangle the value. High severity:
        // unlike stray whitespace this is visible corruption of the content.
        let moji = crate::data::mojibake::scan_column(
            &non_null.iter().map(|c| (*c).clone()).collect::<Vec<_>>(),
        );
        if moji.affected > 0 {
            out.push(Suggestion {
                kind: CleanupKind::Mojibake,
                column: Some(col),
                affected: moji.affected,
                severity: Severity::High,
                detail: moji.affected.to_string(),
                // `before -> after` reads the same way UntidyHeaders shows its
                // `old -> new` rename examples.
                examples: moji
                    .examples
                    .iter()
                    .map(|(before, after)| format!("{} -> {}", short(before), short(after)))
                    .collect(),
            });
        }

        // Text column that is really numeric. `can_convert_column` is the same
        // check the cast itself runs, so a suggestion can never propose a cast
        // that would then fail.
        let is_text = table
            .columns
            .get(col)
            .is_some_and(|c| c.data_type.eq_ignore_ascii_case("Utf8"));
        if is_text {
            let parses = non_null
                .iter()
                .filter(|c| c.to_string().trim().parse::<f64>().is_ok())
                .count();
            let ratio = parses as f64 / non_null.len() as f64;
            if ratio >= limits.min_type_mismatch_ratio && table.can_convert_column(col, "Float64") {
                out.push(Suggestion {
                    kind: CleanupKind::TypeMismatch {
                        target: "Float64".to_string(),
                    },
                    column: Some(col),
                    affected: parses,
                    severity: Severity::Medium,
                    detail: format!("{}", (ratio * 100.0).round() as i64),
                    examples: non_null
                        .iter()
                        .take(EXAMPLE_LIMIT)
                        .map(|c| short(&c.to_string()))
                        .collect(),
                });
            }
        }

        // Missing values. No examples: an empty cell has nothing to show.
        let null_ratio = null_count as f64 / cells.len() as f64;
        if null_count > 0 && null_ratio >= limits.min_null_ratio {
            out.push(Suggestion {
                kind: CleanupKind::MissingValues,
                column: Some(col),
                affected: null_count,
                severity: Severity::Medium,
                detail: format!("{}", (null_ratio * 100.0).round() as i64),
                examples: Vec::new(),
            });
        }

        // Outliers, numeric columns only. `detect_outliers` ignores the rest.
        if outliers > 0 {
            out.push(Suggestion {
                kind: CleanupKind::Outliers,
                column: Some(col),
                affected: outliers,
                severity: Severity::Low,
                detail: outliers.to_string(),
                examples: outlier_examples[col].clone(),
            });
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return out;
    }

    // --- Personal data -----------------------------------------------------
    for hit in scan_pii(table, PII_SAMPLE_ROWS) {
        if hit.confidence >= limits.min_pii_confidence {
            out.push(Suggestion {
                kind: CleanupKind::PersonalData { kind: hit.kind },
                column: Some(hit.column),
                affected: rows,
                severity: Severity::High,
                detail: format!("{}", (hit.confidence * 100.0).round() as i64),
                examples: (0..rows)
                    .filter_map(|r| table.get(r, hit.column))
                    .filter(|v| !is_null(v))
                    .take(EXAMPLE_LIMIT)
                    .map(|v| short(&v.to_string()))
                    .collect(),
            });
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return out;
    }

    // --- Whole-table checks ------------------------------------------------
    let duplicate_rows = find_duplicate_rows(table, &all_cols);
    if !duplicate_rows.is_empty() {
        out.push(Suggestion {
            kind: CleanupKind::DuplicateRows,
            column: None,
            affected: duplicate_rows.len(),
            severity: Severity::High,
            detail: duplicate_rows.len().to_string(),
            examples: duplicate_rows
                .iter()
                .take(EXAMPLE_LIMIT)
                .map(|&r| render_row(table, r, cols))
                .collect(),
        });
    }

    // Examples read `Order ID -> order_id`, so the user sees exactly what the
    // titles would become before agreeing to it.
    let planned = crate::data::trim::planned_header_names(table);
    let untidy: Vec<String> = table
        .columns
        .iter()
        .zip(&planned)
        .filter(|(c, _)| header_is_untidy(&c.name))
        .map(|(c, new)| format!("{} -> {}", short(&c.name), short(new)))
        .collect();
    if !untidy.is_empty() {
        out.push(Suggestion {
            kind: CleanupKind::UntidyHeaders,
            column: None,
            affected: untidy.len(),
            severity: Severity::Low,
            detail: untidy.len().to_string(),
            examples: untidy.into_iter().take(EXAMPLE_LIMIT).collect(),
        });
    }

    // Highest severity first, then biggest impact, then column order. Stable,
    // so rescanning unchanged data gives the same list in the same order.
    out.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(b.affected.cmp(&a.affected))
            .then(a.column.cmp(&b.column))
    });
    out
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
