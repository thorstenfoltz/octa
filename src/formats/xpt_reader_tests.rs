use super::*;

fn pad(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.resize(bytes.len().div_ceil(RECORD) * RECORD, b' ');
    bytes
}

fn head(kind: &str, digits: &str) -> Vec<u8> {
    pad(format!("HEADER RECORD*******{kind:<8}HEADER RECORD!!!!!!!{digits:0<30}").into_bytes())
}

/// An IEEE double as an 8-byte IBM float.
fn ibm(v: f64) -> [u8; 8] {
    if v == 0.0 {
        return [0; 8];
    }
    let (mut a, mut exponent) = (v.abs(), 64u8);
    while a >= 1.0 {
        a /= 16.0;
        exponent += 1;
    }
    while a < 1.0 / 16.0 {
        a *= 16.0;
        exponent -= 1;
    }
    let mut out = ((a * 2f64.powi(56)) as u64).to_be_bytes();
    out[0] = exponent | if v < 0.0 { 0x80 } else { 0 };
    out
}

/// (name, numeric, length, SAS format)
type Var<'a> = (&'a str, bool, usize, &'a str);

fn member(name: &str, vars: &[Var], rows: &[Vec<u8>]) -> Vec<u8> {
    let mut out = head("MEMBER", "000000000000000001600000000140");
    out.extend(head("DSCRPTR", ""));
    out.extend(pad(
        format!("SAS     {name:<8}SASDATA 9.4     Linux").into_bytes()
    ));
    out.extend(pad(b"12NOV26:10:00:00".to_vec()));
    out.extend(head("NAMESTR", &format!("{:010}", vars.len())));
    let mut entries = Vec::new();
    let mut pos = 0u32;
    for (i, (name, numeric, len, format)) in vars.iter().enumerate() {
        let mut e = vec![0u8; 140];
        e[..2].copy_from_slice(&(if *numeric { 1u16 } else { 2 }).to_be_bytes());
        e[4..6].copy_from_slice(&(*len as u16).to_be_bytes());
        e[6..8].copy_from_slice(&(i as u16 + 1).to_be_bytes());
        e[8..16].copy_from_slice(format!("{name:<8}").as_bytes());
        e[16..56].fill(b' ');
        e[56..64].copy_from_slice(format!("{format:<8}").as_bytes());
        e[72..80].fill(b' ');
        e[84..88].copy_from_slice(&pos.to_be_bytes());
        pos += *len as u32;
        entries.extend(e);
    }
    out.extend(pad(entries));
    out.extend(head("OBS", ""));
    out.extend(pad(rows.concat()));
    out
}

fn library(members: &[Vec<u8>]) -> Vec<u8> {
    let mut out = head("LIBRARY", "");
    out.extend(pad(b"SAS     SAS     SASLIB  9.4     Linux".to_vec()));
    out.extend(pad(b"12NOV26:10:00:00".to_vec()));
    for m in members {
        out.extend_from_slice(m);
    }
    out
}

const MISSING: [u8; 8] = [b'.', 0, 0, 0, 0, 0, 0, 0];

fn two_datasets() -> Vec<u8> {
    let people = member(
        "PEOPLE",
        &[
            ("NAME", false, 6, ""),
            ("AGE", true, 8, ""),
            ("HEIGHT", true, 8, "BEST"),
            ("BORN", true, 8, "DATE"),
            ("SEEN", true, 8, "DATETIME"),
            ("ALARM", true, 8, "TIME"),
            ("SHORT", true, 3, ""),
        ],
        &[
            [
                &b"Ann   "[..],
                &ibm(34.0),
                &ibm(1.72),
                &ibm(0.0),                // 1960-01-01
                &ibm(86_400.0 + 3_661.0), // 1960-01-02 01:01:01
                &ibm(27_000.0),           // 07:30:00
                &ibm(2.0)[..3],
            ]
            .concat(),
            [
                &b"      "[..],
                &MISSING,
                &ibm(-1.5),
                &ibm(23_376.0), // 2024-01-01
                &MISSING,
                &[b'A', 0, 0, 0, 0, 0, 0, 0],
                &ibm(-7.0)[..3],
            ]
            .concat(),
        ],
    );
    let visits = member("VISITS", &[("ID", true, 8, "")], &[ibm(1.0).to_vec()]);
    library(&[people, visits])
}

fn write(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    (dir, path)
}

#[test]
fn ibm_floats_become_the_same_numbers() {
    assert_eq!(ibm_float(&[0x41, 0x10, 0, 0, 0, 0, 0, 0]), Some(1.0));
    assert_eq!(
        ibm_float(&[0xC2, 0x76, 0xA0, 0, 0, 0, 0, 0]),
        Some(-118.625)
    );
    assert_eq!(ibm_float(&[0; 8]), Some(0.0));
    // A number stored in fewer bytes is cut on the right.
    assert_eq!(ibm_float(&[0x41, 0x10]), Some(1.0));
    for v in [19.99, 1200.75, -0.001, 1e-20, 6.02e23, 123_456_789.125] {
        assert_eq!(ibm_float(&ibm(v)), Some(v), "{v}");
    }
}

#[test]
fn missing_numbers_are_empty() {
    assert_eq!(ibm_float(&MISSING), None);
    assert_eq!(ibm_float(&[b'_', 0, 0, 0]), None);
    assert_eq!(ibm_float(&[b'Z', 0, 0, 0, 0, 0, 0, 0]), None);
    // Not a missing value: a real number that happens to start with that byte.
    assert!(ibm_float(&[b'A', 0x10, 0, 0, 0, 0, 0, 0]).is_some());
}

#[test]
fn the_first_dataset_opens_with_typed_columns() {
    let (_dir, path) = write(&two_datasets(), "study.xpt");
    let t = XptReader.read_file(&path).unwrap();
    let types: Vec<(&str, &str)> = t
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.data_type.as_str()))
        .collect();
    assert_eq!(
        types,
        [
            ("NAME", "Utf8"),
            ("AGE", "Int64"),
            ("HEIGHT", "Float64"),
            ("BORN", "Date"),
            ("SEEN", "DateTime"),
            ("ALARM", "Time64(Microsecond)"),
            ("SHORT", "Int64"),
        ]
    );
    assert_eq!(
        t.rows[0],
        vec![
            CellValue::String("Ann".into()),
            CellValue::Int(34),
            CellValue::Float(1.72),
            CellValue::Date("1960-01-01".into()),
            CellValue::DateTime("1960-01-02 01:01:01".into()),
            CellValue::Time(27_000 * NANOS_PER_SEC),
            CellValue::Int(2),
        ]
    );
    assert_eq!(
        t.rows[1],
        vec![
            CellValue::Null,
            CellValue::Null,
            CellValue::Float(-1.5),
            CellValue::Date("2024-01-01".into()),
            CellValue::Null,
            CellValue::Null,
            CellValue::Int(-7),
        ]
    );
    // The blanks that fill the last 80-byte record are not rows.
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.format_name.as_deref(), Some("SAS transport"));
}

#[test]
fn several_datasets_are_listed_and_read_by_name() {
    let (_dir, path) = write(&two_datasets(), "study.xpt");
    let tables = XptReader.list_tables(&path).unwrap().unwrap();
    let listed: Vec<(&str, Option<usize>)> = tables
        .iter()
        .map(|t| (t.name.as_str(), t.row_count))
        .collect();
    assert_eq!(listed, [("PEOPLE", Some(2)), ("VISITS", Some(1))]);
    let visits = XptReader.read_table(&path, "VISITS").unwrap();
    assert_eq!(visits.rows, vec![vec![CellValue::Int(1)]]);
    assert!(XptReader.read_table(&path, "NOPE").is_err());
}

#[test]
fn one_dataset_needs_no_table_picker() {
    let one = library(&[member("ONLY", &[("ID", true, 8, "")], &[])]);
    let (_dir, path) = write(&one, "one.xpt");
    assert!(XptReader.list_tables(&path).unwrap().is_none());
    let t = XptReader.read_file(&path).unwrap();
    assert_eq!(t.columns[0].data_type, "Float64");
    assert!(t.rows.is_empty());
}

#[test]
fn format_names_decide_dates_and_times() {
    assert!(format_kind("DATETIME") == Kind::DateTime);
    assert!(format_kind("E8601DT") == Kind::DateTime);
    assert!(format_kind("DATE") == Kind::Date);
    assert!(format_kind("YYMMDD") == Kind::Date);
    assert!(format_kind("TIME") == Kind::Time);
    assert!(format_kind("HHMM") == Kind::Time);
    assert!(format_kind("BEST") == Kind::Number);
    assert!(format_kind("DOLLAR") == Kind::Number);
    assert!(format_kind("") == Kind::Number);
}

#[test]
fn a_file_that_is_not_a_transport_file_is_refused() {
    let (_dir, path) = write(b"just some text", "notes.xpt");
    let err = XptReader.read_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("not a SAS transport file"));
    let whole = two_datasets();
    let (_dir, path) = write(&whole[..500], "cut.xpt");
    let err = XptReader.read_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("cut short"), "{err:#}");
}

#[test]
fn the_registry_opens_a_transport_file() {
    let (_dir, path) = write(&two_datasets(), "study.xpt");
    let registry = crate::formats::FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    assert_eq!(reader.name(), "SAS transport");
    assert!(reader.supports_write());
    assert_eq!(reader.read_file(&path).unwrap().rows.len(), 2);
}

/// The header's digits are read as text; a damaged byte there can make a
/// wide character across the place the entry size starts, and slicing the
/// text by bytes there used to panic.
#[test]
fn a_wide_character_in_the_member_header_is_no_crash() {
    let mut one = member("ONLY", &[("ID", true, 8, "")], &[]);
    let damaged = head("MEMBER", &format!("{}é000", "0".repeat(25)));
    one[..RECORD].copy_from_slice(&damaged[..RECORD]);
    let (_dir, path) = write(&library(&[one]), "one.xpt");
    // The entry size falls back to the usual 140, which this file uses.
    assert!(XptReader.read_file(&path).is_ok());
}
