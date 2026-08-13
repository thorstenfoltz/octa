//! The physical layout of a file, as opposed to the data in it: row groups,
//! compression codecs, encodings, column statistics.
//!
//! `describe.rs` answers "what is in this file". This answers "how was this
//! file written", which is the question behind "why is it 4 GB" and "why is
//! every query slow". Parquet parses all of it on every read anyway.

use std::path::Path;

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Row groups smaller than this are worth warning about: thousands of tiny
/// groups is the classic streaming-ingest output that reads badly.
pub const SMALL_ROW_GROUP_ROWS: i64 = 10_000;

/// File-level facts, the per-chunk table, and any plain-language warnings.
#[derive(Debug, Clone)]
pub struct FileInternals {
    /// Ordered key/value pairs, e.g. `("row_groups", "12")`.
    pub facts: Vec<(String, String)>,
    /// One row per column per row group (empty for formats without structure).
    pub chunks: DataTable,
    /// Derived warnings in plain words, at most three.
    pub hints: Vec<String>,
}

/// Inspect `path`. Formats without an inspectable structure return their size
/// and nothing else, which is an answer rather than an error.
pub fn inspect(path: &Path) -> anyhow::Result<FileInternals> {
    let size = std::fs::metadata(path).map(|m| m.len()).ok();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut out = match ext.as_str() {
        "parquet" => inspect_parquet(path)?,
        other => FileInternals {
            facts: vec![(
                "format".to_string(),
                format!(
                    "{} (no inspectable internal structure)",
                    if other.is_empty() {
                        "unknown".to_string()
                    } else {
                        other.to_uppercase()
                    }
                ),
            )],
            chunks: empty_chunk_table(),
            hints: Vec::new(),
        },
    };
    if let Some(bytes) = size {
        out.facts
            .push(("file_size_bytes".to_string(), bytes.to_string()));
    }
    Ok(out)
}

/// The per-chunk table's shape, shared by every arm so the GUI, CLI and MCP
/// see the same columns whatever the format.
fn empty_chunk_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        ("row_group", "Int64"),
        ("column", "Utf8"),
        ("rows", "Int64"),
        ("compression", "Utf8"),
        ("encodings", "Utf8"),
        ("compressed_bytes", "Int64"),
        ("uncompressed_bytes", "Int64"),
        ("nulls", "Int64"),
        ("min", "Utf8"),
        ("max", "Utf8"),
    ]
    .iter()
    .map(|(n, ty)| ColumnInfo {
        name: (*n).to_string(),
        data_type: (*ty).to_string(),
    })
    .collect();
    t
}

fn inspect_parquet(path: &Path) -> anyhow::Result<FileInternals> {
    use parquet::file::reader::{FileReader, SerializedFileReader};

    let file = std::fs::File::open(path)?;
    let reader = SerializedFileReader::new(file)?;
    let meta = reader.metadata();
    let file_meta = meta.file_metadata();

    let mut facts = vec![
        ("format".to_string(), "Parquet".to_string()),
        ("rows".to_string(), file_meta.num_rows().to_string()),
        ("row_groups".to_string(), meta.num_row_groups().to_string()),
        (
            "columns".to_string(),
            file_meta.schema_descr().num_columns().to_string(),
        ),
        (
            "writer_version".to_string(),
            file_meta.version().to_string(),
        ),
        (
            "created_by".to_string(),
            file_meta.created_by().unwrap_or("unknown").to_string(),
        ),
    ];

    let mut table = empty_chunk_table();
    let mut total_compressed: i64 = 0;
    let mut total_uncompressed: i64 = 0;
    let mut group_rows: Vec<i64> = Vec::new();
    let mut any_stats = false;
    let mut any_compression = false;
    let mut bloom_filters = 0usize;

    for (g, rg) in meta.row_groups().iter().enumerate() {
        group_rows.push(rg.num_rows());
        for col in rg.columns() {
            let compression = format!("{:?}", col.compression());
            if !compression.contains("UNCOMPRESSED") {
                any_compression = true;
            }
            if col.bloom_filter_offset().is_some() {
                bloom_filters += 1;
            }
            // `encodings()` hands back an iterator, not a slice.
            let encodings = col
                .encodings()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            let (nulls, min, max) = match col.statistics() {
                Some(s) => {
                    any_stats = true;
                    let (lo, hi) = stat_bounds(s);
                    (s.null_count_opt().map(|n| n as i64), lo, hi)
                }
                None => (None, String::new(), String::new()),
            };
            total_compressed += col.compressed_size();
            total_uncompressed += col.uncompressed_size();

            table.rows.push(vec![
                CellValue::Int(g as i64),
                CellValue::String(col.column_path().string()),
                CellValue::Int(rg.num_rows()),
                CellValue::String(compression),
                CellValue::String(encodings),
                CellValue::Int(col.compressed_size()),
                CellValue::Int(col.uncompressed_size()),
                nulls.map(CellValue::Int).unwrap_or(CellValue::Null),
                CellValue::String(min),
                CellValue::String(max),
            ]);
        }
    }

    facts.push(("compressed_bytes".to_string(), total_compressed.to_string()));
    facts.push((
        "uncompressed_bytes".to_string(),
        total_uncompressed.to_string(),
    ));
    facts.push(("bloom_filters".to_string(), bloom_filters.to_string()));

    let mut hints = Vec::new();
    if group_rows.len() > 1 {
        let mut sorted = group_rows.clone();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        if median < SMALL_ROW_GROUP_ROWS {
            hints.push(format!(
                "Very small row groups: {} groups, median {median} rows each. \
                 Readers pay per-group overhead, so this file scans slowly. \
                 Rewriting it with a larger row-group size usually helps.",
                group_rows.len()
            ));
        }
    }
    if !any_stats {
        hints.push(
            "No column statistics. Query engines cannot skip row groups without \
             min/max values, so every read touches the whole file."
                .to_string(),
        );
    }
    if !any_compression && !group_rows.is_empty() {
        hints.push(
            "No compression. Rewriting with zstd or snappy usually shrinks the \
             file substantially at little read cost."
                .to_string(),
        );
    }

    Ok(FileInternals {
        facts,
        chunks: table,
        hints,
    })
}

/// Render a statistics min/max pair as display strings. Parquet stores these
/// per physical type, so this matches on the variant rather than guessing.
fn stat_bounds(s: &parquet::file::statistics::Statistics) -> (String, String) {
    use parquet::file::statistics::Statistics as S;

    fn fmt<T: std::fmt::Display>(v: Option<&T>) -> String {
        v.map(|x| x.to_string()).unwrap_or_default()
    }
    fn fmt_bytes(v: Option<&parquet::data_type::ByteArray>) -> String {
        v.map(|b| String::from_utf8_lossy(b.data()).to_string())
            .unwrap_or_default()
    }

    match s {
        S::Boolean(v) => (fmt(v.min_opt()), fmt(v.max_opt())),
        S::Int32(v) => (fmt(v.min_opt()), fmt(v.max_opt())),
        S::Int64(v) => (fmt(v.min_opt()), fmt(v.max_opt())),
        S::Float(v) => (fmt(v.min_opt()), fmt(v.max_opt())),
        S::Double(v) => (fmt(v.min_opt()), fmt(v.max_opt())),
        S::ByteArray(v) => (fmt_bytes(v.min_opt()), fmt_bytes(v.max_opt())),
        S::FixedLenByteArray(v) => (
            v.min_opt()
                .map(|b| String::from_utf8_lossy(b.data()).to_string())
                .unwrap_or_default(),
            v.max_opt()
                .map(|b| String::from_utf8_lossy(b.data()).to_string())
                .unwrap_or_default(),
        ),
        S::Int96(v) => (
            v.min_opt().map(|x| format!("{x}")).unwrap_or_default(),
            v.max_opt().map(|x| format!("{x}")).unwrap_or_default(),
        ),
    }
}

#[cfg(test)]
mod tests {
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
}
