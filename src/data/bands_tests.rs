use super::*;
use crate::data::ColumnInfo;

fn ints(v: &[i64]) -> Vec<CellValue> {
    v.iter().map(|i| CellValue::Int(*i)).collect()
}

fn texts(v: &[&str]) -> Vec<CellValue> {
    v.iter().map(|s| CellValue::String((*s).into())).collect()
}

fn shown(cells: &[CellValue]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

fn back(labels: &[&str], b: Back) -> Vec<String> {
    shown(&from_bands(&texts(labels), b).0)
}

#[test]
fn equal_widths_of_whole_numbers_get_whole_labels() {
    let bands = to_bands(&ints(&[0, 19, 20, 99]), &BandBy::Width(5)).unwrap();
    assert_eq!(shown(&bands), ["0-19", "0-19", "20-39", "80-99"]);
    // Negative numbers are written with "to", so the minus stays readable.
    let neg = to_bands(&ints(&[-10, -6, -5, -1]), &BandBy::Width(2)).unwrap();
    assert_eq!(
        shown(&neg),
        ["-10 to -6", "-10 to -6", "-5 to -1", "-5 to -1"]
    );
}

#[test]
fn equal_counts_cut_at_the_quantiles() {
    let bands = to_bands(&ints(&[1, 2, 3, 4, 5, 6, 7, 8]), &BandBy::Count(4)).unwrap();
    assert_eq!(
        shown(&bands),
        ["1-2", "1-2", "3-4", "3-4", "5-6", "5-6", "7-8", "7-8"]
    );
    // Equal values cannot be cut apart: fewer bands.
    let tied = to_bands(&ints(&[1, 1, 1, 1, 5]), &BandBy::Count(4)).unwrap();
    assert_eq!(shown(&tied), ["1-5", "1-5", "1-5", "1-5", "1-5"]);
}

#[test]
fn own_edges_open_below_the_first_and_from_the_last() {
    let bands = to_bands(
        &ints(&[5, 18, 34, 35, 70]),
        &BandBy::Edges(vec![65.0, 18.0, 35.0]),
    )
    .unwrap();
    assert_eq!(shown(&bands), ["<18", "18-34", "18-34", "35-64", "65+"]);
}

#[test]
fn decimals_get_interval_labels_and_non_numbers_stay_empty() {
    let values = vec![
        CellValue::Float(0.5),
        CellValue::Float(1.5),
        CellValue::Float(2.5),
        CellValue::String("n/a".into()),
        CellValue::Null,
    ];
    let bands = to_bands(&values, &BandBy::Width(2)).unwrap();
    assert_eq!(
        shown(&bands),
        ["[0.5, 1.5)", "[1.5, 2.5]", "[1.5, 2.5]", "", ""]
    );
    assert_eq!(
        to_bands(&texts(&["a"]), &BandBy::Width(2)),
        Err(BandError::NoNumbers)
    );
    assert_eq!(
        to_bands(&ints(&[1]), &BandBy::Width(0)),
        Err(BandError::NoBands)
    );
    assert_eq!(
        to_bands(&ints(&[1]), &BandBy::Edges(vec![])),
        Err(BandError::NoBands)
    );
}

#[test]
fn age_bands_come_back_as_their_middles_and_open_ends_take_a_neighbours_width() {
    let labels = ["<18", "18-24", "25-34", "35-44", "45-54", "55-64", "65+"];
    assert_eq!(
        back(&labels, Back::Middle),
        ["15.0", "21.0", "29.5", "39.5", "49.5", "59.5", "69.5"]
    );
    assert_eq!(
        back(&labels, Back::Lower),
        ["12.0", "18.0", "25.0", "35.0", "45.0", "55.0", "65.0"]
    );
    assert_eq!(back(&labels, Back::Upper)[6], "74.0");
}

#[test]
fn labels_in_other_shapes_and_languages_read_too() {
    // Words between the numbers do not matter; a lone number with words is
    // placed by the other bands.
    assert_eq!(
        back(
            &["unter 18", "18 bis 24", "25 bis 34", "35 und älter"],
            Back::Middle
        ),
        ["15.0", "21.0", "29.5", "39.5"]
    );
    assert_eq!(
        back(&["18 to 24", "18 – 24", "[18, 25)"], Back::Middle),
        ["21.0", "21.0", "21.5"]
    );
    // Thousands and k, currency signs around them.
    assert_eq!(
        back(&["10,000-19,999", "€20k-30k", "$50k+"], Back::Lower),
        ["10000.0", "20000.0", "50000.0"]
    );
    assert_eq!(
        back(&["-10 to -5", "-10--6"], Back::Upper),
        ["-5.0", "-6.0"]
    );
}

#[test]
fn a_bare_number_is_that_number_and_unreadable_labels_are_counted() {
    // "0" beside "1-4" is the age 0, not an open band.
    assert_eq!(
        back(&["0", "1-4", "100"], Back::Middle),
        ["0.0", "2.5", "100.0"]
    );
    // "18 months": the m is the start of a word, not millions.
    assert_eq!(back(&["18 months"], Back::Lower), ["18.0"]);
    let (cells, unread) = from_bands(
        &[
            CellValue::String("unknown".into()),
            CellValue::String(" ".into()),
            CellValue::Null,
            CellValue::String("18-24".into()),
        ],
        Back::Middle,
    );
    assert_eq!(shown(&cells), ["", "", "", "21.0"]);
    assert_eq!(unread, 1, "only the label with text but no number");
}

#[test]
fn bands_octa_writes_read_back() {
    let ages = ints(&[5, 18, 34, 35, 70]);
    let bands = to_bands(&ages, &BandBy::Edges(vec![18.0, 35.0, 65.0])).unwrap();
    let (lower, unread) = from_bands(&bands, Back::Lower);
    assert_eq!(unread, 0);
    // "<18" takes the width of 18-34: 2 to 18.
    assert_eq!(shown(&lower), ["2.0", "18.0", "18.0", "35.0", "65.0"]);
}

#[test]
fn edges_are_typed_with_commas_or_semicolons() {
    assert_eq!(parse_edges("18, 35, 65"), Ok(vec![18.0, 35.0, 65.0]));
    assert_eq!(parse_edges("18 35\n65"), Ok(vec![18.0, 35.0, 65.0]));
    assert_eq!(parse_edges("1,5; 3"), Ok(vec![1.5, 3.0]));
    assert_eq!(parse_edges("18, x"), Err("x".to_string()));
}

#[test]
fn the_new_column_lands_right_after_its_source() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "age".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "age_band".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(20), CellValue::Null],
        vec![CellValue::Int(70), CellValue::Null],
    ];
    let spec = BandSpec::ToBands(BandBy::Edges(vec![18.0, 65.0]));
    let (name, _) = add_band_column(&mut t, 0, &spec).unwrap();
    assert_eq!(name, "age_band_2", "the taken name gets a number");
    assert_eq!(t.columns[1].name, "age_band_2");
    assert_eq!(t.get(1, 1).unwrap().to_string(), "65+");

    let (name, unread) = add_band_column(&mut t, 1, &BandSpec::Back(Back::Lower)).unwrap();
    assert_eq!((name.as_str(), unread), ("age_band_2_lower", 0));
    assert_eq!(t.columns[2].data_type, "Int64");
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(18)));
}
