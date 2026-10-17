use super::*;
use octa::data::ColumnInfo;

/// Table with one awkward cell per format concern: a comma (CSV
/// quoting), a double quote (CSV doubling), an embedded tab and
/// newline (TSV sanitising), and a Null (empty vs JSON null).
fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = ["id", "label", "amount", "ok", "note"]
        .iter()
        .zip(["Int64", "Utf8", "Float64", "Boolean", "Utf8"])
        .map(|(name, data_type)| ColumnInfo {
            name: (*name).to_string(),
            data_type: data_type.to_string(),
        })
        .collect();
    t.rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("a,b".to_string()),
            CellValue::Float(1.5),
            CellValue::Bool(true),
            CellValue::Null,
        ],
        vec![
            CellValue::Int(2),
            CellValue::String("say \"hi\"".to_string()),
            CellValue::Float(2.0),
            CellValue::Bool(false),
            CellValue::String("one\ttwo\nthree".to_string()),
        ],
    ];
    t
}

fn render(f: OutputFormat) -> String {
    let t = table();
    let mut buf: Vec<u8> = Vec::new();
    match f {
        OutputFormat::Tsv => write_delimited(&mut buf, &t, b'\t').unwrap(),
        OutputFormat::Csv => write_csv(&mut buf, &t).unwrap(),
        OutputFormat::Json => write_json(&mut buf, &t).unwrap(),
    }
    String::from_utf8(buf).unwrap()
}

#[test]
fn tsv_writes_header_and_keeps_one_row_per_line() {
    let out = render(OutputFormat::Tsv);
    let lines: Vec<&str> = out.lines().collect();
    // Header + two rows, and nothing else: the embedded newline in
    // the last cell must not have split a row in two.
    assert_eq!(lines.len(), 3, "unexpected line count in:\n{out}");
    assert_eq!(lines[0], "id\tlabel\tamount\tok\tnote");
    // Null renders as an empty field, so the row ends with a tab.
    assert_eq!(lines[1], "1\ta,b\t1.5\ttrue\t");
    // TAB -> two spaces, NEWLINE -> one space (TSV has no escape).
    assert_eq!(lines[2], "2\tsay \"hi\"\t2.0\tfalse\tone  two three");
}

#[test]
fn csv_quotes_per_rfc_4180() {
    let out = render(OutputFormat::Csv);
    assert!(out.starts_with("id,label,amount,ok,note\n"), "{out}");
    // Comma-bearing field is quoted; the trailing Null is an empty field.
    assert!(out.contains("1,\"a,b\",1.5,true,\n"), "{out}");
    // Internal quotes doubled, embedded newline preserved inside quotes.
    assert!(out.contains("\"say \"\"hi\"\"\""), "{out}");
    assert!(out.contains("\"one\ttwo\nthree\""), "{out}");
}

#[test]
fn json_emits_native_types_and_null() {
    let out = render(OutputFormat::Json);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], serde_json::json!(1));
    assert_eq!(rows[0]["label"], serde_json::json!("a,b"));
    assert_eq!(rows[0]["amount"], serde_json::json!(1.5));
    assert_eq!(rows[0]["ok"], serde_json::json!(true));
    // Null becomes JSON null, not the empty string TSV/CSV use.
    assert!(rows[0]["note"].is_null());
    // JSON keeps the raw cell text; only TSV sanitises it.
    assert_eq!(rows[1]["note"], serde_json::json!("one\ttwo\nthree"));
}

#[test]
fn json_maps_non_finite_floats_to_null() {
    // `serde_json::Number::from_f64` rejects NaN / inf; the fallback
    // must be `null` rather than a panic or invalid JSON.
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".to_string(),
        data_type: "Float64".to_string(),
    }];
    t.rows = vec![
        vec![CellValue::Float(f64::NAN)],
        vec![CellValue::Float(f64::INFINITY)],
    ];
    let mut buf: Vec<u8> = Vec::new();
    write_json(&mut buf, &t).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&buf).unwrap();
    assert!(rows[0]["v"].is_null());
    assert!(rows[1]["v"].is_null());
}

#[test]
fn empty_table_still_writes_a_header() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "only".to_string(),
        data_type: "Utf8".to_string(),
    }];
    let mut tsv: Vec<u8> = Vec::new();
    write_delimited(&mut tsv, &t, b'\t').unwrap();
    assert_eq!(String::from_utf8(tsv).unwrap(), "only\n");
    // JSON's empty case is an empty array, not an empty document.
    let mut json: Vec<u8> = Vec::new();
    write_json(&mut json, &t).unwrap();
    assert_eq!(String::from_utf8(json).unwrap().trim(), "[]");
}
