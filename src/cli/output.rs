//! Shared output formatters for CLI subcommands. Each writes to stdout
//! directly - the subcommands themselves never touch `println!` for table
//! data, so adding a new output format is a one-place change.

use std::io::{self, Write};

use octa::data::{CellValue, DataTable};

use super::OutputFormat;

/// Write `table` to stdout in the requested format. Includes the header
/// row for TSV / CSV; JSON is a single array of `{column: value}` objects.
pub fn write_table(table: &DataTable, format: OutputFormat) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Tsv => write_delimited(&mut out, table, b'\t')?,
        OutputFormat::Csv => write_csv(&mut out, table)?,
        OutputFormat::Json => write_json(&mut out, table)?,
    }
    Ok(())
}

/// Tab-separated values. Field-internal tabs are replaced with two spaces
/// so the row count never blows out - TSV has no escape mechanism, and
/// silently corrupting cells with embedded tabs would be worse than the
/// readability loss from the substitution.
fn write_delimited(w: &mut impl Write, table: &DataTable, delim: u8) -> io::Result<()> {
    let dch = delim as char;
    let mut header = String::new();
    for (i, col) in table.columns.iter().enumerate() {
        if i > 0 {
            header.push(dch);
        }
        header.push_str(&sanitize_tsv_cell(&col.name));
    }
    writeln!(w, "{header}")?;
    for row in 0..table.row_count() {
        let mut line = String::new();
        for col in 0..table.col_count() {
            if col > 0 {
                line.push(dch);
            }
            let text = cell_to_string(table.get(row, col));
            line.push_str(&sanitize_tsv_cell(&text));
        }
        writeln!(w, "{line}")?;
    }
    Ok(())
}

/// RFC 4180 CSV writer. Uses the existing `csv` crate so quoting rules
/// match the rest of Octa's CSV behaviour - fields with comma, quote, or
/// newline get wrapped, internal quotes are doubled.
fn write_csv(w: &mut impl Write, table: &DataTable) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(w);
    wtr.write_record(table.columns.iter().map(|c| c.name.as_str()))?;
    for row in 0..table.row_count() {
        let row_strs: Vec<String> = (0..table.col_count())
            .map(|col| cell_to_string(table.get(row, col)))
            .collect();
        wtr.write_record(&row_strs)?;
    }
    wtr.flush()?;
    Ok(())
}

/// JSON array of objects, two-space indented for readability. Numeric and
/// boolean cells are emitted as their native JSON types; everything else
/// (dates, blobs, nested values) falls back to its string representation
/// so the output is always a valid JSON document.
fn write_json(w: &mut impl Write, table: &DataTable) -> anyhow::Result<()> {
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    let mut rows: Vec<serde_json::Value> = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        let mut map = serde_json::Map::with_capacity(table.col_count());
        for (col, &name) in names.iter().enumerate() {
            map.insert(name.to_string(), cell_to_json(table.get(row, col)));
        }
        rows.push(serde_json::Value::Object(map));
    }
    let text = serde_json::to_string_pretty(&rows)?;
    writeln!(w, "{text}")?;
    Ok(())
}

fn cell_to_string(cell: Option<&CellValue>) -> String {
    match cell {
        Some(CellValue::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

fn cell_to_json(cell: Option<&CellValue>) -> serde_json::Value {
    use serde_json::Value;
    match cell {
        Some(CellValue::Null) | None => Value::Null,
        Some(CellValue::Bool(b)) => Value::Bool(*b),
        Some(CellValue::Int(i)) => Value::from(*i),
        Some(CellValue::Float(f)) => {
            serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number)
        }
        Some(other) => Value::String(other.to_string()),
    }
}

/// TSV has no escape; replace TAB and NEWLINE characters in cells with
/// spaces so each row stays on one line. Matches the convention used by
/// most shell tools (column, awk, etc.).
fn sanitize_tsv_cell(s: &str) -> String {
    s.replace('\t', "  ").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
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
}
