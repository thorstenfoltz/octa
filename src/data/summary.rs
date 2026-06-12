//! Localized, configurable per-column Summary statistics.
//!
//! The Summary tab (Analyse -> Summary) shows one row per source column with
//! a chosen set of descriptive statistics. The heavy lifting is a single
//! DuckDB `SUMMARIZE data` pass (min / max / approx-unique / avg / std /
//! quartiles / count / null-percentage); the few extra figures we surface
//! (null count, distinct ratio, total rows) are derived from that pass plus
//! the snapshot's own row count.
//!
//! Output column headers are stable `snake_case` identifiers
//! ([`SummaryStat::column_id`]) so the table is easy to reuse; the localized
//! label and description ([`SummaryStat::i18n_key`] / [`SummaryStat::hint_key`])
//! surface as the header's hover tooltip and the Settings checkboxes. Which
//! statistics appear is driven by the user's Settings (the `enabled` list).
//! [`SummaryStat::ColumnName`] and [`SummaryStat::Type`] are always shown so a
//! row is never anonymous. Modelled one-variant-per-statistic so adding a
//! statistic is a drop-in (see `feedback_modular_features`).

use crate::data::{CellValue, ColumnInfo, DataTable};
use strum::{EnumIter, IntoEnumIterator};

/// One descriptive statistic shown as a column in the Summary tab.
///
/// The variant order here is the column order in the output table.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, serde::Serialize, serde::Deserialize,
)]
pub enum SummaryStat {
    /// Name of the source column (always shown).
    ColumnName,
    /// Declared data type of the source column (always shown).
    Type,
    /// Smallest value.
    Min,
    /// Largest value.
    Max,
    /// Sum of the numeric values (numeric columns only).
    Sum,
    /// Arithmetic mean (numeric columns only).
    Mean,
    /// Median / 50th percentile (numeric columns only).
    Median,
    /// Standard deviation (numeric columns only).
    Std,
    /// Spread: largest minus smallest (numeric columns only).
    Range,
    /// Interquartile range: 75th minus 25th percentile (numeric columns only).
    Iqr,
    /// 25th percentile (numeric columns only).
    Q25,
    /// 75th percentile (numeric columns only).
    Q75,
    /// Most frequent (modal) value.
    Mode,
    /// How many times the most frequent value occurs.
    ModeCount,
    /// Count of non-null (present) values.
    NotNullCount,
    /// Count of null (missing) values.
    NullCount,
    /// Percentage of values that are null.
    NullPercent,
    /// Exact count of distinct values.
    UniqueCount,
    /// Distinct values divided by total rows (0..1).
    DistinctRatio,
    /// Shortest text length (characters) over the column's values.
    TextLenMin,
    /// Longest text length (characters) over the column's values.
    TextLenMax,
    /// Total number of rows in the table (same on every row).
    TotalRows,
}

impl SummaryStat {
    /// Every statistic, in display order.
    pub fn all() -> Vec<SummaryStat> {
        SummaryStat::iter().collect()
    }

    /// Statistics shown by default (the full set).
    pub fn default_enabled() -> Vec<SummaryStat> {
        SummaryStat::all()
    }

    /// Whether this statistic is always shown regardless of Settings.
    /// Column name and type are mandatory so a row is never anonymous.
    pub fn is_mandatory(self) -> bool {
        matches!(self, SummaryStat::ColumnName | SummaryStat::Type)
    }

    /// Stable, machine-friendly column identifier used as the Summary table's
    /// header. Lowercase, underscores only, no spaces, never localized, so the
    /// table can be reused (saved / queried / pasted) without renaming. The
    /// localized friendly name and description stay reachable as the header's
    /// hover tooltip (see [`Self::hint_key`]).
    pub fn column_id(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "column_name",
            SummaryStat::Type => "type",
            SummaryStat::Min => "min",
            SummaryStat::Max => "max",
            SummaryStat::Sum => "sum",
            SummaryStat::Mean => "mean",
            SummaryStat::Median => "median",
            SummaryStat::Std => "std_dev",
            SummaryStat::Range => "range",
            SummaryStat::Iqr => "iqr",
            SummaryStat::Q25 => "q25",
            SummaryStat::Q75 => "q75",
            SummaryStat::Mode => "mode",
            SummaryStat::ModeCount => "mode_count",
            SummaryStat::NotNullCount => "not_null",
            SummaryStat::NullCount => "null_count",
            SummaryStat::NullPercent => "null_percent",
            SummaryStat::UniqueCount => "unique_count",
            SummaryStat::DistinctRatio => "distinct_ratio",
            SummaryStat::TextLenMin => "text_len_min",
            SummaryStat::TextLenMax => "text_len_max",
            SummaryStat::TotalRows => "total_rows",
        }
    }

    /// i18n key for the column title.
    pub fn i18n_key(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "summary_stat.column_name",
            SummaryStat::Type => "summary_stat.type",
            SummaryStat::Min => "summary_stat.min",
            SummaryStat::Max => "summary_stat.max",
            SummaryStat::Sum => "summary_stat.sum",
            SummaryStat::Mean => "summary_stat.mean",
            SummaryStat::Median => "summary_stat.median",
            SummaryStat::Std => "summary_stat.std",
            SummaryStat::Range => "summary_stat.range",
            SummaryStat::Iqr => "summary_stat.iqr",
            SummaryStat::Q25 => "summary_stat.q25",
            SummaryStat::Q75 => "summary_stat.q75",
            SummaryStat::Mode => "summary_stat.mode",
            SummaryStat::ModeCount => "summary_stat.mode_count",
            SummaryStat::NotNullCount => "summary_stat.not_null_count",
            SummaryStat::NullCount => "summary_stat.null_count",
            SummaryStat::NullPercent => "summary_stat.null_percent",
            SummaryStat::UniqueCount => "summary_stat.unique_count",
            SummaryStat::DistinctRatio => "summary_stat.distinct_ratio",
            SummaryStat::TextLenMin => "summary_stat.text_len_min",
            SummaryStat::TextLenMax => "summary_stat.text_len_max",
            SummaryStat::TotalRows => "summary_stat.total_rows",
        }
    }

    /// i18n key for the hover description (header tooltip + Settings hint).
    pub fn hint_key(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "summary_hint.column_name",
            SummaryStat::Type => "summary_hint.type",
            SummaryStat::Min => "summary_hint.min",
            SummaryStat::Max => "summary_hint.max",
            SummaryStat::Sum => "summary_hint.sum",
            SummaryStat::Mean => "summary_hint.mean",
            SummaryStat::Median => "summary_hint.median",
            SummaryStat::Std => "summary_hint.std",
            SummaryStat::Range => "summary_hint.range",
            SummaryStat::Iqr => "summary_hint.iqr",
            SummaryStat::Q25 => "summary_hint.q25",
            SummaryStat::Q75 => "summary_hint.q75",
            SummaryStat::Mode => "summary_hint.mode",
            SummaryStat::ModeCount => "summary_hint.mode_count",
            SummaryStat::NotNullCount => "summary_hint.not_null_count",
            SummaryStat::NullCount => "summary_hint.null_count",
            SummaryStat::NullPercent => "summary_hint.null_percent",
            SummaryStat::UniqueCount => "summary_hint.unique_count",
            SummaryStat::DistinctRatio => "summary_hint.distinct_ratio",
            SummaryStat::TextLenMin => "summary_hint.text_len_min",
            SummaryStat::TextLenMax => "summary_hint.text_len_max",
            SummaryStat::TotalRows => "summary_hint.total_rows",
        }
    }
}

/// Which `SUMMARIZE` column each direct-mapped statistic reads from.
/// Derived statistics (NullCount, DistinctRatio, TotalRows) are computed
/// separately and have no entry here.
fn summarize_field(stat: SummaryStat) -> Option<&'static str> {
    match stat {
        SummaryStat::ColumnName => Some("column_name"),
        SummaryStat::Type => Some("column_type"),
        SummaryStat::Min => Some("min"),
        SummaryStat::Max => Some("max"),
        SummaryStat::Mean => Some("avg"),
        SummaryStat::Median => Some("q50"),
        SummaryStat::Std => Some("std"),
        SummaryStat::Q25 => Some("q25"),
        SummaryStat::Q75 => Some("q75"),
        SummaryStat::NullPercent => Some("null_percentage"),
        // Derived from other SUMMARIZE fields (no own column): Range = max-min,
        // Iqr = q75-q25.
        SummaryStat::Range | SummaryStat::Iqr => None,
        // Computed by extra passes: Sum/TextLenMin/TextLenMax via one aggregate
        // query; Mode/ModeCount via a per-column GROUP BY. NotNull/Null from
        // `count` + `null_percentage` (SUMMARIZE's `count` is the *total* row
        // count, not the non-null count); Unique and DistinctRatio from an exact
        // `COUNT(DISTINCT)` pass (SUMMARIZE's `approx_unique` is a HyperLogLog
        // estimate that can exceed the row count); TotalRows from the snapshot.
        SummaryStat::Sum
        | SummaryStat::TextLenMin
        | SummaryStat::TextLenMax
        | SummaryStat::Mode
        | SummaryStat::ModeCount
        | SummaryStat::UniqueCount
        | SummaryStat::NotNullCount
        | SummaryStat::NullCount
        | SummaryStat::DistinctRatio
        | SummaryStat::TotalRows => None,
    }
}

/// Quote a column name as a DuckDB identifier (double quotes, internal quotes
/// doubled) so names with spaces or punctuation survive in a SELECT.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Largest f64 that represents every integer exactly (2^53). Above it a whole
/// number can't be round-tripped through `i64`, so we keep it as a `Float`.
const MAX_EXACT_INT_F64: f64 = 9_007_199_254_740_992.0;

/// Convert a computed numeric statistic (sum / range / iqr / ratio) into its
/// tightest cell type. Rounds to 6 decimals first to kill float noise (e.g.
/// `0.1 + 0.2`), then stores an exact whole number as `Int` and anything else
/// as `Float`, so the table view's numeric display path groups and right-aligns
/// it. Non-finite values become a blank cell.
fn num_cell(x: f64) -> CellValue {
    if !x.is_finite() {
        return CellValue::String(String::new());
    }
    let rounded = (x * 1_000_000.0).round() / 1_000_000.0;
    if rounded.fract() == 0.0 && rounded.abs() < MAX_EXACT_INT_F64 {
        CellValue::Int(rounded as i64)
    } else {
        CellValue::Float(rounded)
    }
}

/// Parse a `SUMMARIZE` / text value into its tightest cell type: an integer
/// becomes `Int`, a finite decimal `Float`, an empty value a blank string, and
/// anything else (a lexicographic text min/max, a category mode) stays text.
/// This is what lets a numeric column's min/max/mode group like a number while
/// a text column's stay verbatim.
fn typed_cell(s: &str) -> CellValue {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return CellValue::String(String::new());
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return CellValue::Int(n);
    }
    if let Ok(f) = trimmed.parse::<f64>()
        && f.is_finite()
    {
        return CellValue::Float(f);
    }
    CellValue::String(s.to_string())
}

/// Infer a column's Arrow type name from the cell variants it holds: `Int64`
/// when every present value is an integer, `Float64` when they're all numeric
/// with at least one decimal, `Utf8` otherwise. Empty-string / null cells are
/// ignored (a numeric column keeps its type even with blank rows).
fn infer_column_type(cells: impl Iterator<Item = CellValue>) -> String {
    let mut saw_value = false;
    let mut saw_float = false;
    for cell in cells {
        match cell {
            CellValue::Int(_) => saw_value = true,
            CellValue::Float(_) => {
                saw_value = true;
                saw_float = true;
            }
            CellValue::Null => {}
            CellValue::String(s) if s.is_empty() => {}
            // Any real text (text min/max, mode category, type name) -> Utf8.
            _ => return "Utf8".to_string(),
        }
    }
    if !saw_value {
        "Utf8".to_string()
    } else if saw_float {
        "Float64".to_string()
    } else {
        "Int64".to_string()
    }
}

/// Exact distinct-value count per source column, positionally aligned with
/// `snap.columns`. One `COUNT(DISTINCT col)` query; `None` per column if it
/// can't be read. `COUNT(DISTINCT)` ignores nulls, so the result never exceeds
/// the row count (unlike SUMMARIZE's approximate `approx_unique`).
fn exact_distinct_counts(snap: &DataTable) -> Vec<Option<i64>> {
    let mut out = vec![None; snap.columns.len()];
    if snap.columns.is_empty() {
        return out;
    }
    let selects: Vec<String> = snap
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("COUNT(DISTINCT {}) AS d{i}", quote_ident(&c.name)))
        .collect();
    let query = format!("SELECT {} FROM data", selects.join(", "));
    if let Ok(outcome) = crate::sql::run_query(snap, &query)
        && outcome.table.row_count() >= 1
    {
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = outcome
                .table
                .get(0, i)
                .and_then(|v| v.to_string().parse::<i64>().ok());
        }
    }
    out
}

/// Per-column extra aggregates that `SUMMARIZE` does not provide: numeric sum
/// and text-length extremes. Positionally aligned with `snap.columns`.
#[derive(Clone, Default)]
struct ExtraAgg {
    sum: Option<f64>,
    text_len_min: Option<i64>,
    text_len_max: Option<i64>,
}

/// One pass computing [`ExtraAgg`] for every column. `SUM(TRY_CAST(.. AS
/// DOUBLE))` yields `NULL` (rendered blank) on non-numeric columns instead of
/// erroring; text length is measured over `CAST(.. AS VARCHAR)` so it works for
/// any type. Returns all-default on query failure.
fn extra_aggregates(snap: &DataTable) -> Vec<ExtraAgg> {
    let mut out = vec![ExtraAgg::default(); snap.columns.len()];
    if snap.columns.is_empty() {
        return out;
    }
    let selects: Vec<String> = snap
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let q = quote_ident(&c.name);
            format!(
                "SUM(TRY_CAST({q} AS DOUBLE)) AS s{i}, \
                 MIN(length(CAST({q} AS VARCHAR))) AS lmin{i}, \
                 MAX(length(CAST({q} AS VARCHAR))) AS lmax{i}"
            )
        })
        .collect();
    let query = format!("SELECT {} FROM data", selects.join(", "));
    if let Ok(outcome) = crate::sql::run_query(snap, &query)
        && outcome.table.row_count() >= 1
    {
        let parse_i64 = |c: usize| {
            outcome
                .table
                .get(0, c)
                .and_then(|v| v.to_string().parse::<i64>().ok())
        };
        let parse_f64 = |c: usize| {
            outcome
                .table
                .get(0, c)
                .and_then(|v| v.to_string().parse::<f64>().ok())
        };
        for (i, slot) in out.iter_mut().enumerate() {
            // Three result columns per source column, in select order.
            slot.sum = parse_f64(i * 3);
            slot.text_len_min = parse_i64(i * 3 + 1);
            slot.text_len_max = parse_i64(i * 3 + 2);
        }
    }
    out
}

/// Most frequent (modal) value and its count per column, positionally aligned
/// with `snap.columns`. One small `GROUP BY ... ORDER BY count DESC LIMIT 1`
/// query per column; `(None, None)` per column on failure or an all-null
/// column. Nulls are excluded from the mode.
fn mode_values(snap: &DataTable) -> Vec<(Option<String>, Option<i64>)> {
    let mut out = vec![(None, None); snap.columns.len()];
    for (i, c) in snap.columns.iter().enumerate() {
        let q = quote_ident(&c.name);
        // Tie-break on the value so the result is deterministic.
        let query = format!(
            "SELECT CAST({q} AS VARCHAR) AS v, COUNT(*) AS n FROM data \
             WHERE {q} IS NOT NULL GROUP BY v ORDER BY n DESC, v LIMIT 1"
        );
        if let Ok(outcome) = crate::sql::run_query(snap, &query)
            && outcome.table.row_count() >= 1
        {
            let value = outcome.table.get(0, 0).map(|v| v.to_string());
            let count = outcome
                .table
                .get(0, 1)
                .and_then(|v| v.to_string().parse::<i64>().ok());
            out[i] = (value, count);
        }
    }
    out
}

/// The statistics actually rendered, in canonical (variant) order: every
/// mandatory stat plus any the caller enabled. Order is independent of the
/// order of `enabled`.
pub fn active_stats(enabled: &[SummaryStat]) -> Vec<SummaryStat> {
    SummaryStat::iter()
        .filter(|s| s.is_mandatory() || enabled.contains(s))
        .collect()
}

/// Build the Summary table: one row per source column, one column per active
/// statistic, with localized column titles. Pure apart from the i18n lookup
/// for titles. Runs a single `SUMMARIZE` over `snap`.
pub fn build_summary_table(snap: &DataTable, enabled: &[SummaryStat]) -> anyhow::Result<DataTable> {
    let total_rows = snap.row_count();
    let outcome = crate::sql::run_query(snap, "SUMMARIZE data")?;
    let summ = outcome.table;

    let field_idx = |name: &str| summ.columns.iter().position(|c| c.name == name);
    let active = active_stats(enabled);

    // Machine-friendly, never-localized column ids so the table is reusable.
    // Types start as Utf8 and are refined to Int64 / Float64 once the cells
    // exist (see the inference pass below) so numeric statistics render as real
    // numbers (grouped, right-aligned) rather than plain text.
    let mut columns: Vec<ColumnInfo> = active
        .iter()
        .map(|s| ColumnInfo {
            name: s.column_id().to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let cell_str = |row: usize, name: &str| -> Option<String> {
        let ci = field_idx(name)?;
        summ.get(row, ci).map(|v| v.to_string())
    };
    // Parse a SUMMARIZE numeric field (min/max/q25/q75) for a row, for the
    // derived Range / Iqr stats. Blank when the field is missing or non-numeric.
    let cell_f64 = |row: usize, name: &str| -> Option<f64> {
        cell_str(row, name).and_then(|s| s.parse::<f64>().ok())
    };

    // Exact distinct counts only when a stat that needs them is shown.
    let need_unique = active
        .iter()
        .any(|s| matches!(s, SummaryStat::UniqueCount | SummaryStat::DistinctRatio));
    let exact_unique = if need_unique {
        exact_distinct_counts(snap)
    } else {
        Vec::new()
    };

    // Sum / text-length extremes share one aggregate pass; only run it when one
    // of those stats is on.
    let need_extra = active.iter().any(|s| {
        matches!(
            s,
            SummaryStat::Sum | SummaryStat::TextLenMin | SummaryStat::TextLenMax
        )
    });
    let extra = if need_extra {
        extra_aggregates(snap)
    } else {
        Vec::new()
    };

    // Mode + its count share a per-column GROUP BY pass; only when on.
    let need_mode = active
        .iter()
        .any(|s| matches!(s, SummaryStat::Mode | SummaryStat::ModeCount));
    let modes = if need_mode {
        mode_values(snap)
    } else {
        Vec::new()
    };

    let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(summ.row_count());
    for r in 0..summ.row_count() {
        let unique: Option<i64> = exact_unique.get(r).copied().flatten();
        let agg = extra.get(r).cloned().unwrap_or_default();
        let (mode_value, mode_count) = modes.get(r).cloned().unwrap_or((None, None));
        // SUMMARIZE's `count` is the total row count; the non-null and null
        // counts come from it and `null_percentage`.
        let null_pct: Option<f64> = cell_str(r, "null_percentage").and_then(|s| s.parse().ok());
        let null_count = null_pct.map(|p| (total_rows as f64 * p / 100.0).round());
        let not_null = null_count.map(|n| (total_rows as f64 - n).max(0.0));

        let blank = || CellValue::String(String::new());
        let row: Vec<CellValue> = active
            .iter()
            .map(|stat| match stat {
                SummaryStat::Sum => agg.sum.map(num_cell).unwrap_or_else(blank),
                SummaryStat::Range => match (cell_f64(r, "min"), cell_f64(r, "max")) {
                    (Some(lo), Some(hi)) => num_cell(hi - lo),
                    _ => blank(),
                },
                SummaryStat::Iqr => match (cell_f64(r, "q25"), cell_f64(r, "q75")) {
                    (Some(lo), Some(hi)) => num_cell(hi - lo),
                    _ => blank(),
                },
                SummaryStat::Mode => mode_value.as_deref().map(typed_cell).unwrap_or_else(blank),
                SummaryStat::ModeCount => mode_count.map(CellValue::Int).unwrap_or_else(blank),
                SummaryStat::TextLenMin => {
                    agg.text_len_min.map(CellValue::Int).unwrap_or_else(blank)
                }
                SummaryStat::TextLenMax => {
                    agg.text_len_max.map(CellValue::Int).unwrap_or_else(blank)
                }
                SummaryStat::NotNullCount => not_null
                    .map(|n| CellValue::Int(n as i64))
                    .unwrap_or_else(blank),
                SummaryStat::NullCount => null_count
                    .map(|n| CellValue::Int(n as i64))
                    .unwrap_or_else(blank),
                SummaryStat::UniqueCount => unique.map(CellValue::Int).unwrap_or_else(blank),
                SummaryStat::DistinctRatio => match unique {
                    Some(u) if total_rows > 0 => num_cell(u as f64 / total_rows as f64),
                    _ => blank(),
                },
                SummaryStat::TotalRows => CellValue::Int(total_rows as i64),
                // Column name and type are always plain text, even if a column
                // happens to be named numerically.
                SummaryStat::ColumnName | SummaryStat::Type => CellValue::String(
                    summarize_field(*stat)
                        .and_then(|f| cell_str(r, f))
                        .unwrap_or_default(),
                ),
                // Min / Max / Mean / Median / Std / Q25 / Q75 / NullPercent come
                // from SUMMARIZE as strings; type them so numeric columns group.
                other => summarize_field(*other)
                    .and_then(|f| cell_str(r, f))
                    .map(|s| typed_cell(&s))
                    .unwrap_or_else(blank),
            })
            .collect();
        rows.push(row);
    }

    // Refine each column's type from the cells it ended up with, so numeric
    // statistics are real Int64 / Float64 columns (grouped + right-aligned by
    // the table view) while mixed or textual ones stay Utf8.
    for (ci, col) in columns.iter_mut().enumerate() {
        col.data_type = infer_column_type(rows.iter().map(|row| row[ci].clone()));
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
