use super::*;

/// Writes a two-row-group Parquet file, then asserts the inspector sees
/// both groups, the codec, and per-column statistics.
#[test]
fn parquet_row_groups_and_codec() {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use parquet::basic::Compression;
    use parquet::file::properties::WriterProperties;
    use std::sync::Arc;

    let dir = std::env::temp_dir().join("octa_internals_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two_groups.parquet");

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_max_row_group_row_count(Some(2))
        .build();
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props)).unwrap();
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
            Arc::new(StringArray::from(vec!["a", "b", "c", "d"])),
        ],
    )
    .unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let got = inspect(&path).unwrap();

    let facts: std::collections::HashMap<String, String> = got.facts.iter().cloned().collect();
    assert_eq!(facts.get("format").map(String::as_str), Some("Parquet"));
    assert_eq!(facts.get("row_groups").map(String::as_str), Some("2"));
    assert_eq!(facts.get("rows").map(String::as_str), Some("4"));

    // One row per column per row group: 2 columns x 2 groups.
    assert_eq!(got.chunks.row_count(), 4);
    let headers: Vec<&str> = got.chunks.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        headers,
        vec![
            "row_group",
            "column",
            "rows",
            "compression",
            "encodings",
            "compressed_bytes",
            "uncompressed_bytes",
            "nulls",
            "min",
            "max",
        ]
    );
    let compression: Vec<String> = (0..4)
        .map(|r| {
            got.chunks
                .get(r, 3)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .collect();
    assert!(
        compression.iter().all(|c| c.contains("SNAPPY")),
        "{compression:?}"
    );

    // Statistics are on by default, so min/max must be populated for the
    // id column; without them a reader cannot skip row groups.
    let id_min = got
        .chunks
        .get(0, 8)
        .map(|c| c.to_string())
        .unwrap_or_default();
    assert_eq!(id_min, "1");
}

/// Two rows per row group is far below the threshold, so the "small row
/// groups" hint must fire. Hints are the plain-language half of this tool.
#[test]
fn small_row_groups_raise_a_hint() {
    let dir = std::env::temp_dir().join("octa_internals_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hint_groups.parquet");
    write_tiny_groups(&path);

    let got = inspect(&path).unwrap();
    assert!(
        got.hints.iter().any(|h| h.contains("row group")),
        "expected a row-group hint, got {:?}",
        got.hints
    );
}

#[test]
fn unknown_format_returns_minimal_facts() {
    let dir = std::env::temp_dir().join("octa_internals_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plain.csv");
    std::fs::write(&path, "a,b\n1,2\n").unwrap();

    let got = inspect(&path).unwrap();
    let facts: std::collections::HashMap<String, String> = got.facts.iter().cloned().collect();
    assert!(facts.contains_key("file_size_bytes"));
    assert_eq!(got.chunks.row_count(), 0);
}

fn write_tiny_groups(path: &std::path::Path) {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use parquet::file::properties::WriterProperties;
    use std::sync::Arc;

    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
    let props = WriterProperties::builder()
        .set_max_row_group_row_count(Some(2))
        .build();
    let file = std::fs::File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props)).unwrap();
    let batch = RecordBatch::try_new(
        schema,
        vec![Arc::new(Int64Array::from((0..10).collect::<Vec<i64>>()))],
    )
    .unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}
