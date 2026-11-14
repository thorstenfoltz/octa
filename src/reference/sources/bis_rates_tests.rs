use super::*;
use crate::reference::test_server::{not_modified, ok, serve, zip_of};

/// Made up, in BIS's column layout: id columns, `TITLE,Series`, then one
/// column per period of every frequency.
const CSV: &str = "FREQ,Frequency,REF_AREA,Reference area,CURRENCY,Currency,COLLECTION,Collection,UNIT_MULT,Unit Multiplier,DECIMALS,Decimals,AVAILABILITY,Availability,TITLE,Series,2026,2026-01,2026-Q1,2026-01-02,2026-01-05\n\
D,Daily,XM,Euro area,EUR,Euro,A,Average,0,Units,4,Four,A,All,t,D:XM,,,,0.9500,0.9512\n\
D,Daily,DE,Germany,EUR,Euro,A,Average,0,Units,4,Four,A,All,t,D:DE,,,,0.9999,0.9999\n\
D,Daily,JP,Japan,JPY,Yen,A,Average,0,Units,2,Two,A,All,t,D:JP,,,,157.10,NaN\n\
M,Monthly,AR,Argentina,ARS,Peso,A,Average,0,Units,2,Two,A,All,t,M:AR:A,,1050.25,,,\n\
M,Monthly,AR,Argentina,ARS,Peso,E,End of period,0,Units,2,Two,A,All,t,M:AR:E,,1060.00,,,\n\
Q,Quarterly,AR,Argentina,ARS,Peso,A,Average,0,Units,2,Two,A,All,t,Q:AR,,,1070.00,,\n";

#[test]
fn daily_and_monthly_average_series_are_read_first_value_winning() {
    let (daily, monthly) = parse(CSV.as_bytes()).unwrap();
    assert_eq!(daily["EUR"]["2026-01-02"].text(), "0.9500"); // the euro area row came first
    assert_eq!(daily["EUR"].len(), 2);
    assert_eq!(daily["JPY"].len(), 1, "NaN is no value");
    assert_eq!(
        monthly["ARS"]["2026-01"].text(),
        "1050.25",
        "averages, not end of period"
    );
    assert!(!monthly["ARS"].contains_key("2026-Q1"));
    assert!(!daily.contains_key("ARS"));
}

#[test]
fn a_refresh_stores_merges_and_skips_an_unchanged_file() {
    let dir = tempfile::tempdir().unwrap();
    let body = zip_of("WS_XRU_csv_col.csv", CSV);
    let later = CSV.replace("0.9512", "0.9600");
    let (base, server) = serve(vec![
        ok(&body, "ETag: \"v1\"\r\n"),
        not_modified(),
        ok(&zip_of("WS_XRU_csv_col.csv", &later), "ETag: \"v2\"\r\n"),
    ]);
    let src = BisRates;
    assert_eq!(
        src.refresh_with(dir.path(), &[], Some(&base))
            .unwrap()
            .downloaded,
        1
    );
    let daily = crate::reference::store::read(&dir.path().join(DAILY)).unwrap();
    assert_eq!(
        daily
            .meta
            .get(crate::reference::store::META_ETAG)
            .map(String::as_str),
        Some("\"v1\"")
    );
    assert_eq!(
        src.refresh_with(dir.path(), &[], Some(&base))
            .unwrap()
            .unchanged,
        1
    );
    src.refresh_with(dir.path(), &[], Some(&base)).unwrap();
    let set = crate::reference::merge::table_to_series(
        &crate::reference::store::read(&dir.path().join(DAILY)).unwrap(),
        "date",
    )
    .unwrap();
    assert_eq!(set["EUR"]["2026-01-05"].text(), "0.9600");
    let heads = server.join().unwrap();
    assert!(
        heads[1]
            .to_ascii_lowercase()
            .contains("if-none-match: \"v1\"")
    );
}

#[test]
fn a_download_without_daily_rates_is_refused_and_keeps_the_old_files() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _server) = serve(vec![
        ok(&zip_of("a.csv", CSV), ""),
        ok(&zip_of("a.csv", "FREQ,CURRENCY,COLLECTION,Series\n"), ""),
    ]);
    BisRates.refresh_with(dir.path(), &[], Some(&base)).unwrap();
    let e = BisRates
        .refresh_with(dir.path(), &[], Some(&base))
        .err()
        .unwrap();
    assert!(format!("{e:#}").contains("no daily exchange rates"));
    assert!(dir.path().join(DAILY).exists());
}
