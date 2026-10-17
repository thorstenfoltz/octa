use super::*;
use crate::data::ColumnInfo;

fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows;
    t
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

fn spec(cols: Vec<usize>, algo: HashColumnsAlgo) -> HashColumnsSpec {
    HashColumnsSpec {
        columns: cols,
        algo,
        ..HashColumnsSpec::default()
    }
}

/// Known answers, checked against `printf 'a|b' | md5sum` and friends.
#[test]
fn known_digests() {
    let t = table(&["x", "y"], vec![vec![s("a"), s("b")]]);
    let h = |algo| hash_columns(&t, &spec(vec![0, 1], algo))[0].to_string();
    assert_eq!(h(HashColumnsAlgo::Md5), "d0726241020676b14aa6298ce6a18b21");
    assert_eq!(
        h(HashColumnsAlgo::Sha256),
        "0eab8a0a3380abf4c7d1fb0b43b66aafbb64a4b953e4eb2dccca579461912d0c"
    );
    let sha512 = h(HashColumnsAlgo::Sha512);
    assert_eq!(sha512.len(), 128);
    assert!(sha512.starts_with("3c075e5f72e4ec6eeaf0"), "{sha512}");
}

#[test]
fn the_empty_string_hashes_to_the_textbook_value() {
    let t = table(&["x"], vec![vec![CellValue::Null]]);
    let out = hash_columns(&t, &spec(vec![0], HashColumnsAlgo::Sha256));
    assert_eq!(
        out[0].to_string(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn order_delimiter_null_trim_and_upper_shape_the_input() {
    let t = table(
        &["a", "b", "c"],
        vec![vec![s(" x "), CellValue::Null, s("y")]],
    );
    let mut sp = spec(vec![2, 0, 1], HashColumnsAlgo::Md5);
    assert_eq!(row_input(&t, 0, &sp), "y| x |");
    sp.delimiter = ";;".into();
    sp.null_text = "<NULL>".into();
    sp.trim = true;
    sp.upper = true;
    assert_eq!(row_input(&t, 0, &sp), "Y;;X;;<NULL>");
    sp.delimiter.clear();
    assert_eq!(row_input(&t, 0, &sp), "YX<NULL>");
}

/// Any type goes in as its display text.
#[test]
fn mixed_types_are_hashed_through_their_text() {
    let t = table(
        &["i", "f", "d", "b", "n"],
        vec![vec![
            CellValue::Int(42),
            CellValue::Float(1.5),
            CellValue::Date("2026-10-01".into()),
            CellValue::Bool(true),
            CellValue::Null,
        ]],
    );
    let sp = spec(vec![0, 1, 2, 3, 4], HashColumnsAlgo::Md5);
    assert_eq!(row_input(&t, 0, &sp), "42|1.5|2026-10-01|true|");
    assert_eq!(hash_columns(&t, &sp)[0].to_string().len(), 32);
}

#[test]
fn algo_names_parse_with_or_without_the_dash() {
    assert_eq!(
        HashColumnsAlgo::parse("SHA-256"),
        Some(HashColumnsAlgo::Sha256)
    );
    assert_eq!(
        HashColumnsAlgo::parse("sha512"),
        Some(HashColumnsAlgo::Sha512)
    );
    assert_eq!(HashColumnsAlgo::parse("md5"), Some(HashColumnsAlgo::Md5));
    assert_eq!(HashColumnsAlgo::parse("sha1"), None);
}

#[test]
fn add_hash_column_resolves_names_and_refuses_a_taken_name() {
    let mut t = table(&["x", "y"], vec![vec![s("a"), s("b")]]);
    let name = add_hash_column(
        &mut t,
        &["x".into(), "y".into()],
        HashColumnsSpec::default(),
        None,
    )
    .unwrap();
    assert_eq!(name, "hash_x_y");
    assert_eq!(t.rows[0][2].to_string(), "d0726241020676b14aa6298ce6a18b21");
    assert!(add_hash_column(&mut t, &["x".into()], HashColumnsSpec::default(), Some("x")).is_err());
    assert!(add_hash_column(&mut t, &["nope".into()], HashColumnsSpec::default(), None).is_err());
}

#[test]
fn the_default_name_joins_the_column_names() {
    let t = table(&["id", "email"], vec![]);
    assert_eq!(default_name(&t, &[1, 0]), "hash_email_id");
}
