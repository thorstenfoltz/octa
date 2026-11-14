use super::*;

#[test]
fn rounding_reads_its_three_spellings() {
    assert_eq!(Rounding::parse("usual"), Some(Rounding::Usual));
    assert_eq!(Rounding::parse("NONE"), Some(Rounding::None));
    assert_eq!(Rounding::parse("3"), Some(Rounding::Decimals(3)));
    assert_eq!(Rounding::parse("-1"), None);
    assert_eq!(Rounding::parse("x"), None);
}

#[test]
fn usual_decimals_follow_the_currency() {
    assert_eq!(
        (minor_units("EUR"), minor_units("JPY"), minor_units("KWD")),
        (2, 0, 3)
    );
}

#[test]
fn several_values_join_sorted_or_give_the_first() {
    let v = || {
        vec![
            "Tettau".to_string(),
            "Guteborn".to_string(),
            "Tettau".to_string(),
        ]
    };
    assert_eq!(join(v(), Several::All).as_deref(), Some("Guteborn, Tettau"));
    assert_eq!(join(v(), Several::First).as_deref(), Some("Guteborn"));
    assert_eq!(join(Vec::<String>::new(), Several::All), None);
}

#[test]
fn countries_are_read_by_code_or_name() {
    assert_eq!(country_code("de").as_deref(), Some("DE"));
    assert_eq!(country_code("EL").as_deref(), Some("GR"));
    assert_eq!(country_code("Germany").as_deref(), Some("DE"));
    assert_eq!(country_code("France").as_deref(), Some("FR"));
    assert_eq!(country_code("Atlantis"), None);
}

#[test]
fn amounts_are_exact_and_european_text_is_read() {
    assert_eq!(
        amount(&CellValue::String("1.234,56".into()))
            .unwrap()
            .text(),
        "1234.56"
    );
    assert_eq!(amount(&CellValue::Int(5)).unwrap().text(), "5");
    assert!(amount(&CellValue::String("abc".into())).is_none());
}

#[test]
fn inserted_columns_land_after_the_source_and_dodge_taken_names() {
    let mut t = crate::data::DataTable {
        columns: vec![
            crate::data::ColumnInfo {
                name: "price".into(),
                data_type: "Float64".into(),
            },
            crate::data::ColumnInfo {
                name: "price_USD".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![vec![CellValue::Float(1.0), CellValue::Null]],
        ..crate::data::DataTable::empty()
    };
    let c = Computed {
        after: 0,
        columns: vec![NewColumn::numbers(
            "price_USD".into(),
            vec![CellValue::Float(1.1)],
        )],
        found: 1,
        missed: 0,
    };
    let names = insert_columns(&mut t, &c);
    assert_eq!(names, vec!["price_USD_2".to_string()]);
    assert_eq!(t.columns[1].name, "price_USD_2");
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(1.1)));
}
