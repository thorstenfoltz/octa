//! Everything here runs without DuckDB's extension, so it runs everywhere. What
//! needs the real one is in `tests/lakehouse_live_tests.rs`.

use super::*;
use crate::formats::FormatRegistry;

fn table_of(name: &str, data_type: &str) -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
        }],
        rows: vec![vec![CellValue::Int(1)]],
        ..DataTable::empty()
    }
}

#[test]
fn every_scalar_type_comes_back_as_itself() {
    for raw in [
        "BOOLEAN",
        "TINYINT",
        "SMALLINT",
        "INTEGER",
        "BIGINT",
        "UTINYINT",
        "USMALLINT",
        "UINTEGER",
        "UBIGINT",
        "FLOAT",
        "DOUBLE",
        "VARCHAR",
        "BLOB",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "TIMESTAMP_S",
        "TIMESTAMP_MS",
        "TIMESTAMP_NS",
        "DECIMAL(38,2)",
    ] {
        let name = exact_type(raw, None).unwrap_or_else(|| panic!("{raw} has no name"));
        assert_eq!(stage_type(&name), raw, "through {name}");
    }
}

#[test]
fn a_type_with_no_cell_keeps_its_own_spelling() {
    for raw in [
        "FLOAT[3]",
        "DOUBLE[]",
        "INTEGER[]",
        "VARCHAR[][]",
        "STRUCT(x INTEGER, y VARCHAR)",
        "MAP(VARCHAR, INTEGER)",
    ] {
        assert_eq!(exact_type(raw, None), None, "{raw}");
        assert_eq!(stage_type(raw), raw);
    }
}

/// A column type can come from outside Octa and goes into SQL unquoted.
#[test]
fn a_type_that_could_leave_its_column_definition_is_written_as_text() {
    for hostile in [
        "INTEGER[]); DROP TABLE stage; --[]",
        "FLOAT[3]'",
        "STRUCT(a INT)) AS SELECT 1 [",
        "STRUCT(a INT",
        "INT[]) AS SELECT 1 FROM x[]",
        "MAP(a;b)",
    ] {
        assert_eq!(stage_type(hostile), "VARCHAR", "{hostile}");
    }
}

/// Arrow's own names look alike once upper-cased, and DuckDB knows none of them.
#[test]
fn another_formats_nested_type_is_written_as_text() {
    for arrow in [
        "Map(Utf8, Int32)",
        "Struct(a Int32)",
        "List(Int32)",
        "Union(a)",
    ] {
        assert_eq!(stage_type(arrow), "VARCHAR", "{arrow}");
    }
}

#[test]
fn durations_and_unknown_types_are_written_as_text() {
    assert_eq!(stage_type("Duration(Microsecond)"), "VARCHAR");
    assert_eq!(stage_type("Utf8"), "VARCHAR");
    assert_eq!(stage_type("SomethingNew"), "VARCHAR");
    assert_eq!(stage_type("Time64(Microsecond)"), "TIME");
}

#[test]
fn a_zoned_timestamp_goes_back_to_the_instant_it_stands_for() {
    let zoned = exact_type("TIMESTAMP WITH TIME ZONE", Some("Asia/Tokyo")).unwrap();
    assert_eq!(stage_type(&zoned), "TIMESTAMP");
    let column = ColumnInfo {
        name: "at".to_string(),
        data_type: zoned,
    };
    assert_eq!(
        copy_item(&column),
        "timezone('Asia/Tokyo', \"at\") AS \"at\""
    );

    let plain = ColumnInfo {
        name: "at".to_string(),
        data_type: "Timestamp(Microsecond, None)".to_string(),
    };
    assert_eq!(copy_item(&plain), "\"at\"");
}

#[test]
fn a_plain_file_is_refused_and_left_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("taken.lance");
    std::fs::write(&path, "mine").unwrap();

    let err = LanceReader
        .write_file(&path, &table_of("a", "Int64"))
        .unwrap_err();

    assert!(format!("{err:#}").contains("is a file"), "{err:#}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no tmp_");
}

#[test]
fn a_folder_of_other_files_is_refused_and_left_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("taken.lance");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("mine.txt"), "mine").unwrap();

    let err = LanceReader
        .write_file(&path, &table_of("a", "Int64"))
        .unwrap_err();

    assert!(
        format!("{err:#}").contains("not a Lance dataset"),
        "{err:#}"
    );
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
}

#[test]
fn new_names_empty_folders_and_datasets_may_be_written() {
    let dir = tempfile::tempdir().unwrap();
    assert!(check_target(&dir.path().join("new.lance")).is_ok());

    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(check_target(&empty).is_ok());

    let dataset = dir.path().join("dataset");
    std::fs::create_dir_all(dataset.join(VERSIONS_DIR)).unwrap();
    std::fs::write(dataset.join("other"), "x").unwrap();
    assert!(check_target(&dataset).is_ok());
}

#[test]
fn a_data_file_on_its_own_says_to_open_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("part.lance");
    std::fs::write(&path, "LANC").unwrap();

    let err = LanceReader.read_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains(VERSIONS_DIR), "{err:#}");
    assert!(LanceReader.read_schema(&path).is_err());
}

#[test]
fn a_data_file_names_the_dataset_it_belongs_to() {
    let dir = tempfile::tempdir().unwrap();
    let dataset = dir.path().join("t.lance");
    std::fs::create_dir_all(dataset.join(VERSIONS_DIR)).unwrap();
    std::fs::create_dir(dataset.join("data")).unwrap();
    let part = dataset.join("data").join("part.lance");
    std::fs::write(&part, "LANC").unwrap();
    assert_eq!(dataset_of(&part), Some(dataset.as_path()));

    // Not inside a dataset's data folder: no dataset to open.
    let stray = dir.path().join("part.lance");
    std::fs::write(&stray, "LANC").unwrap();
    assert_eq!(dataset_of(&stray), None);
    let manifest = dataset.join(VERSIONS_DIR).join("1.manifest");
    std::fs::write(&manifest, "x").unwrap();
    assert_eq!(dataset_of(&manifest), None);
    assert_eq!(dataset_of(&dataset), None);
}

#[test]
fn a_table_without_columns_is_refused_before_anything_loads() {
    let dir = tempfile::tempdir().unwrap();
    let err = LanceReader
        .write_file(&dir.path().join("new.lance"), &DataTable::empty())
        .unwrap_err();
    assert!(format!("{err:#}").contains("column"), "{err:#}");
}

#[test]
fn the_registry_finds_a_dataset_by_its_ending_or_by_what_it_holds() {
    let registry = FormatRegistry::new();
    let by_name = registry
        .reader_for_path(Path::new("no/such/x.lance"))
        .unwrap();
    assert_eq!(by_name.name(), "Lance");
    assert!(by_name.supports_write());
    assert!(by_name.writes_a_folder());

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(VERSIONS_DIR)).unwrap();
    let by_content = registry.reader_for_path(dir.path()).unwrap();
    assert_eq!(by_content.name(), "Lance");

    let csv = registry.reader_for_path(Path::new("x.csv")).unwrap();
    assert!(!csv.writes_a_folder());
}

#[test]
fn a_dataset_is_lance_even_with_parquet_files_around() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(VERSIONS_DIR)).unwrap();
    std::fs::write(dir.path().join("part-0.parquet"), "x").unwrap();
    assert_eq!(
        lakehouse_reader::detect(dir.path()),
        Some(LakehouseKind::Lance)
    );
}

#[test]
fn the_documented_scan_is_used_where_the_name_allows_it() {
    assert_eq!(scan("/d/x.lance"), "'/d/x.lance'");
    assert_eq!(scan("/d/plain"), "__lance_scan('/d/plain')");
    assert_eq!(scan("/d/X.LANCE"), "__lance_scan('/d/X.LANCE')");
}

/// A path Octa holds as local must not reach the extension as an object store.
#[test]
fn a_url_shaped_path_stays_a_local_path() {
    let literal = lakehouse_reader::dir_literal(Path::new("s3://bucket/t.lance")).unwrap();
    assert!(!literal.starts_with("s3:"), "{literal}");
    assert!(std::path::Path::new(&literal).is_absolute(), "{literal}");
}

#[test]
fn a_quote_in_a_path_stays_inside_the_literal() {
    let literal = lakehouse_reader::dir_literal(Path::new("/d/it's.lance")).unwrap();
    assert!(literal.ends_with("it''s.lance"), "{literal}");
}

/// Lance keeps the version a save replaces, so there is nothing to copy, and a
/// failed copy would stop the save.
#[test]
fn a_backup_of_a_folder_makes_no_copy_and_does_not_stop_the_save() {
    let dir = tempfile::tempdir().unwrap();
    let dataset = dir.path().join("d.lance");
    std::fs::create_dir(&dataset).unwrap();
    assert_eq!(
        crate::formats::backup_existing_file(&dataset).unwrap(),
        None
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_folder_format_cannot_go_into_an_encrypted_zip() {
    let dir = tempfile::tempdir().unwrap();
    let err = crate::formats::encrypt::write_table_encrypted(
        &table_of("a", "Int64"),
        &dir.path().join("out.zip"),
        "t.lance",
        "secret",
        &Default::default(),
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("pick another format"),
        "{err:#}"
    );
}

#[test]
fn a_size_limit_is_refused_for_a_folder_format() {
    let dir = tempfile::tempdir().unwrap();
    let spec = crate::data::partition::SplitSpec {
        column: None,
        layout: crate::data::partition::PartitionLayout::Flat,
        max_rows: None,
        max_bytes: Some(1_000_000),
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = crate::data::partition::write_split(
        &table_of("a", "Int64"),
        &spec,
        dir.path(),
        "lance",
        "t",
        crate::data::partition::SplitIo {
            write: &mut |_, _| panic!("nothing may be written"),
            cancel: &cancel,
            progress: &mut |_, _| {},
        },
    );
    let Err(err) = result else {
        panic!("a size limit on a folder format must be refused");
    };
    assert!(format!("{err:#}").contains("size limit"), "{err:#}");
}
