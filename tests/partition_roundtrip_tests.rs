//! What the two partition layouts actually differ in.
//!
//! Written because the dialog's tooltip claimed only Hive could be reopened
//! as one table. `partition_table` copies every column into every group, so
//! that was wrong - but "wrong" is a claim that needs checking rather than
//! asserting, and these tests are the check.

use std::path::Path;

use octa::data::partition::{PartitionLayout, dedupe_flat_name, partition_path, partition_table};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;
use octa::formats::lakehouse_reader::{LakehouseKind, PartsFamily, detect, read_dir_report};

fn source() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    for (city, n) in [
        ("New York", 1),
        ("Berlin", 2),
        ("New York", 3),
        ("Sao Paulo", 4),
    ] {
        t.rows
            .push(vec![CellValue::String(city.into()), CellValue::Int(n)]);
    }
    t
}

/// Write the source out in one layout, exactly as `apply_partition` does.
fn write_layout(dir: &Path, layout: PartitionLayout) {
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(Path::new("x.csv")).unwrap();
    let mut seen = std::collections::HashMap::new();
    for (i, (value, group)) in partition_table(&source(), 0).into_iter().enumerate() {
        let rel = partition_path(layout, "city", &value, "csv", i + 1);
        let rel = dedupe_flat_name(layout, rel, "csv", &mut seen);
        let path = dir.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        reader.write_file(&path, &group).unwrap();
    }
}

fn read_back(dir: &Path) -> DataTable {
    let kind = detect(dir).unwrap_or(LakehouseKind::Parts(PartsFamily::Delimited));
    read_dir_report(dir, kind).unwrap().0
}

/// Every layout holds the same rows and every one comes back as a single
/// table with the partition column intact. That is the claim the dialog makes
/// ("all four hold the same rows"), and it is the reason the choice is safe.
#[test]
fn every_layout_round_trips_to_the_same_table() {
    let cities = |t: &DataTable| {
        let col = t.columns.iter().position(|c| c.name == "city").unwrap();
        let mut v: Vec<String> = (0..t.row_count())
            .map(|r| t.get(r, col).unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    let expected = vec![
        "Berlin".to_string(),
        "New York".to_string(),
        "New York".to_string(),
        "Sao Paulo".to_string(),
    ];

    for layout in PartitionLayout::ALL {
        let dir = tempfile::tempdir().unwrap();
        write_layout(dir.path(), *layout);
        let back = read_back(dir.path());

        assert_eq!(back.row_count(), 4, "{layout:?} kept every row");
        let mut names: Vec<String> = back.columns.iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["city".to_string(), "n".to_string()],
            "{layout:?}: partition column present, and not duplicated"
        );
        assert_eq!(
            cities(&back),
            expected,
            "{layout:?}: the ORIGINAL spelling survives, because it is in the file"
        );
    }
}

/// Where they really differ: the names on disk.
#[test]
fn the_difference_is_the_names_on_disk() {
    let names = |layout: PartitionLayout| -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        write_layout(dir.path(), layout);
        let mut out = Vec::new();
        for e in walk(dir.path()) {
            out.push(
                e.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        out.sort();
        out
    };

    assert_eq!(
        names(PartitionLayout::Flat),
        vec!["berlin.csv", "new_york.csv", "sao_paulo.csv"]
    );
    assert_eq!(
        names(PartitionLayout::Folder),
        vec![
            "Berlin/part-0001.csv",
            "New York/part-0002.csv",
            "Sao Paulo/part-0003.csv"
        ]
    );
    assert_eq!(
        names(PartitionLayout::Hive),
        vec![
            "city=Berlin/data.csv",
            "city=New York/data.csv",
            "city=Sao Paulo/data.csv"
        ]
    );
    assert_eq!(
        names(PartitionLayout::HiveParts),
        vec![
            "city=Berlin/part-0001.csv",
            "city=New York/part-0002.csv",
            "city=Sao Paulo/part-0003.csv"
        ]
    );
}

/// Every file, recursively, so a nested layout is compared like a flat one.
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
