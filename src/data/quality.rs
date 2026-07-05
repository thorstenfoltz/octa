//! Data-quality report card: a per-column scorecard that folds together the
//! existing analysis engines (null rate, distinct ratio, outliers, PII, type
//! consistency) into one table plus an overall score.
//!
//! Pure and testable: [`build_quality_report`] takes a `DataTable` and returns a
//! new `DataTable` (one row per source column, snake_case machine headers) plus
//! an `overall_score`. The GUI wraps this in a detached tab with per-column
//! header tooltips, mirroring the Summary tab.

use crate::data::outliers::{OutlierMethod, detect_outliers};
use crate::data::pii::scan_pii;
use crate::data::{CellValue, ColumnInfo, DataTable};
use std::collections::HashSet;

const MAX_EXACT_INT_F64: f64 = 9_007_199_254_740_992.0; // 2^53
const PII_SAMPLE_ROWS: usize = 200;

/// The result of building a quality report: the report table plus the overall
/// score (mean of the per-column scores, 0-100).
pub struct QualityReport {
    pub table: DataTable,
    pub overall_score: f64,
}

/// Machine-friendly snake_case column ids for the report, in output order.
/// Kept in sync with [`quality_column_hint_keys`] (same order).
pub fn quality_column_ids() -> &'static [&'static str] {
    &[
        "column_name",
        "data_type",
        "null_percentage",
        "distinct_ratio",
        "outlier_count",
        "pii_flag",
        "pii_kind",
        "type_consistency",
        "score",
    ]
}

/// i18n hint keys (under `[quality]`) for each report column, in the same order
/// as [`quality_column_ids`]. The GUI sets these as header tooltips.
pub fn quality_column_hint_keys() -> &'static [&'static str] {
    &[
        "quality.hint_column_name",
        "quality.hint_data_type",
        "quality.hint_null_percentage",
        "quality.hint_distinct_ratio",
        "quality.hint_outlier_count",
        "quality.hint_pii_flag",
        "quality.hint_pii_kind",
        "quality.hint_type_consistency",
        "quality.hint_score",
    ]
}

fn is_numeric_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("int") || t.contains("float") || t.contains("double") || t.contains("decimal")
}

fn is_date_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp") || t.contains("time")
}

fn is_bool_type(data_type: &str) -> bool {
    data_type.to_ascii_lowercase().contains("bool")
}

/// Whether a single cell is consistent with the declared column type family.
/// Nulls are handled by the caller (excluded from the denominator).
fn cell_matches_type(cell: &CellValue, data_type: &str) -> bool {
    if is_numeric_type(data_type) {
        match cell {
            CellValue::Int(_) | CellValue::Float(_) => true,
            CellValue::String(s) => s
                .trim()
                .parse::<f64>()
                .map(|f| f.is_finite())
                .unwrap_or(false),
            _ => false,
        }
    } else if is_bool_type(data_type) {
        match cell {
            CellValue::Bool(_) => true,
            CellValue::String(s) => {
                matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "false")
            }
            _ => false,
        }
    } else if is_date_type(data_type) {
        matches!(cell, CellValue::Date(_) | CellValue::DateTime(_))
    } else {
        // Text / unknown: any value is a valid string.
        true
    }
}

fn is_null(cell: &CellValue) -> bool {
    matches!(cell, CellValue::Null)
}

fn num_cell(x: f64) -> CellValue {
    if !x.is_finite() {
        return CellValue::String(String::new());
    }
    if x.fract() == 0.0 && x.abs() < MAX_EXACT_INT_F64 {
        CellValue::Int(x as i64)
    } else {
        CellValue::Float((x * 1_000_000.0).round() / 1_000_000.0)
    }
}

/// Infer a report column's Arrow type name from its cells (`Int64`/`Float64`
/// when all present values are numeric, else `Utf8`). Mirrors `summary.rs`.
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

/// Build the quality report for `table`.
pub fn build_quality_report(table: &DataTable) -> anyhow::Result<QualityReport> {
    let ids = quality_column_ids();
    let row_count = table.row_count();
    let col_count = table.col_count();

    // Run PII detection once for all columns.
    let pii = scan_pii(table, PII_SAMPLE_ROWS);

    // Report rows, each a Vec<CellValue> aligned with `ids`.
    let mut report_rows: Vec<Vec<CellValue>> = Vec::with_capacity(col_count);
    let mut scores: Vec<f64> = Vec::with_capacity(col_count);

    for col in 0..col_count {
        let info = &table.columns[col];

        // Gather this column's cells once.
        let cells: Vec<&CellValue> = (0..row_count).filter_map(|r| table.get(r, col)).collect();

        let null_count = cells.iter().filter(|c| is_null(c)).count();
        let non_null: Vec<&CellValue> = cells.iter().copied().filter(|c| !is_null(c)).collect();
        let non_null_count = non_null.len();

        // null_percentage (whole number).
        let null_frac = if row_count == 0 {
            0.0
        } else {
            null_count as f64 / row_count as f64
        };
        let null_pct = (null_frac * 100.0).round();

        // distinct_ratio = distinct non-null / non-null.
        let distinct = {
            let mut seen: HashSet<String> = HashSet::new();
            for c in &non_null {
                seen.insert(c.to_string());
            }
            seen.len()
        };
        let distinct_ratio = if non_null_count == 0 {
            0.0
        } else {
            distinct as f64 / non_null_count as f64
        };

        // outlier_count (numeric columns only; detect_outliers ignores non-numeric).
        let outlier_count = if is_numeric_type(&info.data_type) {
            let flagged = detect_outliers(table, &[col], OutlierMethod::Iqr, 1.5);
            flagged.iter().filter(|(_, c)| *c == col).count()
        } else {
            0
        };

        // PII.
        let col_pii = pii.iter().find(|p| p.column == col && p.confidence >= 0.5);
        let (pii_flag, pii_kind) = match col_pii {
            Some(p) => ("yes".to_string(), p.kind.id().to_string()),
            None => ("no".to_string(), String::new()),
        };

        // type_consistency = fraction of non-null cells matching the declared type.
        let type_consistency = if non_null_count == 0 {
            1.0
        } else {
            let ok = non_null
                .iter()
                .filter(|c| cell_matches_type(c, &info.data_type))
                .count();
            ok as f64 / non_null_count as f64
        };

        // score (0-100): weighted completeness + uniqueness + type consistency,
        // minus an outlier penalty capped at 10.
        const W_NULL: f64 = 0.4;
        const W_DUP: f64 = 0.2;
        const W_TYPE: f64 = 0.4;
        let base = 100.0
            * (W_NULL * (1.0 - null_frac) + W_DUP * distinct_ratio + W_TYPE * type_consistency);
        let outlier_penalty = if row_count == 0 {
            0.0
        } else {
            (100.0 * outlier_count as f64 / row_count as f64).min(10.0)
        };
        let score = (base - outlier_penalty).clamp(0.0, 100.0).round();
        scores.push(score);

        report_rows.push(vec![
            CellValue::String(info.name.clone()),
            CellValue::String(info.data_type.clone()),
            num_cell(null_pct),
            num_cell(distinct_ratio),
            CellValue::Int(outlier_count as i64),
            CellValue::String(pii_flag),
            CellValue::String(pii_kind),
            num_cell(type_consistency),
            num_cell(score),
        ]);
    }

    // Build columns, then infer each report column's type from its cells so
    // numeric columns render through the numeric path (mirrors summary.rs).
    let mut columns: Vec<ColumnInfo> = ids
        .iter()
        .map(|id| ColumnInfo {
            name: (*id).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    for (ci, col) in columns.iter_mut().enumerate() {
        // column_name / data_type / pii_* stay Utf8.
        if matches!(
            ids[ci],
            "column_name" | "data_type" | "pii_flag" | "pii_kind"
        ) {
            continue;
        }
        col.data_type = infer_column_type(report_rows.iter().map(|r| r[ci].clone()));
    }

    let overall_score = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    };

    let out = DataTable {
        columns,
        rows: report_rows,
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
    };

    Ok(QualityReport {
        table: out,
        overall_score,
    })
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
