use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};
use std::sync::atomic::AtomicBool;

fn table(rows: usize) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Float64".into(),
        },
    ];
    t.rows = (0..rows)
        .map(|i| {
            vec![
                CellValue::Int(i as i64),
                CellValue::String(if i % 2 == 0 { "Aachen" } else { "Bonn" }.to_string()),
                CellValue::Float(i as f64 * 1.5),
            ]
        })
        .collect();
    t
}

fn all_rows(t: &DataTable) -> Vec<usize> {
    (0..t.row_count()).collect()
}

fn build(t: &DataTable, opts: &ReportOptions) -> String {
    build_report(t, &all_rows(t), opts, &AtomicBool::new(false)).expect("build report")
}

#[test]
fn report_is_one_self_contained_document() {
    let t = table(20);
    let html = build(&t, &ReportOptions::default());

    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "must be a whole document"
    );
    assert!(html.contains("</html>"));
    assert!(!html.contains("<script"), "no JavaScript");

    // Self-contained means it opens on a machine with no internet. What
    // matters is that nothing is *fetched*, so the assertion targets remote
    // references rather than any occurrence of a URL: an inline SVG carries
    // `xmlns="http://www.w3.org/2000/svg"`, which is a namespace identifier
    // and is never dereferenced. Asserting on the bare substring would pass
    // here and fail the moment charts are added.
    for attr in ["src=\"http", "href=\"http", "url(http", "@import"] {
        assert!(
            !html.contains(attr),
            "found a remote reference ({attr}); the report must fetch nothing"
        );
    }
}

#[test]
fn the_title_appears_in_the_document() {
    let t = table(5);
    let opts = ReportOptions {
        title: "Sales export".into(),
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(html.contains("Sales export"), "title must be rendered");
    assert!(html.contains("<title>Sales export</title>"));
}

#[test]
fn statistics_section_names_every_column() {
    let t = table(10);
    let opts = ReportOptions {
        sections: vec![ReportSection::Stats],
        ..Default::default()
    };
    let html = build(&t, &opts);
    for name in ["id", "city", "amount"] {
        assert!(
            html.contains(name),
            "column {name} missing from the statistics"
        );
    }
}

#[test]
fn sections_can_be_switched_off() {
    let t = table(10);
    let none = build(
        &t,
        &ReportOptions {
            sections: Vec::new(),
            ..Default::default()
        },
    );
    assert!(
        none.contains("</html>"),
        "an empty report is still a document"
    );
    assert!(!none.contains("<table"), "no sections means no tables");
}

/// A sampled report must say so, or a reader would take approximate numbers
/// for exact ones.
#[test]
fn sampling_is_stated_in_the_output() {
    let t = table(500);
    let opts = ReportOptions {
        sample_rows: Some(50),
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(html.contains("50"), "the examined count must appear");
    assert!(html.contains("500"), "the total count must appear");
}

/// Sampling draws from the caller's view, not the whole table. A report that
/// announces "N of M rows examined" under an active filter must not have
/// examined rows the filter hides.
#[test]
fn sampling_draws_from_the_filtered_view() {
    let t = table(100);
    // Only the first ten rows are visible, so ids 0..10 and nothing above.
    let visible: Vec<usize> = (0..10).collect();
    let opts = ReportOptions {
        sample_rows: Some(5),
        sections: vec![ReportSection::Stats],
        ..Default::default()
    };
    let html = build_report(&t, &visible, &opts, &AtomicBool::new(false)).expect("report");

    assert!(
        html.contains("5 of 10 rows examined"),
        "totals must come from the view, got:\n{html}"
    );
}

#[test]
fn a_full_pass_does_not_claim_to_be_sampled() {
    let t = table(30);
    let html = build(&t, &ReportOptions::default());
    assert!(
        !html.to_lowercase().contains("sample"),
        "a full pass must not mention sampling"
    );
}

#[test]
fn an_empty_table_is_a_report_not_an_error() {
    let t = DataTable::empty();
    let html = build_report(&t, &[], &ReportOptions::default(), &AtomicBool::new(false))
        .expect("empty table must still produce a report");
    assert!(html.contains("</html>"));
}

/// HTML injection through column names or cell values must not escape.
#[test]
fn content_is_escaped() {
    let mut t = table(2);
    t.columns[1].name = "<script>alert(1)</script>".into();
    let html = build(&t, &ReportOptions::default());
    assert!(
        !html.contains("<script>alert(1)</script>"),
        "column name was not escaped"
    );
    assert!(html.contains("&lt;script&gt;"), "expected escaped output");
}

#[test]
fn cancellation_stops_the_build() {
    let t = table(100);
    let cancelled = AtomicBool::new(true);
    let err = build_report(&t, &all_rows(&t), &ReportOptions::default(), &cancelled);
    assert!(
        err.is_err(),
        "a cancelled build must not return a half-written report"
    );
}

#[test]
fn distributions_embed_one_svg_per_column() {
    let t = table(40);
    let opts = ReportOptions {
        sections: vec![ReportSection::Distributions],
        ..Default::default()
    };
    let html = build(&t, &opts);
    let svgs = html.matches("<svg").count();
    assert_eq!(svgs, 3, "one chart per column, got {svgs}");
    assert!(
        !html.contains("<img"),
        "charts must be inline SVG, not linked images"
    );
}

#[test]
fn charted_columns_are_capped_and_the_omission_is_stated() {
    let mut t = table(10);
    // Widen well past the cap.
    for i in 0..12 {
        t.columns.push(ColumnInfo {
            name: format!("extra_{i}"),
            data_type: "Int64".into(),
        });
        for row in t.rows.iter_mut() {
            row.push(CellValue::Int(1));
        }
    }
    let opts = ReportOptions {
        sections: vec![ReportSection::Distributions],
        max_charted_columns: 4,
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert_eq!(html.matches("<svg").count(), 4, "cap must be honoured");
    assert!(
        html.contains("11 more"),
        "the omission must be named, got:\n{html}"
    );
}

#[test]
fn top_values_lists_the_most_frequent() {
    let t = table(20);
    let opts = ReportOptions {
        sections: vec![ReportSection::TopValues],
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(html.contains("Aachen"), "expected the most frequent city");
    assert!(html.contains("Bonn"));
}

#[test]
fn correlation_is_omitted_when_nothing_is_numeric() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "city".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![vec![CellValue::String("Aachen".into())]];
    let opts = ReportOptions {
        sections: vec![ReportSection::Correlation],
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(html.contains("</html>"), "must still be a document");
    assert!(
        !html.contains("<table"),
        "a table with no numeric columns has no correlation to show"
    );
}

/// One numeric column correlates only with itself, which is always 1 and
/// always noise, so it is omitted just like no numeric columns at all.
#[test]
fn correlation_is_omitted_for_a_single_numeric_column() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "amount".into(),
        data_type: "Float64".into(),
    }];
    t.rows = (0..10).map(|i| vec![CellValue::Float(i as f64)]).collect();
    let opts = ReportOptions {
        sections: vec![ReportSection::Correlation],
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(
        !html.contains("<table"),
        "a 1x1 matrix is not worth showing"
    );
}

#[test]
fn correlation_appears_when_numeric_columns_exist() {
    let t = table(30);
    let opts = ReportOptions {
        sections: vec![ReportSection::Correlation],
        ..Default::default()
    };
    let html = build(&t, &opts);
    assert!(html.contains("<table"), "expected a correlation matrix");
    assert!(html.contains("amount"));
}
