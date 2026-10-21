use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::csv_reader::*;
use std::io::Write;
use tempfile::NamedTempFile;

// --- infer_cell_value ---

#[test]
fn test_infer_empty_is_null() {
    assert_eq!(infer_cell_value(""), CellValue::Null);
}

#[test]
fn test_infer_bool() {
    assert_eq!(infer_cell_value("true"), CellValue::Bool(true));
    assert_eq!(infer_cell_value("false"), CellValue::Bool(false));
    assert_eq!(infer_cell_value("TRUE"), CellValue::Bool(true));
    assert_eq!(infer_cell_value("False"), CellValue::Bool(false));
}

#[test]
fn test_infer_int() {
    assert_eq!(infer_cell_value("42"), CellValue::Int(42));
    assert_eq!(infer_cell_value("-7"), CellValue::Int(-7));
    assert_eq!(infer_cell_value("0"), CellValue::Int(0));
}

#[test]
fn test_infer_float() {
    assert_eq!(infer_cell_value("2.5"), CellValue::Float(2.5));
    assert_eq!(infer_cell_value("-0.5"), CellValue::Float(-0.5));
}

#[test]
fn test_infer_date() {
    assert_eq!(
        infer_cell_value("2024-01-15"),
        CellValue::Date("2024-01-15".into())
    );
}

#[test]
fn test_infer_datetime() {
    assert_eq!(
        infer_cell_value("2024-01-15 10:30:00"),
        CellValue::DateTime("2024-01-15 10:30:00".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00"),
        CellValue::DateTime("2024-01-15T10:30:00".into())
    );
}

#[test]
fn test_infer_datetime_with_timezone() {
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00Z"),
        CellValue::DateTime("2024-01-15T10:30:00Z".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00+01:00"),
        CellValue::DateTime("2024-01-15T10:30:00+01:00".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.123Z"),
        CellValue::DateTime("2024-01-15T10:30:00.123Z".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.123+05:30"),
        CellValue::DateTime("2024-01-15T10:30:00.123+05:30".into())
    );
}

#[test]
fn test_infer_datetime_with_fractional_seconds() {
    assert_eq!(
        infer_cell_value("2024-01-15 10:30:00.123"),
        CellValue::DateTime("2024-01-15 10:30:00.123".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.456789"),
        CellValue::DateTime("2024-01-15T10:30:00.456789".into())
    );
}

#[test]
fn test_infer_string_fallback() {
    assert_eq!(
        infer_cell_value("hello world"),
        CellValue::String("hello world".into())
    );
}

// --- detect_delimiter ---

#[test]
fn test_detect_comma_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a,b,c").unwrap();
    writeln!(f, "1,2,3").unwrap();
    writeln!(f, "4,5,6").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b','));
}

#[test]
fn test_detect_semicolon_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a;b;c").unwrap();
    writeln!(f, "1;2;3").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b';'));
}

#[test]
fn test_detect_tab_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a\tb\tc").unwrap();
    writeln!(f, "1\t2\t3").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b'\t'));
}

#[test]
fn test_detect_empty_file_returns_none() {
    let f = NamedTempFile::new().unwrap();
    assert_eq!(detect_delimiter(f.path()), None);
}

#[test]
fn test_detect_semicolon_below_title_lines() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "Sales report Q3 2025").unwrap();
    writeln!(f, "Created 03.10.2026").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "Region;Revenue;Share").unwrap();
    writeln!(f, "North;1.234,56;0,25").unwrap();
    writeln!(f, "South;987,10;0,5").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b';'));
}

#[test]
fn test_detect_semicolon_below_a_title_with_a_comma() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "Sales report, Q3 2025").unwrap();
    writeln!(f, "Region;Revenue;Share").unwrap();
    writeln!(f, "North;1.234,56;0,25").unwrap();
    writeln!(f, "South;987,10;0,5").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b';'));
}

#[test]
fn test_detect_comma_below_title_line() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "Monthly figures").unwrap();
    writeln!(f, "id,name,amount").unwrap();
    writeln!(f, "1,Anna,10").unwrap();
    writeln!(f, "2,Ben,20").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b','));
}

#[test]
fn test_detect_two_column_european_file_stays_semicolon() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a;b").unwrap();
    writeln!(f, "1,5;2,5").unwrap();
    writeln!(f, "3,5;4,5").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b';'));
}

#[test]
fn test_detect_delimiter_in_needs_two_agreeing_lines() {
    // A title with one comma above a table without commas is not a CSV.
    assert_eq!(detect_delimiter_in("Report, Q3\nfoo\nbar\n"), None);
    // A single header line still counts, as before.
    assert_eq!(detect_delimiter_in("a,b,c\n"), Some(b','));
}

// --- read/write round-trip ---

#[test]
fn test_csv_round_trip() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "name,age,active").unwrap();
    writeln!(f, "Alice,30,true").unwrap();
    writeln!(f, "Bob,25,false").unwrap();

    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.columns[0].name, "name");
    assert_eq!(table.get(0, 0), Some(&CellValue::String("Alice".into())));
    assert_eq!(table.get(0, 1), Some(&CellValue::Int(30)));
    assert_eq!(table.get(1, 2), Some(&CellValue::Bool(false)));

    let out = NamedTempFile::with_suffix(".csv").unwrap();
    CsvReader.write_file(out.path(), &table).unwrap();
    let table2 = CsvReader.read_file(out.path()).unwrap();
    assert_eq!(table2.row_count(), 2);
    assert_eq!(table2.col_count(), 3);
    assert_eq!(table2.get(0, 0), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_tsv_round_trip() {
    let mut f = NamedTempFile::with_suffix(".tsv").unwrap();
    writeln!(f, "x\ty").unwrap();
    writeln!(f, "1\t2").unwrap();

    let table = TsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));

    let out = NamedTempFile::with_suffix(".tsv").unwrap();
    TsvReader.write_file(out.path(), &table).unwrap();
    let table2 = TsvReader.read_file(out.path()).unwrap();
    assert_eq!(table2.get(0, 0), Some(&CellValue::Int(1)));
}

// --- column type refinement ---

#[test]
fn test_column_type_refinement_int() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "1").unwrap();
    writeln!(f, "2").unwrap();
    writeln!(f, "3").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Int64");
}

#[test]
fn test_column_type_refinement_float() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "1.5").unwrap();
    writeln!(f, "2.5").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Float64");
}

#[test]
fn test_column_type_refinement_bool() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "true").unwrap();
    writeln!(f, "false").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Boolean");
}

#[test]
fn test_column_type_mixed_becomes_string() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "42").unwrap();
    writeln!(f, "hello").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Utf8");
}

// --- malformed-file repair (analyze_delimited + read_delimited_opts) ---

#[test]
fn test_analyze_healthy_csv_is_none() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "a,b,c").unwrap();
    writeln!(f, "1,2,3").unwrap();
    writeln!(f, "4,5,6").unwrap();
    assert!(analyze_delimited(f.path(), b',').is_none());
}

#[test]
fn test_analyze_flags_bad_encoding() {
    use std::io::Write as _;
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    // Valid header + a Latin-1 0xE9 ("é") byte that isn't valid UTF-8.
    f.write_all(b"name,city\nJose,M\xE9xico\n").unwrap();
    let plan = analyze_delimited(f.path(), b',').expect("malformed");
    assert!(plan.options.lossy_utf8);
    // The repaired read succeeds where a strict read would fail.
    let table = read_delimited_opts(f.path(), b',', "CSV", &plan.options).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.columns.len(), 2);
}

#[test]
fn a_byte_order_mark_alone_is_not_malformed() {
    use std::io::Write as _;
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    f.write_all(b"\xEF\xBB\xBFa,b\n1,2\n").unwrap();
    // Octa writes this itself on request: it must not ask for a repair.
    assert!(analyze_delimited(f.path(), b',').is_none());
    // And the plain read already drops it from the first header.
    let table = read_delimited_opts(f.path(), b',', "CSV", &Default::default()).unwrap();
    assert_eq!(table.columns[0].name, "a");
}

#[test]
fn test_analyze_flags_tsv_wrong_delimiter() {
    let mut f = NamedTempFile::with_suffix(".tsv").unwrap();
    // A ".tsv" that is actually semicolon-delimited.
    writeln!(f, "a;b;c").unwrap();
    writeln!(f, "1;2;3").unwrap();
    let plan = analyze_delimited(f.path(), b'\t').expect("malformed");
    assert_eq!(plan.options.delimiter, Some(b';'));
    let table = read_delimited_opts(f.path(), b'\t', "TSV", &plan.options).unwrap();
    assert_eq!(table.columns.len(), 3);
}

#[test]
fn test_examples_malformed_csv_triggers_repair() {
    // The shipped demo file `samples/features/malformed.csv` must keep tripping the
    // repair analyzer (it backs the docs walkthrough). Guards against someone
    // "tidying" it into a clean file.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/features/malformed.csv");
    let plan = analyze_delimited(&path, b',').expect("example should be malformed");
    // A control character and ragged rows; its BOM is not an issue.
    assert!(plan.options.strip_bom_controls, "control char");
    assert!(
        !plan.issues.iter().any(|i| i.contains("BOM")),
        "issues: {:?}",
        plan.issues
    );
    assert!(
        plan.issues.iter().any(|i| i.contains("control")),
        "issues: {:?}",
        plan.issues
    );
    assert!(
        plan.issues.iter().any(|i| i.contains("inconsistent")),
        "issues: {:?}",
        plan.issues
    );
    // And it still loads once repaired.
    assert!(read_delimited_opts(&path, b',', "CSV", &plan.options).is_ok());
}

#[test]
fn test_preserve_ragged_keeps_extra_fields() {
    // The repair must not lose data: a row with more fields than the header
    // gets its extra value kept in a new column, not silently dropped.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/features/malformed.csv");
    let plan = analyze_delimited(&path, b',').expect("example should be malformed");
    assert!(
        plan.options.preserve_ragged,
        "ragged file should opt into widening"
    );
    let table = read_delimited_opts(&path, b',', "CSV", &plan.options).unwrap();
    // Header had 3 columns; the widest data row has 4, so a 4th is synthesised.
    assert_eq!(table.columns.len(), 4);
    assert_eq!(table.columns[3].name, "column_4");
    // Rows in order: alice, bob, carol, dave. carol's extra value survives.
    assert_eq!(
        table.get(2, 3).map(|v| v.to_string()),
        Some("extra".to_string())
    );
    assert_eq!(
        table.get(3, 1).map(|v| v.to_string()),
        Some("dave".to_string())
    );
    // dave's missing trailing fields pad with Null.
    assert!(matches!(table.get(3, 3), None | Some(CellValue::Null)));
}

#[test]
fn test_trim_to_header_when_not_preserving() {
    // Default (preserve_ragged off) keeps the historical trim-to-header shape.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/features/malformed.csv");
    let opts = ReadOptions {
        strip_bom_controls: true,
        ..Default::default()
    };
    let table = read_delimited_opts(&path, b',', "CSV", &opts).unwrap();
    assert_eq!(table.columns.len(), 3);
}

#[test]
fn test_read_delimited_opts_default_matches_normal() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "a,b").unwrap();
    writeln!(f, "1,2").unwrap();
    let opts = ReadOptions::default();
    let table = read_delimited_opts(f.path(), b',', "CSV", &opts).unwrap();
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.row_count(), 1);
}

// --- table width follows the data ---

fn read_csv(text: &str) -> octa::data::DataTable {
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    f.write_all(text.as_bytes()).unwrap();
    CsvReader.read_file(f.path()).unwrap()
}

fn col_names(t: &octa::data::DataTable) -> Vec<String> {
    t.columns.iter().map(|c| c.name.clone()).collect()
}

#[test]
fn test_title_line_keeps_every_field() {
    let t = read_csv(
        "Sales report Q3 2025\nRegion;Revenue;Share\nNorth;1.234,56;0,25\nSouth;987,10;0,5\n",
    );
    assert_eq!(
        col_names(&t),
        ["Sales report Q3 2025", "column_2", "column_3"]
    );
    let header = (0..t.row_count())
        .find(|&r| t.get(r, 0) == Some(&CellValue::String("Region".into())))
        .expect("the real header is a row");
    assert_eq!(t.get(header, 2), Some(&CellValue::String("Share".into())));
    assert_eq!(
        t.get(header + 1, 1),
        Some(&CellValue::String("1.234,56".into()))
    );
}

#[test]
fn test_trailing_delimiter_adds_no_empty_column() {
    let t = read_csv("a;b;c\n1;2;3;\n4;5;6;\n");
    assert_eq!(col_names(&t), ["a", "b", "c"]);
}

#[test]
fn test_a_few_long_rows_do_not_widen_the_table() {
    let t = read_csv("a,b,c\n1,2,3\n4,5,6,7\n");
    assert_eq!(t.col_count(), 3);
}

#[test]
fn test_title_block_csv_end_to_end() {
    use octa::data::header_rows::{HeaderSuggestion, suggest, use_header_rows};
    let mut t = read_csv(
        "Sales report Q3 2025\nCreated 03.10.2026\n\nRegion;Revenue;Share\nNorth;1.234,56;0,25\nSouth;987,10;0,5\n",
    );
    let s = suggest(&t).expect("the Region row is offered");
    assert_eq!(t.get(s.last, 0), Some(&CellValue::String("Region".into())));
    assert_eq!(
        s,
        HeaderSuggestion {
            first: s.last,
            last: s.last
        }
    );
    use_header_rows(&mut t, s.first, s.last, true).unwrap();
    assert_eq!(col_names(&t), ["Region", "Revenue", "Share"]);
    let types: Vec<&str> = t.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(types, ["Utf8", "Float64", "Float64"]);
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(1234.56)));
}

/// Background chunks of a CSV: each one carries on from the byte position the
/// last one stopped at, and together they hold every record exactly once,
/// a quoted line break included (records, not lines).
#[test]
fn csv_chunks_resume_where_the_last_one_stopped() {
    use octa::formats::csv_reader::{ChunkResume, ChunkSink, ChunkSpec, load_csv_rows_chunk};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chunks.csv");
    let mut text = String::from("id,note\n");
    for i in 0..10 {
        if i == 4 {
            text.push_str("4,\"two\nlines\"\n");
        } else {
            text.push_str(&format!("{i},n{i}\n"));
        }
    }
    std::fs::write(&path, text).unwrap();

    let resume: ChunkResume = Default::default();
    let chunk = |skip_rows: usize, resume: &ChunkResume| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        load_csv_rows_chunk(
            &path,
            b',',
            ChunkSpec {
                skip_rows,
                max_rows: 3,
                num_cols: 2,
            },
            ChunkSink {
                buffer: buffer.clone(),
                done: Arc::new(AtomicBool::new(false)),
                exhausted: Arc::new(AtomicBool::new(false)),
                resume: resume.clone(),
            },
        )
        .unwrap();
        let rows = std::mem::take(&mut *buffer.lock().unwrap());
        rows.iter().map(|r| r[0].to_string()).collect::<Vec<_>>()
    };

    // The first chunk has no position yet and skips by counting.
    assert_eq!(chunk(1, &resume), ["1", "2", "3"]);
    assert_eq!(resume.lock().unwrap().as_ref().unwrap().0, 4);
    assert_eq!(chunk(4, &resume), ["4", "5", "6"]);
    assert_eq!(chunk(7, &resume), ["7", "8", "9"]);

    // The position is really used: keyed to another count, it still wins
    // over counting from the start.
    let pos = resume.lock().unwrap().clone().unwrap().1;
    let after_six: ChunkResume = Default::default();
    *after_six.lock().unwrap() = Some((4, pos));
    assert!(
        chunk(4, &after_six).is_empty(),
        "seeked to the end, not row 4"
    );
}
