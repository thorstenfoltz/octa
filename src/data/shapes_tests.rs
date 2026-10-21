//! Unit tests for [`shapes`](super). Included via `#[path]`.

use super::*;
use std::collections::HashSet;

fn table(values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![crate::data::ColumnInfo {
        name: "code".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = values
        .iter()
        .map(|v| {
            vec![if v.is_empty() {
                CellValue::Null
            } else {
                CellValue::String((*v).into())
            }]
        })
        .collect();
    t
}

#[test]
fn shape_maps_digits_capitals_and_letters_and_keeps_punctuation() {
    assert_eq!(shape_of("D-80331", ShapeDetail::Exact), "A-99999");
    assert_eq!(shape_of("anna@x.de", ShapeDetail::Exact), "aaaa@a.aa");
    assert_eq!(shape_of("Zoe 12", ShapeDetail::Exact), "Aaa 99");
    assert_eq!(shape_of("東京1", ShapeDetail::Exact), "aa9");
}

#[test]
fn words_write_a_run_of_one_kind_of_letter_once() {
    let w = |v| shape_of(v, ShapeDetail::Words);
    assert_eq!(w("Anna Schmidt"), "Aa Aa");
    assert_eq!(w("anna schmidt"), "a a");
    assert_eq!(w("ANNA"), "A");
    assert_eq!(w("McDonald"), "AaAa");
    // Digits keep their count: that is what tells codes apart.
    assert_eq!(w("D-80331"), "A-99999");
    assert_eq!(w("INV-2026-0042"), "A-9999-9999");
    assert_eq!(w("anna@x.de"), "a@a.a");
    assert_eq!(w("東京1"), "a9");
    // Two names of different lengths are one shape; Exact tells them apart.
    let t = table(&["Anna Schmidt", "Bob Li", "eve"]);
    assert_eq!(shape_frequency(&t, 0, ShapeDetail::Words).shapes.len(), 2);
    assert_eq!(shape_frequency(&t, 0, ShapeDetail::Exact).shapes.len(), 3);
}

#[test]
fn long_values_compress_runs_of_four_or_more() {
    let v = "abcdefghijklmnopqrstuvwxyz-12";
    assert_eq!(shape_of(v, ShapeDetail::Exact), "a(26)-99");
    // Short values never compress, even with long runs.
    assert_eq!(shape_of("aaaaaa", ShapeDetail::Exact), "aaaaaa");
}

#[test]
fn frequency_sorts_by_count_and_counts_empties_apart() {
    let t = table(&["D-1", "D-2", "E-3", "123", "", "x"]);
    let s = shape_frequency(&t, 0, ShapeDetail::Exact);
    assert_eq!(s.empty, 1);
    assert_eq!(s.shapes[0].shape, "A-9");
    assert_eq!(s.shapes[0].count, 3);
    assert_eq!(s.shapes[0].example, "D-1");
    assert_eq!(s.shapes.len(), 3);
}

#[test]
fn values_with_shapes_expands_to_every_matching_value() {
    let t = table(&["D-1", "D-2", "123", ""]);
    let want: HashSet<String> = ["A-9".to_string()].into();
    let got = values_with_shapes(&t, 0, &want, ShapeDetail::Exact);
    assert_eq!(got, ["D-1".to_string(), "D-2".to_string()].into());
}

#[test]
fn verdict_is_mixed_only_with_a_dominant_shape_and_few_shapes() {
    let mut vals = vec!["D-1"; 19];
    vals.push("12");
    assert_eq!(
        verdict(&shape_frequency(&table(&vals), 0, ShapeDetail::Exact)),
        ShapeVerdict::Mixed
    );
    assert_eq!(
        verdict(&shape_frequency(
            &table(&["D-1", "E-2"]),
            0,
            ShapeDetail::Exact
        )),
        ShapeVerdict::Consistent
    );
    // 50/50: two formats on purpose, not a straggler.
    assert_eq!(
        verdict(&shape_frequency(
            &table(&["D-1", "12"]),
            0,
            ShapeDetail::Exact
        )),
        ShapeVerdict::NotApplicable
    );
    assert_eq!(
        verdict(&shape_frequency(&table(&[""]), 0, ShapeDetail::Exact)),
        ShapeVerdict::NotApplicable
    );
}

#[test]
fn section_lists_minority_shapes_of_mixed_text_columns_only() {
    let mut vals = vec!["D-1"; 19];
    vals.push("12");
    let s =
        section_table(&table(&vals), ShapeDetail::Exact).expect("a mixed column gives a section");
    let names: Vec<&str> = s.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["column", "shape", "count", "example"]);
    assert_eq!(s.rows.len(), 1);
    assert_eq!(s.rows[0][1], CellValue::String("99".into()));
    assert!(section_table(&table(&["D-1", "E-2"]), ShapeDetail::Exact).is_none());
}
