//! QlikView data file (`.qvd`) reader. Read-only, one table per file.
//!
//! A QVD is three parts: an XML header describing the fields, then one
//! symbol table per field (its distinct values), then the rows as bit-packed
//! indexes into those symbol tables. A symbol is a number, a text, or both
//! (Qlik's "dual": the number plus the text it is shown as).
//!
//! Column types: a field whose symbols are all numbers is `Date`,
//! `DateTime`, `Duration(s)` (TIME / INTERVAL), `Int64` or `Float64`,
//! decided by the header's number format and tags. A dual whose text is not
//! just the number written out (`007`, a currency text) makes the column
//! text, so nothing shown in Qlik is lost.
//!
//! The header is parsed with the XML reader's tree (`xml_reader::parse_tree`).

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::data::times::NANOS_PER_SEC;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use crate::formats::sas_reader::format_unix_secs;
use crate::formats::xml_reader::{XmlElement, parse_tree};

pub struct QvdReader;

impl FormatReader for QvdReader {
    fn name(&self) -> &str {
        "QVD"
    }

    fn extensions(&self) -> &[&str] {
        &["qvd"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let bytes =
            std::fs::read(path).with_context(|| format!("reading QVD {}", path.display()))?;
        let mut table =
            read_bytes(&bytes).with_context(|| format!("parsing QVD {}", path.display()))?;
        table.source_path = Some(path.to_string_lossy().to_string());
        Ok(table)
    }
}

const HEADER_END: &[u8] = b"</QvdTableHeader>";

/// Qlik counts days from 1899-12-30; this is 1970-01-01 on that scale.
const UNIX_EPOCH_SERIAL: f64 = 25_569.0;

enum Symbol {
    Int(i32),
    Float(f64),
    Text(String),
    DualInt(i32, String),
    DualFloat(f64, String),
}

impl Symbol {
    fn number(&self) -> Option<f64> {
        match self {
            Symbol::Int(i) | Symbol::DualInt(i, _) => Some(f64::from(*i)),
            Symbol::Float(f) | Symbol::DualFloat(f, _) => Some(*f),
            Symbol::Text(_) => None,
        }
    }

    /// The value as Qlik shows it.
    fn text(&self) -> String {
        match self {
            Symbol::Int(i) => i.to_string(),
            Symbol::Float(f) => f.to_string(),
            Symbol::Text(s) | Symbol::DualInt(_, s) | Symbol::DualFloat(_, s) => s.clone(),
        }
    }

    /// False for a dual whose text says more than its number (`007`).
    fn is_plain_number(&self) -> bool {
        match self {
            Symbol::Int(_) | Symbol::Float(_) => true,
            Symbol::DualInt(i, s) => s.trim() == i.to_string(),
            Symbol::DualFloat(f, s) => s.trim().parse::<f64>().is_ok_and(|p| p == *f),
            Symbol::Text(_) => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Date,
    DateTime,
    /// A clock time (fraction of a day); narrowed to Time when it fits a day.
    Time,
    Interval,
    Int,
    Float,
    Text,
}

impl Kind {
    fn type_name(self) -> &'static str {
        match self {
            Kind::Date => "Date",
            Kind::DateTime => "DateTime",
            Kind::Time | Kind::Interval => "Duration(s)",
            Kind::Int => "Int64",
            Kind::Float => "Float64",
            Kind::Text => "Utf8",
        }
    }
}

struct Field {
    name: String,
    kind: Kind,
    bit_offset: usize,
    bit_width: usize,
    bias: i64,
    /// One ready cell per symbol; a row clones the one its index names.
    cells: Vec<CellValue>,
}

pub(crate) fn read_bytes(bytes: &[u8]) -> Result<DataTable> {
    let end = bytes
        .windows(HEADER_END.len())
        .position(|w| w == HEADER_END)
        .ok_or_else(|| anyhow!("no QVD header found"))?
        + HEADER_END.len();
    let header = String::from_utf8_lossy(&bytes[..end]);
    let tree = parse_tree(&header).context("reading the QVD header")?;
    let root = tree
        .iter()
        .find(|e| e.name == "QvdTableHeader")
        .ok_or_else(|| anyhow!("no QVD header found"))?;

    // The data starts after the header's line break and one NUL byte.
    let mut start = end;
    while matches!(bytes.get(start), Some(b'\r' | b'\n')) {
        start += 1;
    }
    if bytes.get(start) == Some(&0) {
        start += 1;
    }
    let body = &bytes[start..];

    let record_size = number(root, "RecordByteSize")?;
    let records = number(root, "NoOfRecords")?;
    let index = slice(body, number(root, "Offset")?, number(root, "Length")?)
        .context("the row section is cut short")?;
    if records
        .checked_mul(record_size)
        .is_none_or(|n| n > index.len())
    {
        bail!("the row section is cut short");
    }

    let mut fields = Vec::new();
    for f in child(root, "Fields")
        .map(|f| f.children.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|f| f.name == "QvdFieldHeader")
    {
        let field = read_field(f, body)?;
        if field.bit_width > 64 || field.bit_offset + field.bit_width > record_size * 8 {
            bail!("field '{}' does not fit its row", field.name);
        }
        fields.push(field);
    }

    let mut rows = Vec::with_capacity(records);
    for r in 0..records {
        let rec = &index[r * record_size..(r + 1) * record_size];
        rows.push(
            fields
                .iter()
                .map(|f| {
                    let at = bits(rec, f.bit_offset, f.bit_width) as i64 + f.bias;
                    usize::try_from(at)
                        .ok()
                        .and_then(|i| f.cells.get(i))
                        .cloned()
                        .unwrap_or(CellValue::Null)
                })
                .collect(),
        );
    }

    let mut table = DataTable {
        columns: fields
            .iter()
            .map(|f| ColumnInfo {
                name: f.name.clone(),
                data_type: f.kind.type_name().to_string(),
            })
            .collect(),
        rows,
        format_name: Some("QVD".to_string()),
        ..DataTable::empty()
    };
    for (col, f) in fields.iter().enumerate() {
        if f.kind == Kind::Time {
            crate::data::times::narrow_to_time(&mut table, col);
        }
    }
    Ok(table)
}

fn read_field(f: &XmlElement, body: &[u8]) -> Result<Field> {
    let name = text(f, "FieldName").unwrap_or_default().to_string();
    let raw = slice(body, number(f, "Offset")?, number(f, "Length")?)
        .with_context(|| format!("the values of field '{name}' are cut short"))?;
    let symbols =
        read_symbols(raw).with_context(|| format!("reading the values of field '{name}'"))?;
    let format = child(f, "NumberFormat")
        .and_then(|n| text(n, "Type"))
        .unwrap_or_default()
        .to_ascii_uppercase();
    let tags: Vec<&str> = child(f, "Tags")
        .map(|t| t.children.iter().map(|c| c.text.trim()).collect())
        .unwrap_or_default();
    let kind = kind_of(&symbols, &format, &tags);
    Ok(Field {
        cells: symbols.iter().map(|s| cell(s, kind)).collect(),
        name,
        kind,
        bit_offset: number(f, "BitOffset")?,
        bit_width: number(f, "BitWidth")?,
        bias: text(f, "Bias")
            .and_then(|b| b.parse().ok())
            .unwrap_or_default(),
    })
}

fn kind_of(symbols: &[Symbol], format: &str, tags: &[&str]) -> Kind {
    let numbers: Option<Vec<f64>> = symbols.iter().map(Symbol::number).collect();
    let Some(numbers) = numbers.filter(|n| !n.is_empty()) else {
        return Kind::Text;
    };
    let whole = numbers.iter().all(|n| n.fract() == 0.0);
    match format {
        "DATE" => return Kind::Date,
        "TIMESTAMP" => return Kind::DateTime,
        "TIME" => return Kind::Time,
        "INTERVAL" => return Kind::Interval,
        _ => {}
    }
    // Qlik Sense often leaves the format UNKNOWN and says it in the tags.
    if tags.contains(&"$date") || tags.contains(&"$timestamp") {
        return if whole { Kind::Date } else { Kind::DateTime };
    }
    if !symbols.iter().all(Symbol::is_plain_number) {
        return Kind::Text;
    }
    if symbols
        .iter()
        .all(|s| matches!(s, Symbol::Int(_) | Symbol::DualInt(..)))
    {
        Kind::Int
    } else {
        Kind::Float
    }
}

fn cell(symbol: &Symbol, kind: Kind) -> CellValue {
    if kind == Kind::Text {
        return CellValue::String(symbol.text());
    }
    let Some(v) = symbol.number().filter(|v| v.is_finite()) else {
        return CellValue::Null;
    };
    match kind {
        Kind::Date => CellValue::Date(format_unix_secs(
            (v.floor() - UNIX_EPOCH_SERIAL) as i64 * 86_400,
            "%Y-%m-%d",
        )),
        Kind::DateTime => CellValue::DateTime(format_unix_secs(
            ((v - UNIX_EPOCH_SERIAL) * 86_400.0).round() as i64,
            "%Y-%m-%d %H:%M:%S",
        )),
        Kind::Time | Kind::Interval => {
            CellValue::Duration((v * 86_400.0).round() as i64 * NANOS_PER_SEC)
        }
        Kind::Int => CellValue::Int(v as i64),
        Kind::Float => CellValue::Float(v),
        Kind::Text => unreachable!("handled above"),
    }
}

fn read_symbols(mut raw: &[u8]) -> Result<Vec<Symbol>> {
    let mut out = Vec::new();
    while let Some((&tag, rest)) = raw.split_first() {
        raw = rest;
        let symbol = match tag {
            1 => Symbol::Int(take_i32(&mut raw)?),
            2 => Symbol::Float(take_f64(&mut raw)?),
            4 => Symbol::Text(take_text(&mut raw)?),
            5 => {
                let i = take_i32(&mut raw)?;
                Symbol::DualInt(i, take_text(&mut raw)?)
            }
            6 => {
                let f = take_f64(&mut raw)?;
                Symbol::DualFloat(f, take_text(&mut raw)?)
            }
            other => bail!("unknown value kind {other}"),
        };
        out.push(symbol);
    }
    Ok(out)
}

fn take<const N: usize>(raw: &mut &[u8]) -> Result<[u8; N]> {
    let (head, rest) = raw
        .split_first_chunk::<N>()
        .ok_or_else(|| anyhow!("a value is cut short"))?;
    *raw = rest;
    Ok(*head)
}

fn take_i32(raw: &mut &[u8]) -> Result<i32> {
    take(raw).map(i32::from_le_bytes)
}

fn take_f64(raw: &mut &[u8]) -> Result<f64> {
    take(raw).map(f64::from_le_bytes)
}

fn take_text(raw: &mut &[u8]) -> Result<String> {
    let len = raw
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| anyhow!("a text has no end"))?;
    let text = String::from_utf8_lossy(&raw[..len]).into_owned();
    *raw = &raw[len + 1..];
    Ok(text)
}

/// `width` bits (at most 64) of a little-endian record, starting at bit `offset`.
fn bits(rec: &[u8], offset: usize, width: usize) -> u64 {
    if width == 0 {
        return 0;
    }
    let mut wide = [0u8; 16];
    let from = offset / 8;
    let n = (rec.len() - from).min(16);
    wide[..n].copy_from_slice(&rec[from..from + n]);
    let v = u128::from_le_bytes(wide) >> (offset % 8);
    (v & ((1u128 << width) - 1)) as u64
}

fn slice(body: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .and_then(|end| body.get(offset..end))
        .ok_or_else(|| anyhow!("the file ends too early"))
}

fn child<'a>(e: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    e.children.iter().find(|c| c.name == name)
}

fn text<'a>(e: &'a XmlElement, name: &str) -> Option<&'a str> {
    child(e, name).map(|c| c.text.trim())
}

fn number(e: &XmlElement, name: &str) -> Result<usize> {
    text(e, name)
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| anyhow!("the QVD header has no usable {name}"))
}

#[cfg(test)]
#[path = "qvd_reader_tests.rs"]
mod tests;
