use super::*;

fn sample() -> RefTable {
    RefTable {
        names: vec!["date".into(), "USD".into(), "lat".into()],
        columns: vec![
            RefColumn::Text(vec![
                Some("2026-01-02".into()),
                Some("2026-01-05".into()),
                None,
            ]),
            RefColumn::Decimal {
                scale: 4,
                values: vec![Some(1_1052), None, Some(-3)],
            },
            RefColumn::Int(vec![Some(51_4333), Some(48_1345), None]),
        ],
        meta: [(META_SOURCE.to_string(), "made up".to_string())].into(),
    }
}

#[test]
fn a_table_round_trips_with_exact_values_and_its_footer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.parquet");
    write(&path, &sample()).unwrap();
    let back = read(&path).unwrap();
    assert_eq!(back.names, sample().names);
    assert_eq!(back.columns, sample().columns);
    assert_eq!(
        back.meta.get(META_SOURCE).map(String::as_str),
        Some("made up")
    );
    assert_eq!(
        read_meta(&path)
            .unwrap()
            .get(META_SOURCE)
            .map(String::as_str),
        Some("made up")
    );
}

#[test]
fn a_failed_write_leaves_the_old_file_and_no_part_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.parquet");
    write(&path, &sample()).unwrap();
    let mut broken = sample();
    broken.columns[1] = RefColumn::Decimal {
        scale: 4,
        values: vec![Some(1)],
    }; // wrong length
    assert!(write(&path, &broken).is_err());
    assert_eq!(read(&path).unwrap().columns, sample().columns);
    assert!(!path.with_extension("parquet.part").exists());
}

#[test]
fn huge_decimals_get_a_wider_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.parquet");
    let t = RefTable {
        names: vec!["v".into()],
        columns: vec![RefColumn::Decimal {
            scale: 2,
            values: vec![Some(10i128.pow(30))],
        }],
        meta: BTreeMap::new(),
    };
    write(&path, &t).unwrap();
    assert_eq!(read(&path).unwrap().columns, t.columns);
}

#[test]
fn touch_moves_the_modification_time_forward() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.parquet");
    write(&path, &sample()).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    touch(&path).unwrap();
    assert!(modified(&path).unwrap() > old);
    assert!(size_of(&[path.clone(), dir.path().join("missing")]) > 0);
}

#[test]
fn narrow_decimals_and_ints_are_delta_encoded_without_a_dictionary() {
    use parquet::basic::Encoding;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.parquet");
    write(&path, &sample()).unwrap();
    let reader = SerializedFileReader::new(File::open(&path).unwrap()).unwrap();
    let group = reader.metadata().row_group(0);
    for i in [1, 2] {
        let c = group.column(i);
        let encodings: Vec<Encoding> = c.encodings().collect();
        assert!(
            encodings.contains(&Encoding::DELTA_BINARY_PACKED),
            "{i}: {encodings:?}"
        );
        assert!(
            !encodings.contains(&Encoding::RLE_DICTIONARY),
            "{i}: {encodings:?}"
        );
        assert!(
            !encodings.contains(&Encoding::PLAIN_DICTIONARY),
            "{i}: {encodings:?}"
        );
        assert!(matches!(c.compression(), Compression::ZSTD(_)), "{i}");
    }
}
