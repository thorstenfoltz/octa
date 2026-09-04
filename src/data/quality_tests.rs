//! Unit tests for [`quality`](quality). Split out and included via `#[path]`
//! so it stays an inner `tests` module with access to private items.

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table() -> DataTable {
    // Two columns: an all-present numeric with an outlier, and a text col with a null.
    let columns = vec![
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "email".into(),
            data_type: "Utf8".into(),
        },
    ];
    let rows = vec![
        vec![CellValue::Int(10), CellValue::String("a@b.com".into())],
        vec![CellValue::Int(11), CellValue::String("c@d.com".into())],
        vec![CellValue::Int(12), CellValue::Null],
        vec![CellValue::Int(13), CellValue::String("e@f.com".into())],
        vec![CellValue::Int(9000), CellValue::String("g@h.com".into())],
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
        formulas: std::collections::HashMap::new(),
    }
}

#[test]
fn one_row_per_source_column() {
    let rep = build_quality_report(&table()).unwrap();
    assert_eq!(rep.table.row_count(), 2);
}

#[test]
fn headers_are_snake_case_ids() {
    let rep = build_quality_report(&table()).unwrap();
    let names: Vec<&str> = rep.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"null_percentage"));
    assert!(names.contains(&"score"));
    assert!(names.contains(&"pii_flag"));
}

#[test]
fn null_percentage_reflects_missing_cell() {
    let rep = build_quality_report(&table()).unwrap();
    // email column (report row index 1) has 1/5 null = 20%.
    let col = rep
        .table
        .columns
        .iter()
        .position(|c| c.name == "null_percentage")
        .unwrap();
    let email_row = 1;
    let v = rep.table.get(email_row, col).unwrap();
    assert_eq!(v.to_string(), "20"); // rounded whole percent
}

#[test]
fn overall_score_in_range() {
    let rep = build_quality_report(&table()).unwrap();
    assert!(rep.overall_score >= 0.0 && rep.overall_score <= 100.0);
}

#[test]
fn hint_keys_align_with_ids() {
    assert_eq!(quality_column_ids().len(), quality_column_hint_keys().len());
    assert_eq!(
        quality_column_ids().len(),
        quality_column_value_hint_keys().len()
    );
}

/// The value hints are indexed by column, so an entry against the wrong column
/// would hang the calendar's explanations off Benford's cells. Only those two
/// columns have any, and each has to carry its own engine's table.
#[test]
fn value_hints_sit_on_the_two_verdict_columns() {
    let by_id: Vec<(&str, usize)> = quality_column_ids()
        .iter()
        .zip(quality_column_value_hint_keys())
        .map(|(id, hints)| (*id, hints.len()))
        .collect();
    for (id, count) in by_id {
        let expected = match id {
            "benford_verdict" => crate::data::benford::VALUE_HINTS.len(),
            "calendar_verdict" => crate::data::calendar_coverage::VALUE_HINTS.len(),
            _ => 0,
        };
        assert_eq!(count, expected, "{id} carries the wrong value hints");
    }
}

/// Every value a verdict column can hold reaches a real catalogue entry, so a
/// hovered cell cannot show a raw key. `t()` falls back to the key itself when
/// it does not know one, which is exactly what this catches.
#[test]
fn every_value_hint_key_resolves() {
    for (value, key) in crate::data::benford::VALUE_HINTS
        .iter()
        .chain(crate::data::calendar_coverage::VALUE_HINTS)
    {
        assert_ne!(crate::i18n::t(key), *key, "{value} has no English text");
    }
}

/// A table whose nulls are scattered has nothing extra to say, and must not
/// grow an empty tab beside the report.
#[test]
fn a_table_without_a_missingness_pattern_has_no_sections() {
    let rep = build_quality_report(&table()).unwrap();
    assert!(rep.sections.is_empty());
}

/// Two columns null in the same rows is the finding the section exists for,
/// and it has to reach the report rather than stop at the engine.
#[test]
fn columns_missing_together_become_a_section() {
    let mut t = table();
    t.columns.push(ColumnInfo {
        name: "phone".into(),
        data_type: "Utf8".into(),
    });
    // Both text columns null in the same six rows, on top of the five above.
    for row in t.rows.iter_mut() {
        row.push(CellValue::String("555".into()));
    }
    for i in 0..6 {
        t.rows.push(vec![
            CellValue::Int(20 + i),
            CellValue::Null,
            CellValue::Null,
        ]);
    }

    let rep = build_quality_report(&t).unwrap();
    assert_eq!(rep.sections.len(), 1, "{:?}", rep.sections.len());
    assert_eq!(rep.sections[0].title_key, "quality.section_missingness");
    let section = &rep.sections[0].table;
    assert_eq!(section.row_count(), 1);
    assert_eq!(
        section.get(0, 0),
        Some(&CellValue::String("email, phone".to_string()))
    );
    assert_eq!(section.get(0, 2), Some(&CellValue::Int(6)));
}

/// The verdict column has to be there for every source column, and to say
/// *why* it did not run rather than leaving a blank the reader has to guess at.
#[test]
fn benford_reports_a_reason_when_it_does_not_apply() {
    let rep = build_quality_report(&table()).unwrap();
    let col = rep
        .table
        .columns
        .iter()
        .position(|c| c.name == "benford_verdict")
        .expect("the report carries a benford_verdict column");
    // Five numeric rows is far below the minimum, and the text column is not
    // numeric at all: two different reasons, both stated.
    assert_eq!(
        rep.table.get(0, col).map(ToString::to_string),
        Some("not tested: too few values".to_string())
    );
    assert_eq!(
        rep.table.get(1, col).map(ToString::to_string),
        Some("not tested: not numbers".to_string())
    );
}

/// A time column with a hole has to reach both surfaces at once: a verdict on
/// the main table, and the gap itself in a section.
#[test]
fn a_gappy_time_column_gets_a_verdict_and_a_section() {
    use chrono::{Datelike, Duration, NaiveDate};
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "day".into(),
        data_type: "Date32".into(),
    }];
    t.rows = (0..60)
        .map(|i| start + Duration::days(i))
        // A three-day hole in the middle.
        .filter(|d| !(20..23).contains(&d.day()) || d.month() != 1)
        .map(|d| vec![CellValue::Date(d.format("%Y-%m-%d").to_string())])
        .collect();

    let rep = build_quality_report(&t).unwrap();
    let col = rep
        .table
        .columns
        .iter()
        .position(|c| c.name == "calendar_verdict")
        .expect("the report carries a calendar_verdict column");
    assert_eq!(
        rep.table.get(0, col).map(ToString::to_string),
        Some("gaps".to_string())
    );

    let section = rep
        .sections
        .iter()
        .find(|s| s.title_key == "quality.section_calendar")
        .expect("the gap is listed");
    assert_eq!(section.table.row_count(), 1);
    assert_eq!(
        section.table.get(0, 3).map(ToString::to_string),
        Some("3".to_string())
    );
}
