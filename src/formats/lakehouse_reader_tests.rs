use super::*;

#[test]
fn detects_delta_by_log_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("_delta_log")).unwrap();
    assert_eq!(detect(dir.path()), Some(LakehouseKind::Delta));
}

#[test]
fn detects_iceberg_by_metadata_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("metadata")).unwrap();
    assert_eq!(detect(dir.path()), Some(LakehouseKind::Iceberg));
}

#[test]
fn plain_directory_is_not_a_lakehouse() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("data")).unwrap();
    assert_eq!(detect(dir.path()), None);
}

#[test]
fn a_file_is_not_a_lakehouse() {
    let f = tempfile::NamedTempFile::new().unwrap();
    assert_eq!(detect(f.path()), None);
}

#[test]
fn detects_parts_dir_of_csvs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("year=2024")).unwrap();
    std::fs::write(dir.path().join("year=2024/p0.csv"), "a,b\n1,2\n").unwrap();
    std::fs::write(dir.path().join("year=2024/p1.csv"), "a,b\n3,4\n").unwrap();
    assert_eq!(
        detect(dir.path()),
        Some(LakehouseKind::Parts(PartsFamily::Delimited))
    );
}

#[test]
fn delta_marker_wins_over_parts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("_delta_log")).unwrap();
    std::fs::write(dir.path().join("p0.parquet"), b"junk").unwrap();
    assert_eq!(detect(dir.path()), Some(LakehouseKind::Delta));
}

#[test]
fn reads_csv_parts_with_hive_column() {
    let dir = tempfile::tempdir().unwrap();
    for (y, v) in [("2023", "1"), ("2024", "2")] {
        let sub = dir.path().join(format!("year={y}"));
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("part.csv"), format!("a\n{v}\n")).unwrap();
    }
    let (t, skipped) =
        read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Delimited)).unwrap();
    assert!(skipped.is_empty());
    assert_eq!(t.row_count(), 2);
    assert!(
        t.columns.iter().any(|c| c.name == "year"),
        "hive col materialised: {:?}",
        t.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn majority_family_wins_and_reports_skips() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), "x\n1\n").unwrap();
    std::fs::write(dir.path().join("b.csv"), "x\n2\n").unwrap();
    std::fs::write(dir.path().join("c.jsonl"), "{\"x\":3}\n").unwrap();
    let Some(LakehouseKind::Parts(fam)) = detect(dir.path()) else {
        panic!("expected a parts dir");
    };
    assert_eq!(fam, PartsFamily::Delimited);
    let (t, skipped) = read_dir_report(dir.path(), LakehouseKind::Parts(fam)).unwrap();
    assert_eq!(t.row_count(), 2);
    assert_eq!(skipped, vec!["c.jsonl".to_string()]);
}

#[test]
fn reads_gzipped_csv_parts() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let f = std::fs::File::create(dir.path().join("p0.csv.gz")).unwrap();
    let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    enc.write_all(b"a\n7\n").unwrap();
    enc.finish().unwrap();
    assert_eq!(
        detect(dir.path()),
        Some(LakehouseKind::Parts(PartsFamily::Delimited))
    );
    let (t, _) = read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Delimited)).unwrap();
    assert_eq!(t.row_count(), 1);
}

#[test]
fn jsonl_parts_read_as_dataset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p0.jsonl"), "{\"x\": 1}\n{\"x\": 2}\n").unwrap();
    let (t, _) = read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::JsonLines)).unwrap();
    assert_eq!(t.row_count(), 2);
    assert!(t.columns.iter().any(|c| c.name == "x"));
}

#[test]
fn parquet_wins_family_tie() {
    let files = vec![
        (PartsFamily::JsonLines, "a.jsonl".to_string()),
        (PartsFamily::Parquet, "b.parquet".to_string()),
    ];
    let (winner, skipped) = majority_family(&files).unwrap();
    assert_eq!(winner, PartsFamily::Parquet);
    assert_eq!(skipped, vec!["a.jsonl".to_string()]);
}

#[test]
fn empty_dataset_dir_errors_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let err = read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Parquet))
        .unwrap_err()
        .to_string();
    assert!(err.contains("no tabular part files"), "{err}");
}
