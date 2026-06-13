//! Unit tests for [`summary`](summary). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;

fn sample_table() -> DataTable {
    // 4 rows: one null in `score`, distinct ids.
    let columns = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "score".to_string(),
            data_type: "Float64".to_string(),
        },
    ];
    let rows = vec![
        vec![CellValue::Int(1), CellValue::Float(10.0)],
        vec![CellValue::Int(2), CellValue::Float(20.0)],
        vec![CellValue::Int(3), CellValue::Float(30.0)],
        vec![CellValue::Int(4), CellValue::Null],
    ];
    DataTable {
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
    }
}

#[test]
fn active_stats_always_includes_name_and_type() {
    let active = active_stats(&[SummaryStat::Min]);
    assert_eq!(active[0], SummaryStat::ColumnName);
    assert_eq!(active[1], SummaryStat::Type);
    assert!(active.contains(&SummaryStat::Min));
    assert!(!active.contains(&SummaryStat::Max));
}

#[test]
fn active_stats_preserve_canonical_order() {
    // Pass enabled out of order; output must stay in variant order.
    let active = active_stats(&[SummaryStat::Max, SummaryStat::Min]);
    let min_pos = active.iter().position(|s| *s == SummaryStat::Min).unwrap();
    let max_pos = active.iter().position(|s| *s == SummaryStat::Max).unwrap();
    assert!(min_pos < max_pos);
}

#[test]
fn build_summary_has_one_row_per_column() {
    let t = sample_table();
    let out = build_summary_table(&t, &SummaryStat::default_enabled()).unwrap();
    assert_eq!(out.row_count(), 2); // id + score
    // First two output columns are name + type.
    assert_eq!(
        out.columns.len(),
        active_stats(&SummaryStat::default_enabled()).len()
    );
}

#[test]
fn derived_null_and_total_counts() {
    let t = sample_table();
    let enabled = SummaryStat::default_enabled();
    let out = build_summary_table(&t, &enabled).unwrap();
    let active = active_stats(&enabled);

    let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
    let name_col = col(SummaryStat::ColumnName);
    let null_col = col(SummaryStat::NullCount);
    let not_null_col = col(SummaryStat::NotNullCount);
    let total_col = col(SummaryStat::TotalRows);

    // Find the `score` row (has one null).
    let score_row = (0..out.row_count())
        .find(|&r| out.get(r, name_col).map(|v| v.to_string()) == Some("score".to_string()))
        .unwrap();
    assert_eq!(
        out.get(score_row, null_col).map(|v| v.to_string()),
        Some("1".to_string())
    );
    assert_eq!(
        out.get(score_row, not_null_col).map(|v| v.to_string()),
        Some("3".to_string())
    );
    assert_eq!(
        out.get(score_row, total_col).map(|v| v.to_string()),
        Some("4".to_string())
    );
}

#[test]
fn unique_count_is_exact_and_never_exceeds_rows() {
    // `id` has 4 distinct values; `score` has 3 distinct (one null) over
    // 4 rows. Exact COUNT(DISTINCT) must report these, never more.
    let t = sample_table();
    let enabled = SummaryStat::default_enabled();
    let out = build_summary_table(&t, &enabled).unwrap();
    let active = active_stats(&enabled);
    let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
    let name_col = col(SummaryStat::ColumnName);
    let uniq_col = col(SummaryStat::UniqueCount);
    let ratio_col = col(SummaryStat::DistinctRatio);

    let row_for = |want: &str| {
        (0..out.row_count())
            .find(|&r| out.get(r, name_col).map(|v| v.to_string()) == Some(want.to_string()))
            .unwrap()
    };
    let id_row = row_for("id");
    let score_row = row_for("score");
    assert_eq!(
        out.get(id_row, uniq_col).map(|v| v.to_string()),
        Some("4".to_string())
    );
    assert_eq!(
        out.get(score_row, uniq_col).map(|v| v.to_string()),
        Some("3".to_string())
    );
    // Distinct ratio = unique / total rows, in [0, 1]. Stored as a real Float,
    // so it renders as `0.75` (the table view applies any display formatting).
    assert_eq!(out.get(score_row, ratio_col), Some(&CellValue::Float(0.75)));
    // Counts are real Int cells, the ratio a Float, so the columns type numeric.
    let col_type = |stat: SummaryStat| {
        out.columns[active.iter().position(|s| *s == stat).unwrap()]
            .data_type
            .as_str()
    };
    assert_eq!(col_type(SummaryStat::UniqueCount), "Int64");
    assert_eq!(col_type(SummaryStat::DistinctRatio), "Float64");
    assert_eq!(col_type(SummaryStat::ColumnName), "Utf8");
}

/// Build a one-column string table from the given values (no nulls).
fn string_table(col: &str, values: &[&str]) -> DataTable {
    let columns = vec![ColumnInfo {
        name: col.to_string(),
        data_type: "Utf8".to_string(),
    }];
    let rows = values
        .iter()
        .map(|v| vec![CellValue::String(v.to_string())])
        .collect();
    DataTable {
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
    }
}

fn col_value(out: &DataTable, name_col: usize, want: &str, target_col: usize) -> Option<String> {
    let r = (0..out.row_count())
        .find(|&r| out.get(r, name_col).map(|v| v.to_string()) == Some(want.to_string()))?;
    out.get(r, target_col).map(|v| v.to_string())
}

#[test]
fn sum_and_range_are_exact() {
    let t = sample_table();
    let enabled = SummaryStat::default_enabled();
    let out = build_summary_table(&t, &enabled).unwrap();
    let active = active_stats(&enabled);
    let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
    let name_col = col(SummaryStat::ColumnName);
    let sum_col = col(SummaryStat::Sum);
    let range_col = col(SummaryStat::Range);

    // id = 1+2+3+4 = 10, range 4-1 = 3; score = 10+20+30 = 60, range 30-10 = 20.
    assert_eq!(
        col_value(&out, name_col, "id", sum_col),
        Some("10".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "score", sum_col),
        Some("60".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "id", range_col),
        Some("3".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "score", range_col),
        Some("20".to_string())
    );
}

#[test]
fn mode_and_text_length_are_computed() {
    // red, red, blue -> mode "red" (count 2); lengths 3 (red) .. 4 (blue).
    let t = string_table("colour", &["red", "red", "blue"]);
    let enabled = SummaryStat::default_enabled();
    let out = build_summary_table(&t, &enabled).unwrap();
    let active = active_stats(&enabled);
    let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
    let name_col = col(SummaryStat::ColumnName);

    assert_eq!(
        col_value(&out, name_col, "colour", col(SummaryStat::Mode)),
        Some("red".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "colour", col(SummaryStat::ModeCount)),
        Some("2".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "colour", col(SummaryStat::TextLenMin)),
        Some("3".to_string())
    );
    assert_eq!(
        col_value(&out, name_col, "colour", col(SummaryStat::TextLenMax)),
        Some("4".to_string())
    );
}

#[test]
fn headers_are_snake_case_ids() {
    let t = sample_table();
    let out = build_summary_table(&t, &SummaryStat::default_enabled()).unwrap();
    // Every header is a stable snake_case id, not a localized label.
    assert_eq!(out.columns[0].name, "column_name");
    assert!(out.columns.iter().any(|c| c.name == "not_null"));
    assert!(out.columns.iter().any(|c| c.name == "total_rows"));
    assert!(out.columns.iter().all(|c| {
        !c.name.is_empty()
            && c.name
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    }));
}
