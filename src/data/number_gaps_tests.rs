use super::*;

fn ints(v: &[i64]) -> Vec<CellValue> {
    v.iter().map(|&i| CellValue::Int(i)).collect()
}

fn run(owned: &[CellValue], ty: &str) -> Outcome {
    let refs: Vec<&CellValue> = owned.iter().collect();
    analyse(&refs, ty)
}

fn holes(o: Outcome) -> (Vec<Hole>, u64) {
    match o {
        Outcome::Checked { holes, missing } => (holes, missing),
        other => panic!("expected a checked column, got {other:?}"),
    }
}

#[test]
fn every_number_value_is_explained() {
    let listed: Vec<&str> = VALUE_HINTS.iter().map(|(v, _)| *v).collect();
    for v in [COMPLETE, GAPS, REPEAT, NOT_SEQUENCE, TOO_FEW, NOT_NUMBERS] {
        assert!(listed.contains(&v), "{v} has no hint");
    }
}

#[test]
fn the_counter_is_the_last_run_of_digits() {
    let c = split_counter("INV-2024-0007").unwrap();
    assert_eq!(
        (c.prefix.as_str(), c.number, c.width, c.suffix.as_str()),
        ("INV-2024-", 7, 4, "")
    );
    let c = split_counter("A12/b").unwrap();
    assert_eq!(
        (c.prefix.as_str(), c.number, c.suffix.as_str()),
        ("A", 12, "/b")
    );
    assert_eq!(split_counter("Müller 3").unwrap().prefix, "Müller ");
    assert!(split_counter("no digits").is_none());
    assert_eq!(c.format(5), "A05/b");
}

#[test]
fn a_hole_in_whole_numbers_is_listed() {
    let v: Vec<i64> = (1..=50).filter(|i| !(20..=22).contains(i)).collect();
    let (h, missing) = holes(run(&ints(&v), "Int64"));
    assert_eq!(missing, 3);
    assert_eq!(h.len(), 1);
    assert_eq!(
        (h[0].first.as_str(), h[0].last.as_str(), h[0].count),
        ("20", "22", 3)
    );
    assert_eq!(h[0].series, "");
}

#[test]
fn a_complete_run_is_complete() {
    let v: Vec<i64> = (1..=10).collect();
    assert_eq!(run(&ints(&v), "Int64").id(), COMPLETE);
}

#[test]
fn text_ids_keep_their_padding() {
    let v: Vec<CellValue> = (1..=50)
        .filter(|n| !(45..=49).contains(n))
        .map(|n| CellValue::String(format!("INV-2024-{n:04}")))
        .collect();
    let (h, missing) = holes(run(&v, "Utf8"));
    assert_eq!(missing, 5);
    assert_eq!(h[0].series, "INV-2024-#");
    assert_eq!(h[0].first, "INV-2024-0045");
    assert_eq!(h[0].last, "INV-2024-0049");
}

#[test]
fn padding_grows_past_its_width() {
    let v: Vec<CellValue> = (990..=1010)
        .filter(|n| *n != 999 && *n != 1000)
        .map(|n| CellValue::String(format!("{n:04}")))
        .collect();
    let (h, _) = holes(run(&v, "Utf8"));
    assert_eq!((h[0].first.as_str(), h[0].last.as_str()), ("0999", "1000"));
}

#[test]
fn each_prefix_is_its_own_series() {
    let mut v: Vec<CellValue> = (1..=10)
        .map(|n| CellValue::String(format!("INV-2024-{n:04}")))
        .collect();
    v.extend((1..=5).map(|n| CellValue::String(format!("INV-2025-{n:04}"))));
    assert_eq!(run(&v, "Utf8").id(), COMPLETE);
}

#[test]
fn repeats_are_not_a_numbering() {
    let v: Vec<i64> = (18..=60).flat_map(|a| std::iter::repeat_n(a, 10)).collect();
    assert_eq!(run(&ints(&v), "Int64").id(), REPEAT);
}

#[test]
fn sparse_ids_are_not_a_sequence() {
    assert_eq!(
        run(&ints(&[10, 20, 30, 40, 50]), "Int64").id(),
        NOT_SEQUENCE
    );
}

#[test]
fn two_values_are_too_few() {
    assert_eq!(run(&ints(&[1, 2]), "Int64").id(), TOO_FEW);
}

#[test]
fn floats_and_dates_are_not_tested() {
    assert_eq!(run(&[CellValue::Float(1.5)], "Float64").id(), NOT_NUMBERS);
    assert_eq!(
        run(&[CellValue::Date("2024-01-01".into())], "Date32").id(),
        NOT_NUMBERS
    );
}

#[test]
fn the_list_stops_at_the_cap() {
    let v: Vec<i64> = (1..=1000).filter(|i| i % 4 != 0).collect();
    let (h, missing) = holes(run(&ints(&v), "Int64"));
    assert_eq!(missing, 249);
    assert_eq!(h.len(), MAX_GAPS);
}

#[test]
fn the_section_lists_one_row_per_hole() {
    let t = DataTable {
        columns: vec![ColumnInfo {
            name: "invoice".into(),
            data_type: "Int64".into(),
        }],
        rows: (1..=30)
            .filter(|i| *i != 7)
            .map(|i| vec![CellValue::Int(i)])
            .collect(),
        ..DataTable::empty()
    };
    let s = section_table(&t).expect("a hole gives a section");
    assert_eq!(s.row_count(), 1);
    assert_eq!(s.get(0, 0), Some(&CellValue::String("invoice".into())));
    assert_eq!(s.get(0, 4), Some(&CellValue::Int(1)));
}
