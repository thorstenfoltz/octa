//! Shared output formatters for CLI subcommands. Each writes to stdout
//! directly — the subcommands themselves never touch `println!` for table
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
/// so the row count never blows out — TSV has no escape mechanism, and
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
/// match the rest of Octa's CSV behaviour — fields with comma, quote, or
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
