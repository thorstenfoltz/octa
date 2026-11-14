use super::*;

fn int(i: i32) -> Vec<u8> {
    [&[1u8][..], &i.to_le_bytes()].concat()
}

fn float(f: f64) -> Vec<u8> {
    [&[2u8][..], &f.to_le_bytes()].concat()
}

fn text(s: &str) -> Vec<u8> {
    [&[4u8][..], s.as_bytes(), &[0]].concat()
}

fn dual_int(i: i32, s: &str) -> Vec<u8> {
    [&[5u8][..], &i.to_le_bytes(), s.as_bytes(), &[0]].concat()
}

fn dual_float(f: f64, s: &str) -> Vec<u8> {
    [&[6u8][..], &f.to_le_bytes(), s.as_bytes(), &[0]].concat()
}

/// One field of a hand-built file: name, number format, tags, its symbols.
type TestField<'a> = (&'a str, &'a str, &'a [&'a str], Vec<Vec<u8>>);

/// Builds a QVD the way Qlik lays it out. `rows` name a symbol per field,
/// `None` is an empty cell (stored through a bias of -2, as Qlik does).
fn qvd(fields: &[TestField], rows: &[Vec<Option<usize>>]) -> Vec<u8> {
    let mut symbols = Vec::new();
    let mut headers = String::new();
    let mut layout = Vec::new(); // (bit offset, bit width, bias)
    let mut bit = 0usize;
    for (col, (name, format, tags, values)) in fields.iter().enumerate() {
        let bias: i64 = if rows.iter().any(|r| r[col].is_none()) {
            -2
        } else {
            0
        };
        let max = rows
            .iter()
            .map(|r| r[col].map_or(0, |i| i as i64 - bias))
            .max()
            .unwrap_or(0);
        let width = (64 - max.leading_zeros()) as usize;
        let offset = symbols.len();
        for v in values {
            symbols.extend_from_slice(v);
        }
        let tags: String = tags
            .iter()
            .map(|t| format!("<String>{t}</String>"))
            .collect();
        headers.push_str(&format!(
            "<QvdFieldHeader><FieldName>{name}</FieldName><BitOffset>{bit}</BitOffset>\
             <BitWidth>{width}</BitWidth><Bias>{bias}</Bias>\
             <NumberFormat><Type>{format}</Type></NumberFormat>\
             <NoOfSymbols>{}</NoOfSymbols><Offset>{offset}</Offset><Length>{}</Length>\
             <Tags>{tags}</Tags></QvdFieldHeader>",
            values.len(),
            symbols.len() - offset,
        ));
        layout.push((bit, width, bias));
        bit += width;
    }
    let record_size = bit.div_ceil(8);
    let mut index = Vec::new();
    for row in rows {
        let mut packed = 0u128;
        for (cell, (offset, _, bias)) in row.iter().zip(&layout) {
            let stored = cell.map_or(0, |i| i as i64 - bias) as u128;
            packed |= stored << offset;
        }
        index.extend_from_slice(&packed.to_le_bytes()[..record_size]);
    }
    let header = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n\
         <QvdTableHeader><TableName>t</TableName><Fields>{headers}</Fields>\
         <RecordByteSize>{record_size}</RecordByteSize><NoOfRecords>{}</NoOfRecords>\
         <Offset>{}</Offset><Length>{}</Length></QvdTableHeader>\r\n",
        rows.len(),
        symbols.len(),
        index.len(),
    );
    [header.as_bytes(), &[0], &symbols, &index].concat()
}

fn shop() -> Vec<u8> {
    qvd(
        &[
            ("id", "INTEGER", &[], vec![int(1), int(2), int(300)]),
            ("name", "ASCII", &[], vec![text("Ann"), text("Zo\u{eb}")]),
            ("price", "REAL", &[], vec![float(9.5), int(3)]),
            (
                "day",
                "DATE",
                &[],
                vec![dual_int(45_000, "15/03/2023"), int(25_569)],
            ),
            (
                "stamp",
                "TIMESTAMP",
                &[],
                vec![dual_float(45_000.5, "15/03/2023 12:00")],
            ),
            ("clock", "TIME", &[], vec![float(0.25)]),
            ("code", "UNKNOWN", &[], vec![dual_int(7, "007"), int(12)]),
        ],
        &[
            vec![
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
            ],
            vec![Some(1), Some(1), Some(1), Some(1), None, Some(0), Some(1)],
            vec![Some(2), None, None, None, None, Some(0), Some(0)],
        ],
    )
}

#[test]
fn columns_take_their_types() {
    let t = read_bytes(&shop()).unwrap();
    let types: Vec<(&str, &str)> = t
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.data_type.as_str()))
        .collect();
    assert_eq!(
        types,
        [
            ("id", "Int64"),
            ("name", "Utf8"),
            ("price", "Float64"),
            ("day", "Date"),
            ("stamp", "DateTime"),
            ("clock", "Time64(Microsecond)"),
            ("code", "Utf8"),
        ]
    );
    assert_eq!(t.format_name.as_deref(), Some("QVD"));
}

#[test]
fn rows_read_their_values_and_empty_cells() {
    let t = read_bytes(&shop()).unwrap();
    assert_eq!(
        t.rows[0],
        vec![
            CellValue::Int(1),
            CellValue::String("Ann".into()),
            CellValue::Float(9.5),
            CellValue::Date("2023-03-15".into()),
            CellValue::DateTime("2023-03-15 12:00:00".into()),
            CellValue::Time(6 * 3_600 * NANOS_PER_SEC),
            CellValue::String("007".into()),
        ]
    );
    assert_eq!(
        t.rows[1],
        vec![
            CellValue::Int(2),
            CellValue::String("Zo\u{eb}".into()),
            CellValue::Float(3.0),
            CellValue::Date("1970-01-01".into()),
            CellValue::Null,
            CellValue::Time(6 * 3_600 * NANOS_PER_SEC),
            CellValue::String("12".into()),
        ]
    );
    assert_eq!(t.rows[2][0], CellValue::Int(300));
    assert!(t.rows[2][1..5].iter().all(|c| *c == CellValue::Null));
}

#[test]
fn tags_mark_dates_when_the_format_is_unknown() {
    let bytes = qvd(
        &[
            ("d", "UNKNOWN", &["$numeric", "$date"], vec![int(45_000)]),
            (
                "ts",
                "UNKNOWN",
                &["$timestamp", "$date"],
                vec![float(45_000.75)],
            ),
        ],
        &[vec![Some(0), Some(0)]],
    );
    let t = read_bytes(&bytes).unwrap();
    assert_eq!(
        t.rows[0],
        vec![
            CellValue::Date("2023-03-15".into()),
            CellValue::DateTime("2023-03-15 18:00:00".into()),
        ]
    );
}

#[test]
fn an_interval_longer_than_a_day_stays_a_duration() {
    let bytes = qvd(
        &[("took", "INTERVAL", &[], vec![float(1.5)])],
        &[vec![Some(0)]],
    );
    let t = read_bytes(&bytes).unwrap();
    assert_eq!(t.columns[0].data_type, "Duration(s)");
    assert_eq!(
        t.rows[0][0],
        CellValue::Duration(36 * 3_600 * NANOS_PER_SEC)
    );
}

#[test]
fn a_file_with_no_rows_keeps_its_columns() {
    let bytes = qvd(&[("id", "INTEGER", &[], vec![])], &[]);
    let t = read_bytes(&bytes).unwrap();
    assert_eq!(t.columns.len(), 1);
    assert!(t.rows.is_empty());
}

#[test]
fn a_cut_file_is_refused_not_half_read() {
    let whole = shop();
    let err = read_bytes(&whole[..whole.len() - 2]).unwrap_err();
    assert!(format!("{err:#}").contains("cut short"), "{err:#}");
    assert!(read_bytes(b"not a qvd").is_err());
}

#[test]
fn the_registry_opens_a_qvd_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shop.qvd");
    std::fs::write(&path, shop()).unwrap();
    let registry = crate::formats::FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    assert_eq!(reader.name(), "QVD");
    assert!(!reader.supports_write());
    assert_eq!(reader.read_file(&path).unwrap().rows.len(), 3);
}
