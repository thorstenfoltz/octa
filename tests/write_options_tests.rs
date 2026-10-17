//! Write options must change the bytes on disk and must not change the data.
//! The default options have to reproduce what Octa wrote before they existed,
//! or every save path silently changes behaviour.

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;
use octa::formats::write_options::{CsvOptions, ParquetOptions, WriteOptions};

fn sample() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = (0..500)
        .map(|i| vec![CellValue::Int(i), CellValue::String(format!("row {i}"))])
        .collect();
    t
}

fn dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join("octa_write_options_tests");
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn parquet_compression_changes_the_file_but_not_the_data() {
    let registry = FormatRegistry::new();
    let table = sample();

    let mut sizes = Vec::new();
    for codec in ["uncompressed", "zstd"] {
        let path = dir().join(format!("out_{codec}.parquet"));
        let opts = WriteOptions {
            parquet: ParquetOptions {
                compression: codec.to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let reader = registry.reader_for_path(&path).unwrap();
        reader
            .write_file_with_options(&path, &table, &opts)
            .unwrap();

        let back = reader.read_file(&path).unwrap();
        assert_eq!(back.row_count(), 500);
        assert_eq!(back.get(499, 1), Some(&CellValue::String("row 499".into())));
        sizes.push(std::fs::metadata(&path).unwrap().len());
    }
    assert!(sizes[1] < sizes[0], "zstd should be smaller: {sizes:?}");
}

#[test]
fn parquet_row_group_size_is_honoured() {
    let path = dir().join("grouped.parquet");
    let opts = WriteOptions {
        parquet: ParquetOptions {
            row_group_size: Some(100),
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader
        .write_file_with_options(&path, &sample(), &opts)
        .unwrap();

    let internals = octa::data::file_internals::inspect(&path).unwrap();
    let facts: std::collections::HashMap<String, String> =
        internals.facts.iter().cloned().collect();
    assert_eq!(facts.get("row_groups").map(String::as_str), Some("5"));
}

#[test]
fn csv_options_control_delimiter_and_line_ending() {
    let path = dir().join("out.csv");
    let opts = WriteOptions {
        csv: CsvOptions {
            delimiter: b';',
            crlf: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader
        .write_file_with_options(&path, &sample(), &opts)
        .unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("id;name\r\n"), "got: {:?}", &body[..12]);
}

#[test]
fn a_european_csv_keeps_its_decimal_comma_and_byte_order_mark() {
    let mut t = DataTable::empty();
    t.columns = ["amount", "count", "note"]
        .iter()
        .zip(["Float64", "Int64", "Utf8"])
        .map(|(n, ty)| ColumnInfo {
            name: n.to_string(),
            data_type: ty.into(),
        })
        .collect();
    t.rows = vec![
        vec![
            CellValue::Float(1234.56),
            CellValue::Int(7),
            // Text that looks like a number is the user's text: never rewritten.
            CellValue::String("1.5".into()),
        ],
        vec![
            CellValue::Float(15.5),
            CellValue::Int(-3),
            CellValue::String("Grüße".into()),
        ],
    ];
    let path = dir().join("european.csv");
    let opts = WriteOptions {
        csv: CsvOptions {
            delimiter: b';',
            decimal_comma: true,
            bom: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader.write_file_with_options(&path, &t, &opts).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], b"\xEF\xBB\xBF", "byte order mark first");
    let body = String::from_utf8(bytes[3..].to_vec()).unwrap();
    assert_eq!(body, "amount;count;note\n1234,56;7;1.5\n15,5;-3;Grüße\n",);

    // And Octa reads it back as the same numbers.
    let back = octa::formats::read_table_auto(&path, None, u64::MAX).unwrap();
    assert_eq!(
        back.columns[0].name, "amount",
        "the BOM is not part of the name"
    );
    assert_eq!(back.get(0, 0), Some(&CellValue::Float(1234.56)));
    assert_eq!(back.get(1, 0), Some(&CellValue::Float(15.5)));
}

#[test]
fn a_tsv_keeps_its_tab_whatever_the_saved_delimiter_says() {
    // A saved delimiter is a preference about CSV. A `.tsv` is tab-separated
    // by definition, so the extension wins: writing `;` into a file the user
    // named `.tsv` produces something no TSV reader parses as intended.
    let path = dir().join("saved_delim.tsv");
    let opts = WriteOptions {
        csv: CsvOptions {
            delimiter: b';',
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader
        .write_file_with_options(&path, &sample(), &opts)
        .unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.starts_with("id\tname\n"),
        "a .tsv must stay tab-separated, got: {:?}",
        &body[..12]
    );

    // The same setting still reaches a .csv, or it would be inert.
    let csv_path = dir().join("saved_delim.csv");
    let csv_reader = registry.reader_for_path(&csv_path).unwrap();
    csv_reader
        .write_file_with_options(&csv_path, &sample(), &opts)
        .unwrap();
    let csv_body = std::fs::read_to_string(&csv_path).unwrap();
    assert!(
        csv_body.starts_with("id;name\n"),
        "a .csv must honour the saved delimiter, got: {:?}",
        &csv_body[..12]
    );
}

#[test]
fn csv_header_can_be_suppressed() {
    let path = dir().join("headerless.csv");
    let opts = WriteOptions {
        csv: CsvOptions {
            write_header: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader
        .write_file_with_options(&path, &sample(), &opts)
        .unwrap();

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("0,row 0\n"), "got: {:?}", &body[..12]);
}

#[test]
fn default_options_reproduce_the_current_writer() {
    let registry = FormatRegistry::new();
    for name in ["default.csv", "default.parquet"] {
        let a = dir().join(format!("a_{name}"));
        let b = dir().join(format!("b_{name}"));
        let reader = registry.reader_for_path(&a).unwrap();
        reader.write_file(&a, &sample()).unwrap();
        reader
            .write_file_with_options(&b, &sample(), &WriteOptions::default())
            .unwrap();
        assert_eq!(
            std::fs::read(&a).unwrap(),
            std::fs::read(&b).unwrap(),
            "{name}: default options must match the plain writer byte for byte"
        );
    }
}
