use super::*;
use crate::reference::test_server::{ok, serve};

const CSV: &str = "Name,PostalCode,Locality,RegionalKey,Borough,Suburb\n\
\"\"\" 7 Berge \"\"\",74921,Helmstadt-Bargen,08226106,,\n\
Bahnstraße,47877,Willich,05166036,,\n\
Kirchplatz,47877,Willich,05166036,,\n\
Grenzweg,47877,Mönchengladbach,05116000,,\n\
Dorfstraße,01945,Tettau,12066316,,\n\
Kaputt,4787,X,123,,\n";

#[test]
fn distinct_pairs_are_kept_and_broken_codes_dropped() {
    let g = parse(CSV.as_bytes()).unwrap();
    let w: Vec<_> = g["47877"].iter().map(|r| r[1].clone().unwrap()).collect();
    assert_eq!(w, vec!["05116000".to_string(), "05166036".to_string()]);
    assert!(!g.contains_key("4787"));
    assert_eq!(g.len(), 3);
}

#[test]
fn a_refresh_writes_the_pairs() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _s) = serve(vec![ok(CSV.as_bytes(), "ETag: \"o1\"\r\n")]);
    OpenPlz.refresh_with(dir.path(), &[], Some(&base)).unwrap();
    assert_eq!(
        crate::reference::store::read(&dir.path().join(FILE))
            .unwrap()
            .rows(),
        4
    );
}
