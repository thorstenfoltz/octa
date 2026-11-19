//! SAS transport file (`.xpt`, XPORT) reader, versions 5 and 8. Writing is
//! in `xpt_writer.rs`.
//!
//! The file is a run of 80-byte records: a library header, then per dataset
//! ("member") a description, one 140-byte entry per variable, and the rows
//! at a fixed width. Numbers are IBM mainframe floats of 2 to 8 bytes; a
//! missing number is a `.`, `_` or letter followed by zero bytes.
//!
//! A variable whose SAS format is a date, datetime or time format becomes
//! `Date`, `DateTime` or a time (SAS counts from 1960). Other numbers are
//! `Int64` when every value is whole, else `Float64`. A file holding several
//! datasets lists them as tables.
//!
//! ponytail: version 5 does not store a row count, so trailing rows made
//! only of blanks are taken as padding (what SAS and ReadStat do too); a
//! real last row of blank text and nothing else is dropped with them.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::data::times::NANOS_PER_SEC;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::sas_reader::{format_unix_secs, narrow_time_columns};
use crate::formats::{FormatReader, TableInfo};

pub struct XptReader;

impl FormatReader for XptReader {
    fn name(&self) -> &str {
        "SAS transport"
    }

    fn extensions(&self) -> &[&str] {
        &["xpt", "xport"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let bytes = read(path)?;
        let members = members(&bytes).with_context(|| parsing(path))?;
        let first = members
            .first()
            .ok_or_else(|| anyhow!("the transport file holds no dataset"))?;
        Ok(table(&bytes, first, path))
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let bytes = read(path)?;
        let members = members(&bytes).with_context(|| parsing(path))?;
        if members.len() < 2 {
            return Ok(None);
        }
        Ok(Some(
            members
                .iter()
                .map(|m| TableInfo {
                    name: m.name.clone(),
                    schema: None,
                    columns: Vec::new(),
                    row_count: Some(m.rows),
                })
                .collect(),
        ))
    }

    fn read_table(&self, path: &Path, name: &str) -> Result<DataTable> {
        let bytes = read(path)?;
        let members = members(&bytes).with_context(|| parsing(path))?;
        let member = members
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| anyhow!("no dataset '{name}' in {}", path.display()))?;
        Ok(table(&bytes, member, path))
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        // The writer writes one dataset; over a file of several it would
        // delete the others.
        if let Ok(bytes) = std::fs::read(path)
            && members(&bytes).is_ok_and(|m| m.len() > 1)
        {
            bail!(
                "{} holds several datasets and saving would keep only this one: save \
                 under another name",
                path.display()
            );
        }
        super::xpt_writer::write(path, table)
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading SAS transport file {}", path.display()))
}

fn parsing(path: &Path) -> String {
    format!("parsing SAS transport file {}", path.display())
}

const RECORD: usize = 80;
const HEADER: &[u8] = b"HEADER RECORD*******";
/// Days from 1960-01-01 (SAS) to 1970-01-01.
const SAS_EPOCH_DAYS: f64 = 3_653.0;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Number,
    Date,
    DateTime,
    Time,
}

struct Variable {
    name: String,
    kind: Kind,
    pos: usize,
    len: usize,
}

struct Member {
    name: String,
    variables: Vec<Variable>,
    /// Where the rows start, their width and how many there are.
    data: usize,
    row_len: usize,
    rows: usize,
}

/// The header record at `at`, when there is one: its 8-character kind
/// (`LIBRARY`, `MEMBER`, `NAMESTR`, `OBS`, or the `V8` spellings) and the
/// 30 digits after it.
fn header(bytes: &[u8], at: usize) -> Option<(&str, &str)> {
    let rec = bytes.get(at..at + RECORD)?;
    if !rec.starts_with(HEADER) {
        return None;
    }
    let kind = std::str::from_utf8(&rec[20..28]).ok()?.trim_end();
    let digits = std::str::from_utf8(&rec[48..78]).ok()?;
    Some((kind, digits))
}

fn members(bytes: &[u8]) -> Result<Vec<Member>> {
    if !matches!(header(bytes, 0), Some(("LIBRARY" | "LIBV8", _))) {
        bail!("not a SAS transport file");
    }
    // Every section is padded to 80 bytes, so members start on a boundary.
    let starts: Vec<usize> = (0..bytes.len() / RECORD)
        .map(|r| r * RECORD)
        .filter(|&at| matches!(header(bytes, at), Some(("MEMBER" | "MEMBV8", _))))
        .collect();
    let mut out = Vec::new();
    for (i, &at) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(bytes.len());
        out.push(member(bytes, at, end)?);
    }
    Ok(out)
}

fn member(bytes: &[u8], at: usize, end: usize) -> Result<Member> {
    let cut = || anyhow!("the transport file is cut short");
    let (kind, digits) = header(bytes, at).ok_or_else(cut)?;
    let v8 = kind == "MEMBV8";
    // 140 everywhere except files written on VAX (136).
    // `get`: the digits are text, and a damaged byte there can be part of
    // a wider character.
    let entry: usize = digits
        .get(26..30)
        .and_then(|d| d.trim().parse().ok())
        .unwrap_or(140);
    if !(88..=140).contains(&entry) {
        bail!("unexpected variable entry size {entry}");
    }

    let desc = bytes
        .get(at + 2 * RECORD..at + 3 * RECORD)
        .ok_or_else(cut)?;
    let name = text(&desc[8..if v8 { 40 } else { 16 }]).unwrap_or_default();

    let names_at = at + 4 * RECORD;
    let (_, digits) = header(bytes, names_at).ok_or_else(cut)?;
    let count: usize = digits
        .get(..10)
        .and_then(|d| d.trim().parse().ok())
        .ok_or_else(|| anyhow!("the transport file does not say how many variables it has"))?;
    let first = names_at + RECORD;
    let mut variables = Vec::with_capacity(count);
    for i in 0..count {
        let e = bytes
            .get(first + i * entry..first + (i + 1) * entry)
            .ok_or_else(cut)?;
        variables.push(variable(e, i, v8));
    }

    // Version 8 may put long labels between the variables and the rows.
    let mut obs = first + (count * entry).div_ceil(RECORD) * RECORD;
    while !matches!(header(bytes, obs), Some(("OBS" | "OBSV8", _))) {
        obs += RECORD;
        if obs >= end {
            return Err(cut());
        }
    }
    let data = obs + RECORD;
    let row_len = variables.iter().map(|v| v.pos + v.len).max().unwrap_or(0);
    let mut rows = end.saturating_sub(data).checked_div(row_len).unwrap_or(0);
    while rows > 0
        && bytes[data + (rows - 1) * row_len..data + rows * row_len]
            .iter()
            .all(|&b| b == b' ')
    {
        rows -= 1;
    }
    Ok(Member {
        name,
        variables,
        data,
        row_len,
        rows,
    })
}

fn variable(e: &[u8], index: usize, v8: bool) -> Variable {
    let short = |at: usize| usize::from(u16::from_be_bytes([e[at], e[at + 1]]));
    let numeric = short(0) == 1;
    let long_name = if v8 && e.len() >= 120 {
        text(&e[88..120])
    } else {
        None
    };
    let name = long_name
        .or_else(|| text(&e[8..16]))
        .unwrap_or_else(|| format!("col_{}", index + 1));
    let format = text(&e[56..64]).unwrap_or_default().to_ascii_uppercase();
    Variable {
        name,
        kind: if numeric {
            format_kind(&format)
        } else {
            Kind::Text
        },
        pos: u32::from_be_bytes([e[84], e[85], e[86], e[87]]) as usize,
        len: short(4),
    }
}

/// What a SAS format name says about the number under it.
fn format_kind(format: &str) -> Kind {
    const DATETIME: &[&str] = &["DATETIME", "E8601DT", "B8601DT", "DATEAMPM", "NLDATM", "DT"];
    const TIME: &[&str] = &[
        "TIME", "HHMM", "E8601TM", "B8601TM", "TOD", "HOUR", "MMSS", "NLTIM",
    ];
    const DATE: &[&str] = &[
        "DATE", "DDMMYY", "MMDDYY", "YYMMDD", "E8601DA", "B8601DA", "WEEKDAT", "WORDDAT", "MONYY",
        "YYMON", "YYQ", "YYMM", "MMYY", "JULIAN", "NLDATE", "EURDF",
    ];
    let has = |list: &[&str]| list.iter().any(|p| format.starts_with(p));
    if has(DATETIME) {
        Kind::DateTime
    } else if has(TIME) {
        Kind::Time
    } else if has(DATE) {
        Kind::Date
    } else {
        Kind::Number
    }
}

fn table(bytes: &[u8], m: &Member, path: &Path) -> DataTable {
    let mut rows = Vec::with_capacity(m.rows);
    for r in 0..m.rows {
        let row = &bytes[m.data + r * m.row_len..m.data + (r + 1) * m.row_len];
        rows.push(
            m.variables
                .iter()
                .map(|v| cell(&row[v.pos..v.pos + v.len], v.kind))
                .collect::<Vec<CellValue>>(),
        );
    }
    let columns = m
        .variables
        .iter()
        .enumerate()
        .map(|(col, v)| ColumnInfo {
            name: v.name.clone(),
            data_type: match v.kind {
                Kind::Text => "Utf8",
                Kind::Date => "Date",
                Kind::DateTime => "DateTime",
                Kind::Time => "Duration(s)",
                Kind::Number => {
                    if whole_numbers(&mut rows, col) {
                        "Int64"
                    } else {
                        "Float64"
                    }
                }
            }
            .to_string(),
        })
        .collect();
    let mut table = DataTable {
        columns,
        rows,
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("SAS transport".to_string()),
        ..DataTable::empty()
    };
    narrow_time_columns(&mut table);
    table
}

/// SAS has one number type. A column whose every value is whole (and exact
/// in a float) is turned into integers; returns whether it was.
fn whole_numbers(rows: &mut [Vec<CellValue>], col: usize) -> bool {
    let whole = |c: &CellValue| match c {
        CellValue::Float(f) => f.fract() == 0.0 && f.abs() < 9e15,
        _ => true,
    };
    if !rows.iter().all(|r| whole(&r[col])) || rows.iter().all(|r| r[col] == CellValue::Null) {
        return false;
    }
    for r in rows {
        if let CellValue::Float(f) = r[col] {
            r[col] = CellValue::Int(f as i64);
        }
    }
    true
}

fn cell(raw: &[u8], kind: Kind) -> CellValue {
    if kind == Kind::Text {
        return text(raw).map_or(CellValue::Null, CellValue::String);
    }
    let Some(v) = ibm_float(raw) else {
        return CellValue::Null;
    };
    match kind {
        Kind::Date => CellValue::Date(format_unix_secs(
            ((v.floor() - SAS_EPOCH_DAYS) as i64).saturating_mul(86_400),
            "%Y-%m-%d",
        )),
        Kind::DateTime => CellValue::DateTime(format_unix_secs(
            (v - SAS_EPOCH_DAYS * 86_400.0).round() as i64,
            "%Y-%m-%d %H:%M:%S",
        )),
        Kind::Time => CellValue::Duration((v.round() as i64).saturating_mul(NANOS_PER_SEC)),
        Kind::Number | Kind::Text => CellValue::Float(v),
    }
}

/// An IBM mainframe float of up to 8 bytes (shorter ones are cut on the
/// right). `None` for a SAS missing value.
fn ibm_float(raw: &[u8]) -> Option<f64> {
    let (&first, rest) = raw.split_first()?;
    if rest.iter().all(|&b| b == 0) && matches!(first, b'.' | b'_' | b'A'..=b'Z') {
        return None;
    }
    let mut wide = [0u8; 8];
    let n = raw.len().min(8);
    wide[..n].copy_from_slice(&raw[..n]);
    let fraction = u64::from_be_bytes(wide) & 0x00FF_FFFF_FFFF_FFFF;
    // The fraction is 14 hex digits after the point, the exponent a power
    // of 16 offset by 64.
    let exponent = i32::from(first & 0x7F) - 64 - 14;
    let v = fraction as f64 * 16f64.powi(exponent);
    Some(if first & 0x80 != 0 { -v } else { v })
}

/// Text without its padding; `None` when blank. Old files are Latin-1 or
/// Windows-1252, newer ones UTF-8.
fn text(raw: &[u8]) -> Option<String> {
    let s = match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(raw).0.into_owned(),
    };
    let s = s.trim_end_matches([' ', '\0']);
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
#[path = "xpt_reader_tests.rs"]
mod tests;
