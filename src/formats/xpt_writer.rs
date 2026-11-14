//! SAS transport file (`.xpt`) writer: one dataset, named after the file.
//!
//! Version 5 is what regulators ask for, so it is written whenever the table
//! fits it: column names of at most 8 characters and text of at most 200
//! bytes. A table that does not fit is written as version 8 (names up to 32,
//! text up to 32767). Either way a column name must be a SAS name (letters,
//! digits and `_`, not starting with a digit); another one is refused by
//! name instead of being changed behind the user's back.
//!
//! Numbers, true/false (1/0) and decimals become SAS numbers, dates, date and
//! times and times get the formats `DATE9.`, `DATETIME19.` and `TIME8.`,
//! everything else is text. An empty cell is a SAS missing value.
//!
//! ponytail: a SAS number is a 56-bit float, so a whole number past 2^56 and a
//! decimal with more than about 16 digits are rounded. Labels are not written.

use std::path::Path;

use anyhow::{Result, anyhow, bail};

use crate::data::decimal::decimal_parts;
use crate::data::test_data::{parse_date, parse_datetime};
use crate::data::times::NANOS_PER_SEC;
use crate::data::{CellValue, DataTable};

const RECORD: usize = 80;
/// 1960-01-01, where SAS starts counting.
const SAS_EPOCH: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(1960, 1, 1).unwrap();

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Number,
    Date,
    DateTime,
    Time,
}

pub(crate) fn write(path: &Path, table: &DataTable) -> Result<()> {
    let bytes = encode(
        table,
        &dataset_name(path),
        chrono::Local::now().naive_local(),
    )?;
    super::write_atomically(path, |tmp| Ok(std::fs::write(tmp, bytes)?))
}

/// The file stem as a SAS name, `DATA` when nothing of it is left.
fn dataset_name(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_uppercase());
    let name: String = stem
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(8)
        .collect();
    if is_sas_name(&name) {
        name
    } else {
        "DATA".to_string()
    }
}

fn is_sas_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The cell as the number SAS stores for a column of `kind`. `None` when it
/// is not one (an empty cell included).
fn number(cell: &CellValue, kind: Kind, decimal: bool) -> Option<f64> {
    match (kind, cell) {
        (Kind::Number, CellValue::Int(i)) => Some(*i as f64),
        (Kind::Number, CellValue::Float(f)) => Some(*f),
        (Kind::Number, CellValue::Bool(b)) => Some(f64::from(u8::from(*b))),
        // An exact decimal too long for a float is held as its digits.
        (Kind::Number, CellValue::String(s)) if decimal => s.trim().parse().ok(),
        (Kind::Date, CellValue::Date(s)) => {
            parse_date(s).map(|d| (d - SAS_EPOCH).num_days() as f64)
        }
        (Kind::DateTime, CellValue::DateTime(s)) => parse_datetime(s).map(|d| {
            let since = d - SAS_EPOCH.and_time(chrono::NaiveTime::MIN);
            since.num_milliseconds() as f64 / 1000.0
        }),
        (Kind::Time, CellValue::Time(ns) | CellValue::Duration(ns)) => {
            Some(*ns as f64 / NANOS_PER_SEC as f64)
        }
        _ => None,
    }
}

/// What a column is written as: the one kind every filled cell fits, else
/// text. An empty column is a number unless it is typed as text.
fn kind(table: &DataTable, col: usize) -> Kind {
    let data_type = table.columns[col].data_type.as_str();
    let decimal = decimal_parts(data_type).is_some();
    let cells = || {
        (0..table.row_count())
            .filter_map(move |r| table.get(r, col))
            .filter(|c| **c != CellValue::Null)
    };
    if cells().next().is_none() && data_type == "Utf8" {
        return Kind::Text;
    }
    [Kind::Number, Kind::Date, Kind::DateTime, Kind::Time]
        .into_iter()
        .find(|&k| cells().all(|c| number(c, k, decimal).is_some()))
        .unwrap_or(Kind::Text)
}

fn encode(table: &DataTable, dataset: &str, now: chrono::NaiveDateTime) -> Result<Vec<u8>> {
    let rows = table.row_count();
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    for (i, name) in names.iter().enumerate() {
        if !is_sas_name(name) || name.len() > 32 {
            bail!(
                "a SAS transport file cannot hold the column name '{name}': use letters, \
                 digits and _ only, at most 32 characters, not starting with a digit"
            );
        }
        if names[..i].iter().any(|n| n.eq_ignore_ascii_case(name)) {
            bail!("a SAS transport file cannot hold two columns named '{name}'");
        }
    }
    if names.len() > 9999 {
        bail!("a SAS transport file holds at most 9999 columns");
    }

    // Each column: its kind, its width in bytes and its text cells.
    let mut columns = Vec::with_capacity(names.len());
    for (col, name) in names.iter().enumerate() {
        let kind = kind(table, col);
        let texts: Vec<String> = if kind == Kind::Text {
            (0..rows)
                .map(|r| table.get(r, col).map(|c| c.to_string()).unwrap_or_default())
                .collect()
        } else {
            Vec::new()
        };
        let width = match kind {
            Kind::Text => texts.iter().map(String::len).max().unwrap_or(0).max(1),
            _ => 8,
        };
        if width > 32_767 {
            bail!(
                "a SAS transport file holds at most 32767 bytes of text in a cell, column \
                 '{name}' has one of {width}"
            );
        }
        columns.push((kind, width, texts));
    }
    let v8 = names.iter().any(|n| n.len() > 8) || columns.iter().any(|c| c.1 > 200);

    let stamp = now.format("%d%b%y:%H:%M:%S").to_string().to_uppercase();
    let head = |v5: &str, v8_kind: &str, digits: &str| {
        let kind = if v8 { v8_kind } else { v5 };
        format!("HEADER RECORD*******{kind:<8}HEADER RECORD!!!!!!!{digits:<30}  ").into_bytes()
    };
    let zeros = "0".repeat(30);
    let mut out = head("LIBRARY", "LIBV8", &zeros);
    out.extend(format!("SAS     SAS     SASLIB  9.4     Octa    {:24}{stamp}", "").bytes());
    out.extend(format!("{stamp:<80}").bytes());
    out.extend(head("MEMBER", "MEMBV8", "000000000000000001600000000140"));
    out.extend(head("DSCRPTR", "DSCPTV8", &zeros));
    out.extend(if v8 {
        format!("SAS     {dataset:<32}SASDATA 9.4     Octa    {stamp}").into_bytes()
    } else {
        format!(
            "SAS     {dataset:<8}SASDATA 9.4     Octa    {:24}{stamp}",
            ""
        )
        .into_bytes()
    });
    out.extend(format!("{stamp:<80}").bytes());
    out.extend(head(
        "NAMESTR",
        "NAMSTV8",
        &format!("{:010}{}", names.len(), "0".repeat(20)),
    ));

    let mut pos = 0u32;
    for (i, (name, (kind, width, _))) in names.iter().zip(&columns).enumerate() {
        let mut e = [0u8; 140];
        let numeric = *kind != Kind::Text;
        e[..2].copy_from_slice(&(if numeric { 1u16 } else { 2 }).to_be_bytes());
        e[4..6].copy_from_slice(&(*width as u16).to_be_bytes());
        e[6..8].copy_from_slice(&(i as u16 + 1).to_be_bytes());
        // Name (its first 8 bytes), label, format, informat: blank padded.
        e[8..64].fill(b' ');
        e[72..80].fill(b' ');
        let short = &name.as_bytes()[..name.len().min(8)];
        e[8..8 + short.len()].copy_from_slice(short);
        let (format, format_width) = match kind {
            Kind::Date => ("DATE", 9u16),
            Kind::DateTime => ("DATETIME", 19),
            Kind::Time => ("TIME", 8),
            Kind::Number | Kind::Text => ("", 0),
        };
        e[56..56 + format.len()].copy_from_slice(format.as_bytes());
        e[64..66].copy_from_slice(&format_width.to_be_bytes());
        // Numbers to the right, text to the left.
        e[68..70].copy_from_slice(&u16::from(numeric).to_be_bytes());
        e[84..88].copy_from_slice(&pos.to_be_bytes());
        if v8 {
            e[88..120].fill(b' ');
            e[88..88 + name.len()].copy_from_slice(name.as_bytes());
        }
        pos += *width as u32;
        out.extend(e);
    }
    pad(&mut out);

    out.extend(if v8 {
        format!(
            "HEADER RECORD*******OBSV8   HEADER RECORD!!!!!!!{rows:>15}{:17}",
            ""
        )
        .into_bytes()
    } else {
        head("OBS", "", &zeros)
    });
    for r in 0..rows {
        for (col, (kind, width, texts)) in columns.iter().enumerate() {
            if *kind == Kind::Text {
                out.extend(format!("{:<width$}", texts[r]).bytes());
                // `{:<width$}` pads by characters; make up for the bytes.
                let short = texts[r].len() - texts[r].chars().count();
                out.truncate(out.len() - short);
                continue;
            }
            let decimal = decimal_parts(&table.columns[col].data_type).is_some();
            let value = table.get(r, col).and_then(|c| number(c, *kind, decimal));
            out.extend(match value {
                // SAS's missing value.
                None => [b'.', 0, 0, 0, 0, 0, 0, 0],
                Some(v) if v.is_nan() => [b'.', 0, 0, 0, 0, 0, 0, 0],
                Some(v) => ibm_float(v).ok_or_else(|| {
                    anyhow!(
                        "row {}, column '{}': {v} is too large for a SAS transport file",
                        r + 1,
                        names[col]
                    )
                })?,
            });
        }
    }
    pad(&mut out);
    Ok(out)
}

/// Fill the last 80-byte record with blanks.
fn pad(out: &mut Vec<u8>) {
    out.resize(out.len().div_ceil(RECORD) * RECORD, b' ');
}

/// A double as an 8-byte IBM mainframe float: a sign, a power of 16 offset by
/// 64 and 14 hex digits after the point. `None` when it is too large
/// (beyond about 7e75, infinity included); too small becomes zero.
fn ibm_float(v: f64) -> Option<[u8; 8]> {
    if v == 0.0 {
        return Some([0; 8]);
    }
    if !v.is_finite() {
        return None;
    }
    let bits = v.to_bits();
    let biased = ((bits >> 52) & 0x7FF) as i32;
    // v = mantissa * 2^power, the mantissa a whole number below 2^53.
    let (mantissa, power) = match biased {
        0 => (bits & 0x000F_FFFF_FFFF_FFFF, -1074),
        _ => ((bits & 0x000F_FFFF_FFFF_FFFF) | (1 << 52), biased - 1075),
    };
    // Shift up to 3 bits so the power of two is a power of 16.
    let shift = power.rem_euclid(4);
    let fraction = mantissa << shift;
    let exponent = (power - shift) / 4 + 64 + 14;
    if exponent > 127 {
        return None;
    }
    if exponent < 0 {
        return Some([0; 8]);
    }
    let mut out = fraction.to_be_bytes();
    out[0] = exponent as u8 | if v < 0.0 { 0x80 } else { 0 };
    Some(out)
}

#[cfg(test)]
#[path = "xpt_writer_tests.rs"]
mod tests;
