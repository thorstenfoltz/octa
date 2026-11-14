use super::*;
use crate::data::decimal::Exact;
use crate::data::{ColumnInfo, DataTable};
use crate::reference::merge::{Series, SeriesSet};

fn s(pairs: &[(&str, &str)]) -> Series {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Exact::parse(v).unwrap()))
        .collect()
}

fn idx() -> Indices {
    let set: SeriesSet = [
        (
            "DE".to_string(),
            s(&[
                ("2015-01", "100.0"),
                ("2020-01", "110.0"),
                ("2026-01", "132.0"),
            ]),
        ),
        (
            "XM".to_string(),
            s(&[("2015-01", "100.0"), ("2025-06", "125.0")]),
        ),
    ]
    .into();
    Indices::from_set(&set)
}

fn table(rows: &[(&str, &str, &str)]) -> DataTable {
    DataTable {
        columns: ["amount", "date", "country"]
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: rows
            .iter()
            .map(|(a, d, c)| {
                vec![
                    CellValue::String(a.to_string()),
                    CellValue::String(d.to_string()),
                    CellValue::String(c.to_string()),
                ]
            })
            .collect(),
        ..DataTable::empty()
    }
}

fn spec(target: Option<&str>, rounding: Rounding) -> InflationSpec {
    InflationSpec {
        amount: 0,
        date: Pick::Column(1),
        country: Pick::Column(2),
        target: target.map(|t| NaiveDate::parse_from_str(&format!("{t}-01"), "%Y-%m-%d").unwrap()),
        rounding,
    }
}

#[test]
fn an_amount_is_scaled_by_the_index_ratio() {
    let c = inflation_columns(
        &table(&[("100", "2015-01-15", "Germany")]),
        &idx(),
        &spec(Some("2026-01"), Rounding::Decimals(2)),
    )
    .unwrap();
    assert_eq!(c.columns[0].name, "amount_in_2026-01");
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("132"));
    assert_eq!(
        cell_text(&c.columns[1].values[0]).as_deref(),
        Some("2015-01 to 2026-01")
    );
}

#[test]
fn the_default_target_is_the_newest_month_every_country_has() {
    let t = table(&[
        ("100", "2015-01-15", "DE"),
        ("100", "2015-03-01", "euro area"),
    ]);
    let c = inflation_columns(&t, &idx(), &spec(None, Rounding::Decimals(2))).unwrap();
    assert_eq!(c.columns[0].name, "amount_in_2025-06");
    // Germany's index on 2025-06 is the last one published before it: 2020-01.
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("110"));
    assert_eq!(
        cell_text(&c.columns[1].values[1]).as_deref(),
        Some("2015-01 to 2025-06")
    );
}

#[test]
fn a_country_whose_index_ends_earlier_uses_its_own_newest_month() {
    let c = inflation_columns(
        &table(&[("100", "2015-01-15", "XM")]),
        &idx(),
        &spec(Some("2026-01"), Rounding::Decimals(2)),
    )
    .unwrap();
    assert_eq!(
        cell_text(&c.columns[1].values[0]).as_deref(),
        Some("2015-01 to 2025-06")
    );
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("125"));
}

#[test]
fn misses_say_why() {
    let t = table(&[
        ("100", "2010-01-01", "DE"),
        ("100", "2016-01-01", "Atlantis"),
        ("100", "never", "DE"),
    ]);
    let c = inflation_columns(&t, &idx(), &spec(Some("2026-01"), Rounding::Decimals(2))).unwrap();
    let notes: Vec<_> = c.columns[1].values.iter().map(cell_text).collect();
    assert_eq!(notes[0].as_deref(), Some("no index before 2015-01"));
    assert_eq!(notes[1].as_deref(), Some("no index for Atlantis"));
    assert_eq!(notes[2].as_deref(), Some("not a date"));
    assert_eq!(c.missed, 3);
}

#[test]
fn not_rounding_keeps_the_digits() {
    let c = inflation_columns(
        &table(&[("10", "2020-01-01", "DE")]),
        &idx(),
        &spec(Some("2026-01"), Rounding::None),
    )
    .unwrap();
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("12"));
    let c = inflation_columns(
        &table(&[("1", "2020-01-01", "DE")]),
        &idx(),
        &spec(Some("2026-01"), Rounding::None),
    )
    .unwrap();
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("1.2"));
}

#[test]
fn the_euro_area_has_several_spellings() {
    for t in ["euro area", "Eurozone", "XM", "ea"] {
        assert_eq!(area_of(t).as_deref(), Some("XM"), "{t}");
    }
}

#[test]
fn one_month_for_every_row_needs_no_date_column() {
    let t = table(&[("100", "", "Germany")]);
    let fixed = InflationSpec {
        date: Pick::Fixed("2015-01".into()),
        ..spec(Some("2026-01"), Rounding::Decimals(2))
    };
    let c = inflation_columns(&t, &idx(), &fixed).unwrap();
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("132"));
    let bad = InflationSpec {
        date: Pick::Fixed("soon".into()),
        ..fixed
    };
    assert!(inflation_columns(&t, &idx(), &bad).is_err());
}
