use super::*;
use crate::reference::test_server::{ok, serve, status, zip_of};
use std::time::{Duration, SystemTime};

const DAY: Duration = Duration::from_secs(86_400);

#[test]
fn everything_is_off_by_default_with_a_one_month_interval() {
    let r = ReferenceSettings::default();
    for d in Dataset::ALL {
        let s = r.get(d);
        assert!(!s.enabled, "{} must be off by default", d.id());
        assert_eq!((s.every, s.unit), (1, IntervalUnit::Months));
    }
}

#[test]
fn a_settings_file_without_the_table_reads_as_all_off() {
    let s: crate::ui::settings::AppSettings = toml::from_str("").unwrap();
    assert!(Dataset::ALL.iter().all(|d| !s.reference.get(*d).enabled));
    let s: crate::ui::settings::AppSettings =
        toml::from_str("[reference.inflation]\nenabled = true\nevery = 3\nunit = \"days\"\n")
            .unwrap();
    assert!(s.reference.inflation.enabled);
    assert_eq!(
        (s.reference.inflation.every, s.reference.inflation.unit),
        (3, IntervalUnit::Days)
    );
}

#[test]
fn staleness_follows_the_interval() {
    let now = SystemTime::now();
    let days = SourceSettings {
        enabled: true,
        every: 3,
        unit: IntervalUnit::Days,
    };
    assert!(is_stale(None, &days, now));
    assert!(!is_stale(Some(now - 2 * DAY), &days, now));
    assert!(is_stale(Some(now - 3 * DAY), &days, now));
    let month = SourceSettings {
        enabled: true,
        every: 1,
        unit: IntervalUnit::Months,
    };
    assert!(!is_stale(Some(now - 20 * DAY), &month, now));
    assert!(is_stale(Some(now - 32 * DAY), &month, now));
    let zero = SourceSettings {
        enabled: true,
        every: 0,
        unit: IntervalUnit::Days,
    };
    assert!(
        !is_stale(Some(now - Duration::from_secs(3600)), &zero, now),
        "0 is read as 1"
    );
}

#[test]
fn every_dataset_names_its_source_and_licence() {
    for d in Dataset::ALL {
        assert!(!d.credit().is_empty() && d.credit().is_ascii());
        assert!(d.credit_url().starts_with("https://"));
    }
}

#[test]
fn an_absurd_interval_never_panics_and_never_forces_a_download() {
    let now = SystemTime::now();
    let last = Some(now - 400 * DAY);
    for unit in [IntervalUnit::Days, IntervalUnit::Months] {
        let s = SourceSettings {
            enabled: true,
            every: u32::MAX,
            unit,
        };
        assert!(
            !is_stale(last, &s, now),
            "{unit:?}: an overflowing due date means never, not always"
        );
    }
}

const CPI: &str = "FREQ,Frequency,REF_AREA,Reference area,UNIT_MEASURE,Unit of measure,TIME_FORMAT,Time Format,BREAKS,COVERAGE,DECIMALS,Decimals,TITLE_TS,Series,2026-01\n\
M,Monthly,DE,Germany,628,Index,P1M,Monthly,,,2,Two,t,M:DE,131.60\n";

fn on(d: Dataset) -> ReferenceSettings {
    let mut r = ReferenceSettings::default();
    r.get_mut(d).enabled = true;
    r
}

#[test]
fn a_switched_off_dataset_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let e = prepare_in(
        dir.path(),
        None,
        &ReferenceSettings::default(),
        &[Dataset::Inflation],
        &[],
        SystemTime::now(),
    )
    .err()
    .unwrap();
    assert!(format!("{e:#}").contains("[reference.inflation]"));
}

#[test]
fn missing_data_is_fetched_and_fresh_data_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (base, server) = serve(vec![ok(&zip_of("c.csv", CPI), "")]);
    let r = on(Dataset::Inflation);
    assert!(
        prepare_in(
            dir.path(),
            Some(&base),
            &r,
            &[Dataset::Inflation],
            &[],
            SystemTime::now()
        )
        .unwrap()
        .is_empty()
    );
    // Fresh now: no second request (the server only has one answer).
    prepare_in(
        dir.path(),
        Some(&base),
        &r,
        &[Dataset::Inflation],
        &[],
        SystemTime::now(),
    )
    .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn lookup_prepares_loads_and_computes_in_one_call() {
    use crate::data::reference::inflation::InflationSpec;
    use crate::data::reference::{Pick, Rounding, cell_text};
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let dir = tempfile::tempdir().unwrap();
    let csv = CPI
        .replace(",2026-01\n", ",2015-01,2026-01\n")
        .replace(",131.60\n", ",100.0,131.60\n");
    let (base, _s) = serve(vec![ok(&zip_of("c.csv", &csv), "")]);
    let table = DataTable {
        columns: ["amount", "date"]
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: vec![vec![
            CellValue::String("100".into()),
            CellValue::String("2015-01-20".into()),
        ]],
        ..DataTable::empty()
    };
    let req = Request::Inflation(InflationSpec {
        amount: 0,
        date: Pick::Column(1),
        country: Pick::Fixed("DE".into()),
        target: None,
        rounding: Rounding::Decimals(2),
    });
    let (c, warnings) = lookup_in(
        dir.path(),
        Some(&base),
        &table,
        &req,
        &on(Dataset::Inflation),
        SystemTime::now(),
    )
    .unwrap();
    assert!(warnings.is_empty());
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("131.6"));
    let e = lookup_in(
        dir.path(),
        Some(&base),
        &table,
        &req,
        &ReferenceSettings::default(),
        SystemTime::now(),
    )
    .err()
    .unwrap();
    assert!(format!("{e:#}").contains("[reference.inflation]"));
}

#[test]
fn a_failed_refresh_warns_when_old_data_is_there_and_fails_when_not() {
    let dir = tempfile::tempdir().unwrap();
    let r = on(Dataset::Inflation);
    let (base, _s) = serve(vec![status(500)]);
    assert!(
        prepare_in(
            dir.path(),
            Some(&base),
            &r,
            &[Dataset::Inflation],
            &[],
            SystemTime::now()
        )
        .is_err()
    );
    let (base, _s) = serve(vec![ok(&zip_of("c.csv", CPI), ""), status(500)]);
    prepare_in(
        dir.path(),
        Some(&base),
        &r,
        &[Dataset::Inflation],
        &[],
        SystemTime::now(),
    )
    .unwrap();
    let later = SystemTime::now() + std::time::Duration::from_secs(40 * 86_400);
    let warnings = prepare_in(
        dir.path(),
        Some(&base),
        &r,
        &[Dataset::Inflation],
        &[],
        later,
    )
    .unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("500"), "{}", warnings[0]);
}
