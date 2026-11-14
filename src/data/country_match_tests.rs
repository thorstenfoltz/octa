use super::*;
use crate::data::chart::Aggregation;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn iso3(i: usize) -> String {
    countries().iso3[i].clone()
}

#[test]
fn codes_names_other_languages_and_aliases_match() {
    let c = countries();
    let de = c.find("DEU").map(iso3);
    assert_eq!(de.as_deref(), Some("DEU"));
    assert_eq!(c.find("de").map(iso3).as_deref(), Some("DEU"));
    assert_eq!(c.find("Germany").map(iso3).as_deref(), Some("DEU"));
    assert_eq!(c.find("  deutschland ").map(iso3).as_deref(), Some("DEU"));
    assert_eq!(c.find("Österreich").map(iso3).as_deref(), Some("AUT"));
    assert_eq!(c.find("osterreich").map(iso3).as_deref(), Some("AUT"));
    assert_eq!(c.find("USA").map(iso3).as_deref(), Some("USA"));
    assert_eq!(c.find("U.K.").map(iso3).as_deref(), Some("GBR"));
    assert_eq!(c.find("Holland").map(iso3).as_deref(), Some("NLD"));
    assert_eq!(c.find("Côte d'Ivoire").map(iso3).as_deref(), Some("CIV"));
    assert_eq!(c.find("Untied States"), None);
}

/// `samples/geo/countries.csv` is the file to try the Countries map with, and the
/// docs and the sample notes promise what it shows.
#[test]
fn the_sample_file_opens_on_the_map_with_two_values_unmatched() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/geo/countries.csv");
    let registry = crate::formats::FormatRegistry::new();
    let table = registry
        .reader_for_path(&path)
        .expect("a reader")
        .read_file(&path)
        .expect("read");

    let col = detect_country_column(&table).expect("the country column");
    assert_eq!(table.columns[col].name, "country");
    let rows: Vec<usize> = (0..table.rows.len()).collect();
    let values = aggregate(&table, &rows, col, Some(1), Aggregation::Sum);

    let mut unmatched: Vec<&str> = values.unmatched.iter().map(|(v, _)| v.as_str()).collect();
    unmatched.sort_unstable();
    assert_eq!(unmatched, ["Atlantis", "Germny"]);

    let of = |name: &str| values.by_country[countries().find(name).unwrap()].unwrap();
    assert_eq!(of("DE"), (2300.0, 3), "a code, a name and a translation");
    assert_eq!(of("USA"), (3500.0, 2));
    assert_eq!(of("JP"), (1800.0, 2), "Japan and its Japanese name");
}

#[test]
fn the_bundled_outlines_cover_the_world() {
    let c = countries();
    assert!(c.names.len() > 170, "{}", c.names.len());
    assert!(c.features.iter().all(|f| f.geometry.is_some()));
}

fn table(rows: Vec<(&str, Option<i64>)>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "sales".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = rows
        .into_iter()
        .map(|(c, v)| {
            vec![
                CellValue::String(c.into()),
                v.map_or(CellValue::Null, CellValue::Int),
            ]
        })
        .collect();
    t
}

#[test]
fn several_rows_per_country_combine_and_unmatched_are_counted() {
    let t = table(vec![
        ("DE", Some(5)),
        ("Germany", Some(7)),
        ("France", Some(1)),
        ("Untied States", Some(3)),
        ("Untied States", None),
    ]);
    let rows: Vec<usize> = (0..t.row_count()).collect();
    let v = aggregate(&t, &rows, 0, Some(1), Aggregation::Sum);
    let de = countries().find("DEU").unwrap();
    assert_eq!(v.by_country[de], Some((12.0, 2)));
    assert_eq!(v.unmatched, vec![("Untied States".to_string(), 2)]);
    let counted = aggregate(&t, &rows, 0, None, Aggregation::Count);
    assert_eq!(counted.by_country[de], Some((2.0, 2)));
}

#[test]
fn a_country_column_is_detected_and_a_city_column_is_not() {
    let t = table(vec![
        ("DE", None),
        ("France", None),
        ("Japan", None),
        ("Peru", None),
    ]);
    assert_eq!(detect_country_column(&t), Some(0));
    let cities = table(vec![
        ("Berlin", None),
        ("Paris", None),
        ("Tokyo", None),
        ("Lima", None),
    ]);
    assert_eq!(detect_country_column(&cities), None);
}

#[test]
fn codes_missing_from_iso_a2_still_match() {
    let c = countries();
    assert_eq!(c.find("FR").map(iso3).as_deref(), Some("FRA"));
    assert_eq!(c.find("FRA").map(iso3).as_deref(), Some("FRA"));
    assert_eq!(c.find("France").map(iso3).as_deref(), Some("FRA"));
    assert_eq!(c.find("NO").map(iso3).as_deref(), Some("NOR"));
    assert_eq!(c.find("Norway").map(iso3).as_deref(), Some("NOR"));
}

#[test]
fn a_plain_number_matches_no_country() {
    let c = countries();
    for n in ["6", "9", "12"] {
        assert_eq!(c.find(n), None, "{n}");
    }
}

#[test]
fn every_outline_resolves_by_its_own_codes() {
    let c = countries();
    for i in 0..c.names.len() {
        assert_eq!(c.find(&c.iso3[i]), Some(i), "{}", c.iso3[i]);
        if !c.iso2[i].is_empty() {
            assert_eq!(c.find(&c.iso2[i]), Some(i), "{}", c.iso2[i]);
        }
    }
}

#[test]
fn punctuation_does_not_stop_a_match() {
    let c = countries();
    assert_eq!(c.find("U.S.A.").map(iso3).as_deref(), Some("USA"));
    assert_eq!(
        c.find("Cote d\u{2019}Ivoire").map(iso3).as_deref(),
        Some("CIV")
    );
    assert_eq!(c.find("Bosnia and Herz").map(iso3).as_deref(), Some("BIH"));
}

#[test]
fn an_all_no_flag_column_is_not_norway() {
    let t = table(vec![("No", None), ("No", None), ("no", None), ("NO", None)]);
    assert_eq!(detect_country_column(&t), None);
}
