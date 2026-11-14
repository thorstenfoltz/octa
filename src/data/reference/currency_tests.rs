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

fn rates() -> Rates {
    let daily: SeriesSet = [
        (
            "EUR".to_string(),
            s(&[("2026-01-02", "0.95"), ("2026-01-05", "0.9512")]),
        ),
        ("JPY".to_string(), s(&[("2026-01-05", "157.10")])),
    ]
    .into();
    let monthly: SeriesSet = [("ARS".to_string(), s(&[("2026-01", "1050.25")]))].into();
    Rates::from_sets(&daily, &monthly)
}

fn table(rows: &[(&str, &str, &str)]) -> DataTable {
    DataTable {
        columns: ["amount", "currency", "date"]
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: rows
            .iter()
            .map(|(a, c, d)| {
                vec![
                    CellValue::String(a.to_string()),
                    CellValue::String(c.to_string()),
                    CellValue::String(d.to_string()),
                ]
            })
            .collect(),
        ..DataTable::empty()
    }
}

fn run(
    rows: &[(&str, &str, &str)],
    to: &str,
    rounding: Rounding,
) -> Vec<(Option<String>, Option<String>)> {
    let spec = CurrencySpec {
        amount: 0,
        from: Pick::Column(1),
        to: to.into(),
        date: Some(2),
        rounding,
        mark: Mark::Code,
    };
    let c = currency_columns(&table(rows), &rates(), &spec).unwrap();
    assert_eq!(c.columns[0].name, format!("amount_{to}"));
    assert_eq!(c.columns[1].name, format!("amount_{to}_currency"));
    assert_eq!(c.columns[2].name, format!("amount_{to}_rate_date"));
    c.columns[0]
        .values
        .iter()
        .zip(&c.columns[2].values)
        .map(|(v, n)| (cell_text(v), cell_text(n)))
        .collect()
}

fn one(v: &str, n: &str) -> (Option<String>, Option<String>) {
    (Some(v.into()), Some(n.into()))
}

#[test]
fn a_weekend_uses_the_last_rate_before_it() {
    assert_eq!(
        run(&[("100", "EUR", "2026-01-03")], "USD", Rounding::Usual),
        vec![one("105.26", "2026-01-02")]
    );
}

#[test]
fn a_cross_rate_goes_through_the_dollar_and_rounds_per_currency() {
    assert_eq!(
        run(&[("100", "EUR", "2026-01-05")], "JPY", Rounding::Usual),
        vec![one("16516", "2026-01-05")]
    );
}

#[test]
fn a_currency_without_daily_rates_uses_the_monthly_average() {
    assert_eq!(
        run(&[("1050.25", "ARS", "2026-01-20")], "USD", Rounding::Usual),
        vec![one("1", "2026-01 (monthly average)")]
    );
}

#[test]
fn after_the_newest_rate_the_newest_is_used_and_named() {
    assert_eq!(
        run(&[("100", "EUR", "2026-02-10")], "USD", Rounding::Usual)[0]
            .1
            .as_deref(),
        Some("2026-01-05")
    );
}

#[test]
fn misses_stay_empty_and_say_why() {
    let out = run(
        &[
            ("100", "EUR", "2025-12-01"),
            ("100", "XXX", "2026-01-05"),
            ("100", "EUR", "soon"),
            ("", "EUR", "2026-01-05"),
        ],
        "USD",
        Rounding::Usual,
    );
    assert_eq!(out[0], (None, Some("no rate before 2026-01-02".into())));
    assert_eq!(out[1], (None, Some("unknown currency XXX".into())));
    assert_eq!(out[2], (None, Some("not a date".into())));
    assert_eq!(out[3], (None, None));
}

#[test]
fn rounding_is_the_users_choice() {
    assert_eq!(
        run(
            &[("100", "EUR", "2026-01-02")],
            "USD",
            Rounding::Decimals(4)
        )[0]
        .0
        .as_deref(),
        Some("105.2632")
    );
    assert_eq!(
        run(
            &[("100", "EUR", "2026-01-02")],
            "USD",
            Rounding::Decimals(0)
        )[0]
        .0
        .as_deref(),
        Some("105")
    );
    assert_eq!(
        run(&[("1", "EUR", "2026-01-02")], "USD", Rounding::None)[0]
            .0
            .as_deref(),
        Some("1.05263157894736842105")
    );
}

#[test]
fn the_same_currency_is_copied_unchanged() {
    assert_eq!(
        run(&[("19.999", "USD", "2026-01-05")], "USD", Rounding::Usual),
        vec![one("19.999", "same currency")]
    );
}

#[test]
fn symbols_and_case_are_read() {
    assert_eq!(code_of("€", &rates()).as_deref(), Some("EUR"));
    assert_eq!(code_of(" jpy ", &rates()).as_deref(), Some("JPY"));
    assert_eq!(code_of("$", &rates()).as_deref(), Some("USD"));
}

#[test]
fn an_unknown_target_is_refused_before_any_row() {
    let spec = CurrencySpec {
        amount: 0,
        from: Pick::Column(1),
        to: "ZZZ".into(),
        date: Some(2),
        rounding: Rounding::Usual,
        mark: Mark::Code,
    };
    assert!(currency_columns(&table(&[]), &rates(), &spec).is_err());
}

#[test]
fn latest_rate_ignores_the_date() {
    let spec = CurrencySpec {
        amount: 0,
        from: Pick::Fixed("EUR".into()),
        to: "USD".into(),
        date: None,
        rounding: Rounding::Usual,
        mark: Mark::Code,
    };
    let c = currency_columns(&table(&[("95.12", "x", "x")]), &rates(), &spec).unwrap();
    assert_eq!(cell_text(&c.columns[0].values[0]).as_deref(), Some("100"));
    assert_eq!(
        cell_text(&c.columns[2].values[0]).as_deref(),
        Some("2026-01-05")
    );
}

#[test]
fn the_currency_column_shows_the_code_or_an_unshared_symbol() {
    let mark = |to: &str, mark: Mark| {
        let spec = CurrencySpec {
            amount: 0,
            from: Pick::Fixed("EUR".into()),
            to: to.into(),
            date: Some(2),
            rounding: Rounding::Usual,
            mark,
        };
        let c = currency_columns(
            &table(&[("1", "x", "2026-01-05"), ("", "x", "2026-01-05")]),
            &rates(),
            &spec,
        )
        .unwrap();
        (
            cell_text(&c.columns[1].values[0]),
            cell_text(&c.columns[1].values[1]),
        )
    };
    assert_eq!(
        mark("USD", Mark::Code),
        (Some("USD".into()), None),
        "no amount, no mark"
    );
    assert_eq!(mark("EUR", Mark::Symbol).0.as_deref(), Some("€"));
    assert_eq!(
        mark("USD", Mark::Symbol).0.as_deref(),
        Some("USD"),
        "$ is shared"
    );
    assert_eq!(
        mark("JPY", Mark::Symbol).0.as_deref(),
        Some("JPY"),
        "¥ is shared with the yuan"
    );
    assert_eq!(symbol("THB"), None, "the table font cannot draw the baht");
    assert_eq!(symbol("PLN"), Some("zł"));
    assert_eq!(Mark::parse("SYMBOL"), Some(Mark::Symbol));
}
