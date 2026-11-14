use super::*;
use crate::data::{ColumnInfo, DataTable};
use crate::reference::merge::{Groups, Kind, groups_to_table};
use crate::reference::sources::{eurostat, geonames, openplz};
use crate::reference::store;
use std::collections::BTreeMap;

fn put(path: &std::path::Path, names: &[&str], rows: &[&[&str]]) {
    let mut groups = Groups::new();
    for (i, r) in rows.iter().enumerate() {
        groups.insert(
            format!("{i:06}"),
            vec![
                r.iter()
                    .map(|s| (!s.is_empty()).then(|| s.to_string()))
                    .collect(),
            ],
        );
    }
    let kinds: Vec<Kind> = names
        .iter()
        .map(|n| {
            if *n == "lat" || *n == "lon" {
                Kind::Int
            } else {
                Kind::Text
            }
        })
        .collect();
    store::write(
        path,
        &groups_to_table(names, &kinds, &groups, BTreeMap::new()),
    )
    .unwrap();
}

/// Made-up data in the stored layouts: Willich, Tettau and Guteborn,
/// Mönchengladbach, Paris and one London postcode.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    put(
        &geonames::file(d, "DE"),
        &geonames::NAMES,
        &[
            &[
                "01945",
                "Tettau",
                "Brandenburg",
                "",
                "12066",
                "Landkreis Oberspreewald-Lausitz",
                "514333",
                "137333",
            ],
            &[
                "01945",
                "Guteborn",
                "Brandenburg",
                "",
                "12066",
                "Landkreis Oberspreewald-Lausitz",
                "514167",
                "139333",
            ],
            &[
                "47877",
                "Willich",
                "Nordrhein-Westfalen",
                "",
                "05166",
                "Kreis Viersen",
                "512633",
                "65492",
            ],
            &[
                "41169",
                "Mönchengladbach",
                "Nordrhein-Westfalen",
                "",
                "05116",
                "Mönchengladbach",
                "511900",
                "64300",
            ],
        ],
    );
    put(
        &geonames::file(d, "FR"),
        &geonames::NAMES,
        &[&[
            "75001",
            "Paris 01",
            "Île-de-France",
            "",
            "",
            "",
            "488592",
            "23417",
        ]],
    );
    put(
        &geonames::file(d, "GB"),
        &geonames::NAMES,
        &[&[
            "SW1A 1AA", "London", "England", "", "", "", "515010", "-1416",
        ]],
    );
    put(
        &d.join(openplz::FILE),
        &["postcode", "municipality"],
        &[
            &["01945", "12066116"],
            &["01945", "12066316"],
            &["47877", "05116000"],
            &["47877", "05166036"],
            &["41169", "05116000"],
        ],
    );
    put(
        &d.join(eurostat::MUNICIPALITIES),
        &["country", "municipality", "name", "name_latin", "nuts3"],
        &[
            &["DE", "12066116", "Guteborn", "Guteborn", "DE40B"],
            &["DE", "12066316", "Tettau", "Tettau", "DE40B"],
            &[
                "DE",
                "05116000",
                "Mönchengladbach, Stadt",
                "Mönchengladbach, Stadt",
                "DEA15",
            ],
            &[
                "DE",
                "05166036",
                "Willich, Stadt",
                "Willich, Stadt",
                "DEA1E",
            ],
            &["FR", "75056", "Paris", "Paris", "FR101"],
        ],
    );
    // Eurostat gives 01945 one municipality (Ruhland): OpenPLZ must win for Germany.
    put(
        &d.join(eurostat::POSTCODES),
        &["country", "postcode", "nuts3", "municipality"],
        &[
            &["DE", "01945", "DE40B", "12066064"],
            &["FR", "75001", "FR101", "75056"],
        ],
    );
    put(
        &d.join(eurostat::NUTS),
        &["nuts", "name"],
        &[
            &["DE4", "Brandenburg"],
            &["DE40", "Brandenburg"],
            &["DE40B", "Oberspreewald-Lausitz"],
            &["DEA", "Nordrhein-Westfalen"],
            &["DEA1", "Düsseldorf"],
            &["DEA15", "Mönchengladbach"],
            &["DEA1E", "Viersen"],
            &["FR1", "Ile-de-France"],
            &["FR10", "Ile-de-France"],
            &["FR101", "Paris"],
        ],
    );
    dir
}

fn keyed(keys: &[CellValue], countries: &[&str]) -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "key".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "country".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: keys
            .iter()
            .zip(countries)
            .map(|(k, c)| vec![k.clone(), CellValue::String(c.to_string())])
            .collect(),
        ..DataTable::empty()
    }
}

fn look(
    kind: KeyKind,
    keys: &[CellValue],
    countries: &[&str],
    outputs: &[Output],
    several: Several,
) -> Vec<Vec<Option<String>>> {
    let dir = fixture();
    let t = keyed(keys, countries);
    let spec = PlacesSpec {
        key: 0,
        kind,
        country: Pick::Column(1),
        outputs: outputs.to_vec(),
        several,
    };
    let places = Places::load(dir.path(), &countries_in(&t, &spec)).unwrap();
    let c = place_columns(&t, &places, &spec).unwrap();
    (0..t.row_count())
        .map(|r| {
            c.columns
                .iter()
                .map(|col| cell_text(&col.values[r]))
                .collect()
        })
        .collect()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[test]
fn a_postcode_gives_every_key_both_ways_round() {
    let out = look(
        KeyKind::Postcode,
        &[CellValue::String("47877".into())],
        &["Germany"],
        &[
            Output::Place,
            Output::Municipality,
            Output::Kreis,
            Output::KreisName,
            Output::Nuts3,
            Output::Nuts3Name,
            Output::Nuts1,
        ],
        Several::All,
    );
    assert_eq!(
        out[0],
        vec![
            s("Willich"),
            s("05116000, 05166036"),
            s("05116, 05166"),
            s("Mönchengladbach, Kreis Viersen"),
            s("DEA15, DEA1E"),
            s("Mönchengladbach, Viersen"),
            s("DEA")
        ]
    );
}

#[test]
fn first_only_takes_the_first_alphabetically() {
    let out = look(
        KeyKind::Postcode,
        &[CellValue::String("01945".into())],
        &["DE"],
        &[
            Output::Place,
            Output::Latitude,
            Output::Longitude,
            Output::Municipality,
        ],
        Several::First,
    );
    assert_eq!(
        out[0],
        vec![s("Guteborn"), s("51.4167"), s("13.9333"), s("12066116")]
    );
}

#[test]
fn all_places_give_the_centre_and_openplz_wins_in_germany() {
    let out = look(
        KeyKind::Postcode,
        &[CellValue::String("01945".into())],
        &["DE"],
        &[
            Output::Place,
            Output::Latitude,
            Output::Longitude,
            Output::Municipality,
        ],
        Several::All,
    );
    assert_eq!(
        out[0],
        vec![
            s("Guteborn, Tettau"),
            s("51.425"),
            s("13.8333"),
            s("12066116, 12066316")
        ]
    );
}

#[test]
fn leading_zeros_and_uk_spacing_are_forgiven() {
    let out = look(
        KeyKind::Postcode,
        &[CellValue::Int(1945), CellValue::String("sw1a1aa".into())],
        &["DE", "GB"],
        &[Output::Postcode, Output::Place],
        Several::All,
    );
    assert_eq!(out[0], vec![s("01945"), s("Guteborn, Tettau")]);
    assert_eq!(out[1], vec![s("SW1A 1AA"), s("London")]);
}

#[test]
fn a_kreis_lists_its_postcodes_and_municipalities() {
    let out = look(
        KeyKind::Kreis,
        &[CellValue::String("12066".into())],
        &[""],
        &[
            Output::Postcode,
            Output::Municipality,
            Output::MunicipalityName,
        ],
        Several::All,
    );
    assert_eq!(
        out[0],
        vec![s("01945"), s("12066116, 12066316"), s("Guteborn, Tettau")]
    );
}

#[test]
fn a_municipality_with_a_lost_zero_is_found() {
    let out = look(
        KeyKind::Municipality,
        &[CellValue::Int(5166036)],
        &["DE"],
        &[Output::Postcode, Output::Nuts3],
        Several::All,
    );
    assert_eq!(out[0], vec![s("47877"), s("DEA1E")]);
}

#[test]
fn a_nuts_code_carries_its_country() {
    let out = look(
        KeyKind::Nuts,
        &[
            CellValue::String("DEA1E".into()),
            CellValue::String("fr101".into()),
        ],
        &["", ""],
        &[Output::Municipality, Output::Postcode, Output::Nuts3Name],
        Several::All,
    );
    assert_eq!(out[0], vec![s("05166036"), s("47877"), s("Viersen")]);
    assert_eq!(out[1], vec![s("75056"), s("75001"), s("Paris")]);
}

#[test]
fn outside_germany_eurostat_answers_alone() {
    let out = look(
        KeyKind::Postcode,
        &[CellValue::String("75001".into())],
        &["France"],
        &[Output::Municipality, Output::Nuts2, Output::Place],
        Several::All,
    );
    assert_eq!(out[0], vec![s("75056"), s("FR10"), s("Paris 01")]);
}

#[test]
fn nothing_found_is_empty_and_counted() {
    let dir = fixture();
    let t = keyed(&[CellValue::String("99999".into())], &["DE"]);
    let spec = PlacesSpec {
        key: 0,
        kind: KeyKind::Postcode,
        country: Pick::Column(1),
        outputs: vec![Output::Place],
        several: Several::All,
    };
    let c = place_columns(
        &t,
        &Places::load(dir.path(), &["DE".into()]).unwrap(),
        &spec,
    )
    .unwrap();
    assert_eq!((c.found, c.missed), (0, 1));
    assert_eq!(c.columns[0].name, "key_place");
}

#[test]
fn every_output_and_key_names_a_source() {
    assert!(Output::ALL.iter().all(|o| !o.sources().is_empty()));
    assert!(KeyKind::ALL.iter().all(|k| !k.sources().is_empty()));
    assert_eq!(Output::parse("nuts3_name"), Some(Output::Nuts3Name));
    assert_eq!(KeyKind::parse("kreis"), Some(KeyKind::Kreis));
}

#[test]
fn a_key_with_a_letter_past_ascii_does_not_panic() {
    let out = look(
        KeyKind::Nuts,
        &[
            CellValue::String("aÜ".into()),
            CellValue::String("Ü".into()),
        ],
        &["", ""],
        &[Output::Nuts3],
        Several::All,
    );
    assert_eq!(out, vec![vec![None], vec![None]]);
}

#[test]
fn needs_name_what_to_switch_on() {
    use crate::reference::{Dataset, ReferenceSettings};
    let mut r = ReferenceSettings::default();
    assert_eq!(
        needs(&r, KeyKind::Nuts, &[Output::Nuts3]),
        Err(Dataset::EuRegions)
    );
    r.eu_regions.enabled = true;
    assert_eq!(
        needs(&r, KeyKind::Nuts, &[Output::Nuts3]),
        Ok(vec![Dataset::EuRegions])
    );
    assert_eq!(
        needs(&r, KeyKind::Nuts, &[Output::Place]),
        Err(Dataset::Postcodes)
    );
    assert!(!available(&r, Output::Latitude));
    assert!(available(&r, Output::Municipality));
}
