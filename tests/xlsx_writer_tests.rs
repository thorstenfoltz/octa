//! What the `.xlsx` writer puts in the file beyond the raw values: real dates,
//! clickable links, an autofilter, column widths and exported validation.
//!
//! Everything here is written on **every** save, not only when the formatting
//! switch is on, so the assertions run against a default `WriteOptions`.

use octa::data::validation::{ValidationKind, ValidationRule};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;
use octa::formats::write_options::{TableStyle, WriteOptions};

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    }
}

/// Write with the given options and hand back both the raw bytes and the
/// table Octa's own reader gets when it opens the result.
fn round_trip(table: &DataTable, opts: &WriteOptions) -> (Vec<u8>, DataTable) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("out.xlsx");
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&path).expect("xlsx reader");
    reader
        .write_file_with_options(&path, table, opts)
        .expect("write xlsx");
    let bytes = std::fs::read(&path).expect("read back");
    let read = reader.read_file(&path).expect("reopen xlsx");
    (bytes, read)
}

fn sheet_xml(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open xlsx zip");
    let mut file = zip
        .by_name("xl/worksheets/sheet1.xml")
        .expect("no sheet1 in workbook");
    let mut out = String::new();
    file.read_to_string(&mut out).expect("read sheet xml");
    out
}

/// The defect this replaces: `Date` and `DateTime` fell through to
/// `write_string`, so an exported date sorted and filtered as text in Excel.
#[test]
fn dates_are_written_as_dates_not_text() {
    let mut t = DataTable::empty();
    t.columns = vec![col("day", "Date32"), col("stamp", "Timestamp")];
    t.rows = vec![vec![
        CellValue::Date("2024-03-05".into()),
        CellValue::DateTime("2024-03-05 14:30:00".into()),
    ]];

    let (_, back) = round_trip(&t, &WriteOptions::default());

    assert!(
        matches!(
            back.get(0, 0),
            Some(CellValue::DateTime(_) | CellValue::Date(_))
        ),
        "the date came back as {:?}, not a date",
        back.get(0, 0)
    );
    assert!(
        matches!(
            back.get(0, 1),
            Some(CellValue::DateTime(_) | CellValue::Date(_))
        ),
        "the timestamp came back as {:?}, not a date",
        back.get(0, 1)
    );
}

/// One junk value in a date column must not cost the user the whole save.
#[test]
fn an_unparseable_date_falls_back_to_text() {
    let mut t = DataTable::empty();
    t.columns = vec![col("day", "Date32")];
    t.rows = vec![
        vec![CellValue::Date("2024-03-05".into())],
        vec![CellValue::Date("not a date at all".into())],
    ];

    let (_, back) = round_trip(&t, &WriteOptions::default());

    assert_eq!(back.row_count(), 2, "the save kept both rows");
    assert_eq!(
        back.get(1, 0).map(|v| v.to_string()),
        Some("not a date at all".to_string())
    );
}

/// A bare web address becomes a link, and the header row plus the autofilter
/// are there whether or not the formatting switch is on.
#[test]
fn links_header_and_autofilter_are_always_written() {
    let mut t = DataTable::empty();
    t.columns = vec![col("site", "Utf8"), col("note", "Utf8")];
    t.rows = vec![vec![
        CellValue::String("https://example.com/data".into()),
        CellValue::String("not a link".into()),
    ]];

    let (bytes, back) = round_trip(&t, &WriteOptions::default());
    let xml = sheet_xml(&bytes);

    assert!(xml.contains("<autoFilter"), "no autofilter in:\n{xml}");
    assert!(xml.contains("<hyperlink"), "no hyperlink in:\n{xml}");
    assert_eq!(
        back.get(0, 0).map(|v| v.to_string()),
        Some("https://example.com/data".to_string()),
        "the link text still reads back as the address"
    );
}

/// A validation rule Excel can express reaches the file as real validation,
/// so the workbook rejects bad input rather than only colouring it in Octa.
#[test]
fn validation_rules_are_exported() {
    let mut t = DataTable::empty();
    t.columns = vec![col("amount", "Float64")];
    t.rows = vec![vec![CellValue::Float(12.0)]];

    let opts = WriteOptions {
        style: Some(TableStyle {
            validation: vec![ValidationRule {
                column: Some(0),
                kind: ValidationKind::Range {
                    min: Some(0.0),
                    max: Some(100.0),
                },
            }],
            ..TableStyle::default()
        }),
        ..WriteOptions::default()
    };
    let (bytes, _) = round_trip(&t, &opts);
    let xml = sheet_xml(&bytes);

    assert!(
        xml.contains("<dataValidation"),
        "no data validation in:\n{xml}"
    );
}

/// The mapping decisions, without producing a workbook: what Excel cannot
/// express is skipped rather than approximated.
#[test]
fn validation_kinds_excel_cannot_express_are_skipped() {
    use octa::formats::xlsx_style::{XlsxValidation, map_validation};

    let map = |kind| {
        map_validation(&ValidationRule {
            column: Some(0),
            kind,
        })
    };

    assert_eq!(
        map(ValidationKind::Range {
            min: Some(1.0),
            max: Some(9.0)
        }),
        Some(XlsxValidation::Decimal {
            min: Some(1.0),
            max: Some(9.0)
        })
    );
    assert_eq!(
        map(ValidationKind::Range {
            min: None,
            max: Some(9.0)
        }),
        Some(XlsxValidation::Decimal {
            min: None,
            max: Some(9.0)
        }),
        "an open lower bound is still a real constraint"
    );
    assert_eq!(
        map(ValidationKind::Range {
            min: None,
            max: None
        }),
        None,
        "a range with no bounds constrains nothing"
    );
    assert_eq!(map(ValidationKind::NotNull), Some(XlsxValidation::NotBlank));
    assert_eq!(
        map(ValidationKind::MaxLength(20)),
        Some(XlsxValidation::MaxLength(20))
    );
    // No Excel counterpart: skipped, not approximated.
    assert_eq!(map(ValidationKind::Regex("^a".into())), None);
    assert_eq!(map(ValidationKind::Unique), None);
}

#[test]
fn excel_time_and_hours_cells_read_typed_and_write_back() {
    use rust_xlsxwriter::{Format, Workbook};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.xlsx");
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_string(0, 0, "clock").unwrap();
    ws.write_string(0, 1, "worked").unwrap();
    ws.write_number_with_format(1, 0, 14.5 / 24.0, &Format::new().set_num_format("hh:mm:ss"))
        .unwrap();
    ws.write_number_with_format(
        1,
        1,
        30.25 / 24.0,
        &Format::new().set_num_format("[h]:mm:ss"),
    )
    .unwrap();
    wb.save(&path).unwrap();
    let t = octa::formats::read_table_auto(&path, None, u64::MAX).unwrap();
    let h = 3_600_000_000_000i64;
    assert_eq!(t.rows[0][0], CellValue::Time(14 * h + h / 2));
    assert_eq!(t.rows[0][1], CellValue::Duration(30 * h + h / 4));
    assert!(octa::data::times::TimeKind::of_type(&t.columns[0].data_type).is_some());

    let out = dir.path().join("out.xlsx");
    let mut t2 = t.clone();
    t2.rows.push(vec![CellValue::Null, CellValue::Duration(-h)]);
    octa::formats::FormatReader::write_file(&octa::formats::excel_reader::ExcelReader, &out, &t2)
        .unwrap();
    let back = octa::formats::read_table_auto(&out, None, u64::MAX).unwrap();
    assert_eq!(back.rows[0], t.rows[0]);
    assert_eq!(
        back.rows[1][1],
        CellValue::String("-01:00:00".into()),
        "Excel cannot show a negative time"
    );
}
