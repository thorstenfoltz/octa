use super::*;
use crate::reference::test_server::{ok, serve, zip_of};

const CSV: &str = "FREQ,Frequency,REF_AREA,Reference area,UNIT_MEASURE,Unit of measure,TIME_FORMAT,Time Format,BREAKS,COVERAGE,DECIMALS,Decimals,TITLE_TS,Series,2025,2025-12,2026-01\n\
M,Monthly,DE,Germany,628,Index 2010 = 100,P1M,Monthly,,,2,Two,t,M:DE:628,,131.20,131.60\n\
M,Monthly,DE,Germany,771,Year-on-year changes in %,P1M,Monthly,,,2,Two,t,M:DE:771,,2.10,2.00\n\
A,Annual,DE,Germany,628,Index 2010 = 100,P1Y,Annual,,,2,Two,t,A:DE:628,130.00,,\n\
M,Monthly,XM,Euro area,628,Index 2010 = 100,P1M,Monthly,,,2,Two,t,M:XM:628,,128.00,\n";

#[test]
fn only_the_monthly_index_is_kept() {
    let set = parse(CSV.as_bytes()).unwrap();
    assert_eq!(set["DE"]["2026-01"].text(), "131.60");
    assert_eq!(set["DE"].len(), 2, "no annual value, no yearly change");
    assert_eq!(set["XM"].len(), 1);
}

#[test]
fn a_refresh_writes_the_month_table() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _s) = serve(vec![ok(&zip_of("WS_LONG_CPI_csv_col.csv", CSV), "")]);
    BisInflation
        .refresh_with(dir.path(), &[], Some(&base))
        .unwrap();
    let t = crate::reference::store::read(&dir.path().join(FILE)).unwrap();
    assert_eq!(t.names, vec!["month", "DE", "XM"]);
}
