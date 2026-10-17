use super::*;
use crate::data::ColumnInfo;

#[test]
fn splits_rows_by_value() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "region".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "amt".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("US".into()), CellValue::Int(1)],
        vec![CellValue::String("EU".into()), CellValue::Int(2)],
        vec![CellValue::String("US".into()), CellValue::Int(3)],
    ];
    let parts = partition_table(&t, 0);
    let mut labels: Vec<_> = parts
        .iter()
        .map(|(v, tbl)| (v.clone(), tbl.row_count()))
        .collect();
    labels.sort();
    assert_eq!(labels, vec![("EU".to_string(), 1), ("US".to_string(), 2)]);
    assert_eq!(parts[0].1.columns.len(), 2);
}

#[test]
fn null_groups_under_null_key() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "k".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![
        vec![CellValue::Null],
        vec![CellValue::String("A".into())],
        vec![CellValue::Null],
    ];
    let parts = partition_table(&t, 0);
    let null_group = parts.iter().find(|(k, _)| k == "__null__");
    assert!(null_group.is_some());
    assert_eq!(null_group.unwrap().1.row_count(), 2);
}

#[test]
fn preserves_row_order_within_group() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "g".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("X".into()), CellValue::Int(10)],
        vec![CellValue::String("X".into()), CellValue::Int(20)],
        vec![CellValue::String("X".into()), CellValue::Int(30)],
    ];
    let parts = partition_table(&t, 0);
    assert_eq!(parts.len(), 1);
    let rows = &parts[0].1;
    assert_eq!(rows.get(0, 1), Some(&CellValue::Int(10)));
    assert_eq!(rows.get(1, 1), Some(&CellValue::Int(20)));
    assert_eq!(rows.get(2, 1), Some(&CellValue::Int(30)));
}

#[test]
fn hive_directory_names_are_filesystem_safe() {
    assert_eq!(hive_dir_name("region", "eu"), "region=eu");
    // Characters that cannot appear in a path component become _.
    assert_eq!(hive_dir_name("path", "a/b"), "path=a_b");
    assert_eq!(hive_dir_name("win", "c:\\d"), "win=c__d");
    // The engine's own null marker survives, so dataset mode reads it back.
    assert_eq!(hive_dir_name("region", "__null__"), "region=__null__");
    // Dots and dashes are legal in a Hive value and must be kept, or the
    // value could not be recovered from the directory name.
    assert_eq!(hive_dir_name("d", "2024-01-31"), "d=2024-01-31");
    assert_eq!(hive_dir_name("v", "1.2.3"), "v=1.2.3");
}

#[test]
fn layout_parses_the_cli_words_and_nothing_else() {
    for l in PartitionLayout::ALL {
        assert_eq!(PartitionLayout::parse(l.id()), Some(*l), "{}", l.id());
        assert_eq!(PartitionLayout::parse(&l.id().to_uppercase()), Some(*l));
    }
    // An underscore is as natural as a hyphen on a command line.
    assert_eq!(
        PartitionLayout::parse("hive_parts"),
        Some(PartitionLayout::HiveParts)
    );
    assert_eq!(
        PartitionLayout::parse("  flat  "),
        Some(PartitionLayout::Flat)
    );
    assert_eq!(PartitionLayout::parse("nested"), None);
    assert_eq!(PartitionLayout::parse(""), None);
}

/// The one function every surface names files with. If these shapes
/// change, the dialog preview, the writer, the CLI and the MCP tool all
/// change together - which is the point of it being one function.
#[test]
fn partition_path_shapes() {
    let p = |l| partition_path(l, "city", "New York", "csv");
    assert_eq!(p(PartitionLayout::Flat), "new_york.csv");
    assert_eq!(p(PartitionLayout::Folder), "New York/part-0001.csv");
    assert_eq!(p(PartitionLayout::Hive), "city=New York/data.csv");
    assert_eq!(p(PartitionLayout::HiveParts), "city=New York/part-0001.csv");
}

/// Only the folder layouts keep the value intact; Flat folds it down to a
/// SQL-safe stem, which is the whole reason the other three exist.
#[test]
fn folder_layouts_keep_the_value_flat_does_not() {
    let v = "New-York 2024";
    assert_eq!(
        partition_path(PartitionLayout::Flat, "c", v, "csv"),
        "new_york_2024.csv"
    );
    assert_eq!(
        partition_path(PartitionLayout::Folder, "c", v, "csv"),
        "New-York 2024/part-0001.csv"
    );
}

/// A character that cannot appear in a path is replaced in every layout
/// that puts the value in a directory name.
#[test]
fn path_illegal_characters_are_replaced_in_folder_names() {
    for l in [
        PartitionLayout::Folder,
        PartitionLayout::Hive,
        PartitionLayout::HiveParts,
    ] {
        let out = partition_path(l, "c", "a/b", "csv");
        assert!(!out.starts_with("a/b"), "{l:?} left a raw slash: {out}");
        assert!(out.contains("a_b"), "{l:?}: {out}");
    }
}

/// Flat is the only lossy layout, so it is the only one that can collide.
#[test]
fn only_flat_needs_the_collision_counter() {
    let mut seen = std::collections::HashMap::new();
    let names: Vec<String> = ["New York", "new-york", "NEW_YORK"]
        .iter()
        .map(|v| {
            let rel = partition_path(PartitionLayout::Flat, "c", v, "csv");
            dedupe_flat_name(PartitionLayout::Flat, rel, "csv", &mut seen)
        })
        .collect();
    assert_eq!(names, ["new_york.csv", "new_york_2.csv", "new_york_3.csv"]);

    let mut seen = std::collections::HashMap::new();
    let names: Vec<String> = ["New York", "new-york", "NEW_YORK"]
        .iter()
        .map(|v| {
            let rel = partition_path(PartitionLayout::Hive, "c", v, "csv");
            dedupe_flat_name(PartitionLayout::Hive, rel, "csv", &mut seen)
        })
        .collect();
    assert_eq!(
        names,
        [
            "c=New York/data.csv",
            "c=new-york/data.csv",
            "c=NEW_YORK/data.csv"
        ],
        "already distinct, so no counter is applied"
    );
}

use std::path::Path;
use std::sync::atomic::AtomicBool;

/// `n` rows: an integer and a city alternating Berlin / Tokyo.
fn numbered(n: usize) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = (0..n)
        .map(|i| {
            vec![
                CellValue::Int(i as i64),
                CellValue::String(if i % 2 == 0 { "Berlin" } else { "Tokyo" }.into()),
            ]
        })
        .collect();
    t
}

/// A writer whose file is exactly `overhead + per_row * rows` bytes, so the
/// size search can be checked without a real format.
fn sized_writer(
    overhead: usize,
    per_row: usize,
) -> impl FnMut(&DataTable, &Path) -> anyhow::Result<()> {
    move |t: &DataTable, p: &Path| {
        std::fs::write(p, vec![b'x'; overhead + per_row * t.row_count()])?;
        Ok(())
    }
}

/// Run a split into a fresh temp dir and return `(relative path, rows)`
/// per written file, with `/` separators on every platform.
fn run(
    t: &DataTable,
    spec: &SplitSpec,
    write: &mut dyn FnMut(&DataTable, &Path) -> anyhow::Result<()>,
) -> anyhow::Result<(tempfile::TempDir, Vec<(String, usize)>)> {
    let dir = tempfile::tempdir()?;
    let out = write_split(
        t,
        spec,
        dir.path(),
        "csv",
        "data",
        SplitIo {
            write,
            cancel: &AtomicBool::new(false),
            progress: &mut |_, _| {},
        },
    )?;
    let rels = out
        .parts
        .iter()
        .map(|p| {
            let rel = p.path.strip_prefix(dir.path()).unwrap();
            (rel.to_string_lossy().replace('\\', "/"), p.rows)
        })
        .collect();
    Ok((dir, rels))
}

fn rows_only(max_rows: usize) -> SplitSpec {
    SplitSpec {
        max_rows: Some(max_rows),
        ..Default::default()
    }
}

#[test]
fn a_row_limit_alone_numbers_every_part() {
    let (_d, files) = run(&numbered(25), &rows_only(10), &mut sized_writer(0, 1)).unwrap();
    assert_eq!(
        files,
        [
            ("data_part_0001.csv".to_string(), 10),
            ("data_part_0002.csv".to_string(), 10),
            ("data_part_0003.csv".to_string(), 5),
        ]
    );
}

#[test]
fn an_exact_multiple_leaves_no_empty_tail() {
    let (_d, files) = run(&numbered(20), &rows_only(10), &mut sized_writer(0, 1)).unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn an_empty_table_still_writes_one_part() {
    let (_d, files) = run(&numbered(0), &rows_only(10), &mut sized_writer(0, 1)).unwrap();
    assert_eq!(files, [("data_part_0001.csv".to_string(), 0)]);
}

#[test]
fn column_and_row_limit_number_parts_per_folder() {
    let spec = SplitSpec {
        column: Some(1),
        layout: PartitionLayout::HiveParts,
        max_rows: Some(2),
        max_bytes: None,
    };
    let (_d, files) = run(&numbered(6), &spec, &mut sized_writer(0, 1)).unwrap();
    assert_eq!(
        files,
        [
            ("city=Berlin/part-0001.csv".to_string(), 2),
            ("city=Berlin/part-0002.csv".to_string(), 1),
            ("city=Tokyo/part-0001.csv".to_string(), 2),
            ("city=Tokyo/part-0002.csv".to_string(), 1),
        ]
    );
}

#[test]
fn hive_with_a_limit_switches_data_to_numbered_parts() {
    let spec = SplitSpec {
        column: Some(1),
        layout: PartitionLayout::Hive,
        max_rows: Some(5),
        max_bytes: None,
    };
    let (_d, files) = run(&numbered(4), &spec, &mut sized_writer(0, 1)).unwrap();
    assert_eq!(files[0].0, "city=Berlin/part-0001.csv");
}

#[test]
fn flat_with_parts_keeps_the_value_stem() {
    let spec = SplitSpec {
        column: Some(1),
        layout: PartitionLayout::Flat,
        max_rows: Some(2),
        max_bytes: None,
    };
    let (_d, files) = run(&numbered(4), &spec, &mut sized_writer(0, 1)).unwrap();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["berlin_part_0001.csv", "tokyo_part_0001.csv"]);
}

#[test]
fn without_a_limit_the_names_are_todays() {
    let spec = SplitSpec {
        column: Some(1),
        ..Default::default()
    };
    let (_d, files) = run(&numbered(4), &spec, &mut sized_writer(0, 1)).unwrap();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["berlin.csv", "tokyo.csv"]);
}

#[test]
fn a_size_limit_is_never_exceeded_and_every_row_is_written() {
    let spec = SplitSpec {
        max_bytes: Some(2_000),
        ..Default::default()
    };
    let (dir, files) = run(&numbered(5_000), &spec, &mut sized_writer(100, 10)).unwrap();
    let total: usize = files.iter().map(|(_, r)| r).sum();
    assert_eq!(total, 5_000);
    assert!(files.len() > 1);
    for (name, _) in &files {
        let len = std::fs::metadata(dir.path().join(name)).unwrap().len();
        assert!(len <= 2_000, "{name} is {len} bytes");
    }
    assert!(files.len() <= 30, "parts too small: {}", files.len());
}

#[test]
fn size_and_row_limits_combine() {
    let spec = SplitSpec {
        max_rows: Some(7),
        max_bytes: Some(10_000),
        ..Default::default()
    };
    let (_d, files) = run(&numbered(20), &spec, &mut sized_writer(0, 10)).unwrap();
    let rows: Vec<usize> = files.iter().map(|(_, r)| *r).collect();
    assert_eq!(rows, [7, 7, 6]);
}

#[test]
fn a_row_larger_than_the_limit_is_named() {
    let spec = SplitSpec {
        max_bytes: Some(100),
        ..Default::default()
    };
    let err = run(&numbered(3), &spec, &mut sized_writer(0, 500)).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("row 1"), "{msg}");
}

#[test]
fn nothing_to_split_by_is_refused() {
    let err = SplitSpec::default().validate().unwrap_err();
    assert!(format!("{err:#}").contains("column"));
    assert!(rows_only(0).validate().is_err());
}

#[test]
fn cancelling_stops_before_the_next_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = write_split(
        &numbered(30),
        &rows_only(10),
        dir.path(),
        "csv",
        "data",
        SplitIo {
            write: &mut sized_writer(0, 1),
            cancel: &AtomicBool::new(true),
            progress: &mut |_, _| {},
        },
    )
    .unwrap();
    assert!(out.cancelled);
    assert!(out.parts.is_empty());
}

#[test]
fn parse_size_reads_the_usual_spellings() {
    assert_eq!(parse_size("25MB"), Ok(25_000_000));
    assert_eq!(parse_size("25 mb"), Ok(25_000_000));
    assert_eq!(parse_size("500KB"), Ok(500_000));
    assert_eq!(parse_size("1GB"), Ok(1_000_000_000));
    assert_eq!(parse_size("1.5GB"), Ok(1_500_000_000));
    assert_eq!(parse_size("2MiB"), Ok(2 * 1_048_576));
    assert_eq!(parse_size("1048576"), Ok(1_048_576));
    for bad in ["", "abc", "0MB", "-5MB", "12XB"] {
        assert!(parse_size(bad).is_err(), "{bad:?} should be refused");
    }
}

#[test]
fn the_preview_uses_the_writer_names() {
    let spec = SplitSpec {
        column: Some(1),
        layout: PartitionLayout::Flat,
        max_rows: Some(10),
        max_bytes: None,
    };
    let names = preview_paths(&spec, "city", &["New York".into()], "data", "csv");
    assert_eq!(names, ["new_york_part_0001.csv"]);
    let whole = SplitSpec {
        max_rows: Some(10),
        ..Default::default()
    };
    assert_eq!(
        preview_paths(&whole, "", &[], "Sales 2024", "csv"),
        ["Sales 2024_part_0001.csv"]
    );
}

#[test]
fn an_empty_file_over_the_limit_is_an_error() {
    let spec = SplitSpec {
        max_bytes: Some(10),
        ..Default::default()
    };
    let err = run(&numbered(0), &spec, &mut sized_writer(100, 1)).unwrap_err();
    assert!(format!("{err:#}").contains("empty file"));
}

#[test]
fn cancelling_part_way_keeps_the_parts_written() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let mut inner = sized_writer(0, 1);
    let mut write = |t: &DataTable, p: &Path| {
        inner(t, p)?;
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    };
    let out = write_split(
        &numbered(30),
        &rows_only(10),
        dir.path(),
        "csv",
        "data",
        SplitIo {
            write: &mut write,
            cancel: &cancel,
            progress: &mut |_, _| {},
        },
    )
    .unwrap();
    assert!(out.cancelled);
    assert_eq!(out.parts.len(), 1);
}
