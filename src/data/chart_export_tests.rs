//! Unit tests for [`chart_export`](chart_export). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::chart::{BoxSummary, ChartPrep, ChartSeries, XAxisKind};

fn prep_lines() -> ChartPrep {
    ChartPrep {
        data: ChartData::Lines {
            categories: None,
            series: vec![ChartSeries {
                name: "y".into(),
                points: vec![[0.0, 1.0], [1.0, 2.0], [2.0, 1.5]],
            }],
        },
        total_rows: 3,
        used_rows: 3,
        x_label: "x".into(),
        y_label: "y".into(),
        x_axis_kind: XAxisKind::Numeric,
    }
}

#[test]
fn svg_emits_well_formed_root() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: vec![ResolvedSeries {
                display_name: "y".into(),
                color: None,
            }],
        },
    );
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains(">T<"));
}

#[test]
fn svg_includes_legend_when_position_set() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: String::new(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::TopRight,
            series: vec![ResolvedSeries {
                display_name: "my-legend".into(),
                color: None,
            }],
        },
    );
    assert!(svg.contains(">my-legend<"), "{svg}");
}

#[test]
fn svg_escapes_special_chars() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "a & b < c".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
        },
    );
    assert!(svg.contains("a &amp; b &lt; c"));
}

#[test]
fn png_decodes_to_non_empty_buffer() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
        },
    );
    let png = to_png(&svg, 1.0).unwrap();
    // PNG signature: 89 50 4E 47 0D 0A 1A 0A
    assert_eq!(&png[..4], b"\x89PNG");
}

#[test]
fn pdf_starts_with_magic() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
        },
    );
    let pdf = to_pdf(&svg).unwrap();
    assert_eq!(&pdf[..4], b"%PDF");
}

#[test]
fn box_summary_serialises() {
    let prep = ChartPrep {
        data: ChartData::Boxes(vec![BoxSummary {
            name: "v".into(),
            lower_whisker: 1.0,
            q1: 2.0,
            median: 3.0,
            q3: 4.0,
            upper_whisker: 5.0,
        }]),
        total_rows: 5,
        used_rows: 5,
        x_label: "Series".into(),
        y_label: "Value".into(),
        x_axis_kind: XAxisKind::Numeric,
    };
    let svg = to_svg(
        &prep,
        &ExportOptions {
            title: String::new(),
            x_label: "Series".into(),
            y_label: "Value".into(),
            legend: LegendPosition::Off,
            series: vec![ResolvedSeries {
                display_name: "v".into(),
                color: None,
            }],
        },
    );
    assert!(svg.contains("<rect"));
    assert!(svg.contains(">v<"));
}
