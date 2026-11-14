use super::*;
use std::sync::Arc;

#[test]
fn eurostat_codes_become_iso() {
    assert_eq!(iso2("EL"), "GR");
    assert_eq!(iso2("UK"), "GB");
    assert_eq!(iso2("DE"), "DE");
}

#[test]
fn the_newest_dataset_and_workbook_are_found() {
    let json = r#"{"pcode-2020":{},"pcode-2025":{},"pcode-2024":{}}"#;
    assert_eq!(newest_year(json, "pcode"), Some(2025));
    let page = r#"<a href="/eurostat/documents/345175/501971/EU-27-LAU-2024-NUTS-2024.xlsx/x?t=1">a</a>
                  <a href="/eurostat/documents/345175/501971/EU-27-LAU-2025-NUTS-2024.xlsx/y?t=2">b</a>
                  <a href="/eurostat/documents/345175/501971/EU-27-LAU-2023-NUTS-2021.xlsx">c</a>"#;
    assert_eq!(
        newest_workbook(page).as_deref(),
        Some("EU-27-LAU-2025-NUTS-2024.xlsx")
    );
}

#[test]
fn postcode_points_lose_their_quotes_and_prefixes() {
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pc.parquet");
    let schema = Arc::new(Schema::new(vec![
        Field::new("POSTCODE", DataType::Utf8, true),
        Field::new("CNTR_ID", DataType::Utf8, true),
        Field::new("NUTS3_2024", DataType::Utf8, true),
        Field::new("GISCO_2021", DataType::Utf8, true),
        Field::new("LAU_NAME", DataType::Utf8, true),
    ]));
    let col = |v: &[&str]| Arc::new(StringArray::from(v.to_vec())) as arrow::array::ArrayRef;
    let batch = arrow::record_batch::RecordBatch::try_new(
        schema.clone(),
        vec![
            col(&["47877", "10553"]),
            col(&["DE", "EL"]),
            col(&["\"DEA1E\"", "\"EL303\""]),
            col(&["DE_05166036", "EL_45010000"]),
            col(&["Willich, Stadt", "Athina"]),
        ],
    )
    .unwrap();
    let mut w =
        parquet::arrow::ArrowWriter::try_new(std::fs::File::create(&path).unwrap(), schema, None)
            .unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();
    let g = parse_postcodes(&path).unwrap();
    let row = &g["DE|47877"][0];
    assert_eq!(
        row,
        &vec![
            Some("DE".into()),
            Some("47877".into()),
            Some("DEA1E".into()),
            Some("05166036".into())
        ]
    );
    assert!(g.contains_key("GR|10553"));
}

#[test]
fn the_workbook_gives_every_municipality_its_nuts_region() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lau.xlsx");
    let mut wb = rust_xlsxwriter::Workbook::new();
    let info = wb.add_worksheet().set_name("Overview").unwrap();
    info.write(0, 0, "not a country").unwrap();
    let de = wb.add_worksheet().set_name("DE").unwrap();
    for (c, h) in [
        "PERIOD",
        "NUTS3",
        "LAU CODE",
        "EU LAU CODE",
        "LAU NAME NATIONAL",
        "LAU NAME LATIN",
    ]
    .iter()
    .enumerate()
    {
        de.write(0, c as u16, *h).unwrap();
    }
    for (c, v) in [
        "2025",
        "DEA1E",
        "05166036",
        "DE_05166036",
        "Willich, Stadt",
        "Willich, Stadt",
    ]
    .iter()
    .enumerate()
    {
        de.write(1, c as u16, *v).unwrap();
    }
    let el = wb.add_worksheet().set_name("EL").unwrap();
    for (c, h) in [
        "NUTS3",
        "EU LAU CODE",
        "LAU NAME NATIONAL",
        "LAU NAME LATIN",
    ]
    .iter()
    .enumerate()
    {
        el.write(0, c as u16, *h).unwrap();
    }
    for (c, v) in ["EL303", "EL_45010000", "Αθηναίων", "Athinaion"]
        .iter()
        .enumerate()
    {
        el.write(1, c as u16, *v).unwrap();
    }
    wb.save(&path).unwrap();
    let g = parse_workbook(&path).unwrap();
    assert_eq!(g["DE|05166036"][0][4].as_deref(), Some("DEA1E"));
    assert_eq!(g["DE|05166036"][0][3], None, "equal names are stored once");
    assert_eq!(g["GR|45010000"][0][3].as_deref(), Some("Athinaion"));
    assert_eq!(g.len(), 2, "the overview sheet is not a country");
    let bare = dir.path().join("download");
    std::fs::copy(&path, &bare).unwrap();
    assert_eq!(parse_workbook(&bare).unwrap().len(), 2);
}

#[test]
fn nuts_names_are_read_latin() {
    let csv = "CNTR_CODE,NUTS_ID,NAME_LATN,NUTS_NAME,MOUNT_TYPE,URBN_TYPE,COAST_TYPE\nDE,DE2,Bayern,Bayern,0,0,0\nEL,EL3,Attiki,Αττική ,0,0,0\n";
    let g = parse_nuts(csv.as_bytes()).unwrap();
    assert_eq!(g["EL3"][0][1].as_deref(), Some("Attiki"));
}
