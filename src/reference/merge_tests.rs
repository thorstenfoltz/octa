use super::*;
use crate::data::decimal::Exact;

fn s(pairs: &[(&str, &str)]) -> Series {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Exact::parse(v).unwrap()))
        .collect()
}

#[test]
fn a_new_value_overwrites_and_a_missing_one_keeps_the_old() {
    let old: SeriesSet = [(
        "USD".to_string(),
        s(&[("2026-01", "1.10"), ("2026-02", "1.20")]),
    )]
    .into();
    let new: SeriesSet = [(
        "USD".to_string(),
        s(&[("2026-02", "1.25"), ("2026-03", "1.30")]),
    )]
    .into();
    let (merged, rebased) = merge_series(&old, &new);
    assert!(rebased.is_empty());
    let usd = &merged["USD"];
    assert_eq!(usd["2026-01"].text(), "1.10");
    assert_eq!(usd["2026-02"].text(), "1.25");
    assert_eq!(usd["2026-03"].text(), "1.30");
}

#[test]
fn a_series_only_in_the_old_file_stays() {
    let old: SeriesSet = [("ROL".to_string(), s(&[("1999-01", "4.5")]))].into();
    let (merged, _) = merge_series(&old, &SeriesSet::new());
    assert!(merged.contains_key("ROL"));
}

#[test]
fn a_rebased_series_is_taken_whole_from_the_new_download() {
    // 2010 = 100 -> 2020 = 100: every overlapping value shrinks by the same factor.
    let old = s(&[
        ("2009-12", "99.0"),
        ("2010-01", "100.0"),
        ("2015-06", "110.0"),
        ("2020-01", "125.0"),
    ]);
    let new = s(&[
        ("2010-01", "80.0"),
        ("2015-06", "88.0"),
        ("2020-01", "100.0"),
    ]);
    assert!(is_rebased(&old, &new));
    let (merged, rebased) = merge_series(
        &[("DE".to_string(), old)].into(),
        &[("DE".to_string(), new.clone())].into(),
    );
    assert_eq!(rebased, vec!["DE".to_string()]);
    assert_eq!(merged["DE"], new); // the old-only 2009-12 is dropped, it was on the old base
}

#[test]
fn one_revised_value_is_merged_not_called_a_rebase() {
    let old = s(&[
        ("2026-01", "100.0"),
        ("2026-02", "101.0"),
        ("2026-03", "102.0"),
        ("2026-04", "103.0"),
    ]);
    let new = s(&[
        ("2026-01", "100.0"),
        ("2026-02", "101.0"),
        ("2026-03", "102.4"),
        ("2026-04", "103.0"),
    ]);
    assert!(!is_rebased(&old, &new));
}

#[test]
fn rounding_noise_is_not_a_rebase() {
    let old = s(&[
        ("a", "1.0001"),
        ("b", "2.0002"),
        ("c", "3.0003"),
        ("d", "4.0004"),
    ]);
    let new = s(&[
        ("a", "1.0002"),
        ("b", "2.0001"),
        ("c", "3.0003"),
        ("d", "4.0005"),
    ]);
    assert!(!is_rebased(&old, &new));
}

#[test]
fn a_listed_postcode_is_replaced_and_its_empty_fields_keep_the_old_values() {
    // rows: [postcode, place, region]
    let row = |p: &str, place: &str, region: Option<&str>| -> Row {
        vec![
            Some(p.into()),
            Some(place.into()),
            region.map(str::to_string),
        ]
    };
    let old: Groups = [
        (
            "01945".to_string(),
            vec![
                row("01945", "Tettau", Some("Brandenburg")),
                row("01945", "Wrong", Some("X")),
            ],
        ),
        (
            "47877".to_string(),
            vec![row("47877", "Willich", Some("NRW"))],
        ),
    ]
    .into();
    let new: Groups = [("01945".to_string(), vec![row("01945", "Tettau", None)])].into();
    let merged = merge_groups(&old, &new, &[1]);
    assert_eq!(
        merged["01945"],
        vec![row("01945", "Tettau", Some("Brandenburg"))]
    );
    assert_eq!(merged["47877"], old["47877"]);
}

#[test]
fn series_and_groups_survive_the_table_form() {
    let set: SeriesSet = [
        (
            "EUR".to_string(),
            s(&[("2026-01-02", "0.95"), ("2026-01-05", "0.9512")]),
        ),
        ("JPY".to_string(), s(&[("2026-01-05", "157")])),
    ]
    .into();
    let t = series_to_table("date", &set, BTreeMap::new());
    assert_eq!(t.names, vec!["date", "EUR", "JPY"]);
    assert_eq!(t.rows(), 2);
    let back = table_to_series(&t, "date").unwrap();
    assert!(back["EUR"]["2026-01-05"].same_value(Exact::parse("0.9512").unwrap()));
    assert!(!back["JPY"].contains_key("2026-01-02"));

    let groups: Groups = [(
        "10115".to_string(),
        vec![vec![Some("10115".into()), Some("525323".into())]],
    )]
    .into();
    let t = groups_to_table(
        &["postcode", "lat"],
        &[Kind::Text, Kind::Int],
        &groups,
        BTreeMap::new(),
    );
    assert_eq!(t.columns[1], RefColumn::Int(vec![Some(525323)]));
    assert_eq!(
        table_to_groups(&t, |r| r[0].clone().unwrap_or_default()),
        groups
    );
}
