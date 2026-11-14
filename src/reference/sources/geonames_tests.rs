use super::*;
use crate::reference::test_server::{ok, serve, zip_of};

const DE: &str = "DE\t01945\tTettau\tBrandenburg\tBB\t\t00\tLandkreis Oberspreewald-Lausitz\t12066\t51.4333\t13.7333\t4\n\
DE\t01945\tGuteborn\tBrandenburg\tBB\t\t00\tLandkreis Oberspreewald-Lausitz\t12066\t51.4167\t13.9333\t4\n\
DE\t47877\tWillich\tNordrhein-Westfalen\tNW\tRegierungsbezirk Düsseldorf\t051\tKreis Viersen\t05166\t51.2633\t6.5492\t4\n";

#[test]
fn rows_are_grouped_by_postcode_with_coordinates_as_whole_numbers() {
    let g = parse(DE.as_bytes()).unwrap();
    assert_eq!(g["01945"].len(), 2);
    let w = &g["47877"][0];
    assert_eq!(w[1].as_deref(), Some("Willich"));
    assert_eq!(w[4].as_deref(), Some("05166"));
    assert_eq!(w[5].as_deref(), Some("Kreis Viersen"));
    assert_eq!(w[6].as_deref(), Some("512633"));
    assert_eq!(w[7].as_deref(), Some("65492"));
}

#[test]
fn uk_codes_lose_their_space_for_matching_only() {
    let g = parse("GB\tSW1A 1AA\tLondon\tEngland\tENG\t\t\t\t\t51.5010\t-0.1416\t6\n".as_bytes())
        .unwrap();
    assert_eq!(g["SW1A1AA"][0][0].as_deref(), Some("SW1A 1AA"));
    assert_eq!(norm_postcode(" sw1a 1aa "), "SW1A1AA");
}

#[test]
fn the_three_full_countries_use_their_full_files() {
    assert!(url("GB").ends_with("/export/zip/GB_full.csv.zip"));
    assert!(url("DE").ends_with("/export/zip/DE.zip"));
    assert_eq!(COUNTRIES.len(), 124);
    assert!(COUNTRIES.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn a_refresh_fetches_the_asked_country_and_later_the_stored_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (base, server) = serve(vec![
        ok(&zip_of("DE.txt", DE), "ETag: \"d1\"\r\n"),
        ok(&zip_of("DE.txt", DE), ""),
    ]);
    GeoNames
        .refresh_with(dir.path(), &["DE".into()], Some(&base))
        .unwrap();
    assert_eq!(stored(dir.path()), vec!["DE".to_string()]);
    GeoNames.refresh_with(dir.path(), &[], Some(&base)).unwrap();
    let heads = server.join().unwrap();
    assert!(heads[0].starts_with("GET /export/zip/DE.zip"));
    assert!(
        heads[1]
            .to_ascii_lowercase()
            .contains("if-none-match: \"d1\"")
    );
}

#[test]
fn an_unknown_country_is_named_in_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let e = GeoNames
        .refresh_with(dir.path(), &["GR".into()], Some("http://127.0.0.1:9"))
        .err()
        .unwrap();
    assert!(format!("{e:#}").contains("GR"));
}
