use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result};
use calamine::{Data, Reader, open_workbook_auto};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

pub struct OdsReader;

impl FormatReader for OdsReader {
    fn name(&self) -> &str {
        "ODS"
    }

    fn extensions(&self) -> &[&str] {
        &["ods"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut workbook = open_workbook_auto(path)?;
        let sheet_names = workbook.sheet_names().to_vec();

        if sheet_names.is_empty() {
            return Ok(DataTable::empty());
        }

        let range = workbook
            .worksheet_range(&sheet_names[0])
            .map_err(|e| anyhow::anyhow!("Failed to read sheet: {}", e))?;

        let mut rows_iter = range.rows();

        let header_row = match rows_iter.next() {
            Some(r) => r,
            None => return Ok(DataTable::empty()),
        };

        let columns: Vec<ColumnInfo> = header_row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let name = match cell {
                    Data::String(s) => s.clone(),
                    Data::Float(f) => format!("{}", f),
                    Data::Int(i) => format!("{}", i),
                    Data::Bool(b) => format!("{}", b),
                    _ => format!("Column{}", i + 1),
                };
                ColumnInfo {
                    name,
                    data_type: "Utf8".to_string(),
                }
            })
            .collect();

        let col_count = columns.len();
        let mut rows: Vec<Vec<CellValue>> = Vec::new();

        for row in rows_iter {
            let mut cells: Vec<CellValue> = row
                .iter()
                .map(|cell| match cell {
                    Data::Empty => CellValue::Null,
                    Data::String(s) => CellValue::String(s.clone()),
                    Data::Float(f) => CellValue::Float(*f),
                    Data::Int(i) => CellValue::Int(*i),
                    Data::Bool(b) => CellValue::Bool(*b),
                    Data::DateTime(dt) => CellValue::DateTime(format!("{}", dt)),
                    Data::DateTimeIso(s) => CellValue::DateTime(s.clone()),
                    Data::DurationIso(s) => CellValue::String(s.clone()),
                    Data::Error(e) => CellValue::String(format!("#ERR: {:?}", e)),
                })
                .collect();
            cells.resize(col_count, CellValue::Null);
            rows.push(cells);
        }

        let mut final_columns = columns;
        for (col_idx, col) in final_columns.iter_mut().enumerate() {
            let mut has_int = false;
            let mut has_float = false;
            let mut has_bool = false;
            let mut has_datetime = false;
            let mut has_string = false;

            for row in &rows {
                match row.get(col_idx) {
                    Some(CellValue::Int(_)) => has_int = true,
                    Some(CellValue::Float(_)) => has_float = true,
                    Some(CellValue::Bool(_)) => has_bool = true,
                    Some(CellValue::DateTime(_)) => has_datetime = true,
                    Some(CellValue::String(_)) => has_string = true,
                    _ => {}
                }
            }

            col.data_type = if has_string {
                "Utf8".to_string()
            } else if has_datetime {
                "Timestamp(Microsecond, None)".to_string()
            } else if has_float {
                "Float64".to_string()
            } else if has_int {
                "Int64".to_string()
            } else if has_bool {
                "Boolean".to_string()
            } else {
                "Utf8".to_string()
            };
        }

        Ok(DataTable {
            columns: final_columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("ODS".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create ODS file: {}", path.display()))?;
        let mut zip = ZipWriter::new(file);

        // Per OpenDocument 1.2 §17.4, `mimetype` must be the first entry,
        // stored uncompressed with no extra fields. Office readers (LibreOffice,
        // Excel) rely on this signature to identify the file type without
        // unpacking the whole archive.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored)?;
        zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")?;

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        zip.start_file("META-INF/manifest.xml", deflated)?;
        zip.write_all(MANIFEST_XML.as_bytes())?;

        zip.start_file("content.xml", deflated)?;
        write_content_xml(&mut zip, table)?;

        zip.finish()?;
        Ok(())
    }
}

const MANIFEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">
 <manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"#;

fn write_content_xml<W: Write>(out: &mut W, table: &DataTable) -> Result<()> {
    // No indentation/newlines inside <table:table-row> — calamine's parser
    // treats whitespace text nodes between cells as a structural error.
    out.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2"><office:body><office:spreadsheet><table:table table:name="Sheet1">"#)?;

    if !table.columns.is_empty() {
        write!(
            out,
            "<table:table-column table:number-columns-repeated=\"{}\"/>",
            table.columns.len()
        )?;
    }

    // Header row.
    out.write_all(b"<table:table-row>")?;
    for col in &table.columns {
        write_string_cell(out, &col.name)?;
    }
    out.write_all(b"</table:table-row>")?;

    // Data rows.
    let col_count = table.col_count();
    for row_idx in 0..table.row_count() {
        out.write_all(b"<table:table-row>")?;
        for col_idx in 0..col_count {
            match table.get(row_idx, col_idx) {
                Some(CellValue::Null) | None => out.write_all(b"<table:table-cell/>")?,
                Some(CellValue::Int(i)) => write_float_cell(out, *i as f64, &i.to_string())?,
                Some(CellValue::Float(f)) => write_float_cell(out, *f, &f.to_string())?,
                Some(CellValue::Bool(b)) => write_bool_cell(out, *b)?,
                Some(other) => write_string_cell(out, &other.to_string())?,
            }
        }
        out.write_all(b"</table:table-row>")?;
    }

    out.write_all(
        b"</table:table></office:spreadsheet></office:body></office:document-content>\n",
    )?;
    Ok(())
}

fn write_string_cell<W: Write>(out: &mut W, text: &str) -> Result<()> {
    let escaped = escape_xml_text(text);
    write!(
        out,
        "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
        escaped
    )?;
    Ok(())
}

fn write_float_cell<W: Write>(out: &mut W, value: f64, display: &str) -> Result<()> {
    // `office:value` is locale-independent (always `.` decimal separator); the
    // <text:p> child is what apps show when they don't re-format the cell.
    let value_attr = format_f64_for_attr(value);
    let display_escaped = escape_xml_text(display);
    write!(
        out,
        "<table:table-cell office:value-type=\"float\" office:value=\"{}\"><text:p>{}</text:p></table:table-cell>",
        value_attr, display_escaped
    )?;
    Ok(())
}

fn write_bool_cell<W: Write>(out: &mut W, value: bool) -> Result<()> {
    write!(
        out,
        "<table:table-cell office:value-type=\"boolean\" office:boolean-value=\"{}\"><text:p>{}</text:p></table:table-cell>",
        value, value
    )?;
    Ok(())
}

fn format_f64_for_attr(value: f64) -> String {
    if !value.is_finite() {
        // ODS spec has no NaN/Inf encoding for numeric cells; emit 0 so the
        // file stays valid. Callers can avoid this by stringifying NaN before
        // reaching the writer.
        return "0".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{}", value as i64)
    } else {
        format!("{}", value)
    }
}

fn escape_xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip control characters that aren't valid in XML 1.0
            // (everything below 0x20 except tab / LF / CR).
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_xml_text_basic() {
        assert_eq!(escape_xml_text("a & b"), "a &amp; b");
        assert_eq!(escape_xml_text("<tag>"), "&lt;tag&gt;");
        assert_eq!(
            escape_xml_text("she said \"hi\""),
            "she said &quot;hi&quot;"
        );
    }

    #[test]
    fn escape_xml_text_strips_control_chars() {
        assert_eq!(escape_xml_text("a\x01b\x02"), "ab");
        // Tab/LF/CR must survive
        assert_eq!(escape_xml_text("a\tb\nc\rd"), "a\tb\nc\rd");
    }

    #[test]
    fn format_f64_renders_integer_form_when_exact() {
        assert_eq!(format_f64_for_attr(3.0), "3");
        assert_eq!(format_f64_for_attr(3.5), "3.5");
        assert_eq!(format_f64_for_attr(-2.0), "-2");
    }

    #[test]
    fn format_f64_handles_non_finite() {
        assert_eq!(format_f64_for_attr(f64::NAN), "0");
        assert_eq!(format_f64_for_attr(f64::INFINITY), "0");
    }
}
