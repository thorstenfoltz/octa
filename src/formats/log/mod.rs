//! Log files as tables: one row per log entry, with a real timestamp, a
//! normalised level and the format's own fields as columns.
//!
//! One file per log format, each a [`LogFormat`]; `reader.rs` picks the one
//! that fits a file's first lines. Stack traces stay with their entry (see
//! [`is_continuation`]); every other line that fits no entry is kept in a
//! `raw` column rather than dropped.

use chrono::{DateTime, FixedOffset, NaiveDateTime};

use crate::data::CellValue;

mod apache;
mod json_lines;
mod logfmt;
mod reader;
mod syslog;
mod timestamped;

pub use reader::{LOG_READER, LOG_READER_FORCED, LogReader, unmatched_note};

/// What a line alone cannot say. Classic syslog has no year.
pub struct ParseCtx {
    pub year: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LogEntry {
    /// Wall-clock time, `YYYY-MM-DD HH:MM:SS[.fff]`.
    pub timestamp: Option<String>,
    /// `+02:00`; its own column because no DateTime cell carries an offset.
    pub utc_offset: Option<String>,
    /// Normalised with [`normalise_level`].
    pub level: Option<String>,
    /// The format's own columns, in the format's order.
    pub fields: Vec<(String, CellValue)>,
    pub message: String,
}

pub trait LogFormat: Send + Sync {
    fn name(&self) -> &'static str;
    fn parse_line(&self, line: &str, ctx: &ParseCtx) -> Option<LogEntry>;
}

/// Every format, in detection order: on a tie the earlier one wins.
pub fn formats() -> Vec<Box<dyn LogFormat>> {
    vec![
        Box::new(apache::Combined),
        Box::new(apache::Common),
        Box::new(syslog::Rfc5424),
        Box::new(syslog::Rfc3164),
        Box::new(json_lines::JsonLines),
        Box::new(logfmt::Logfmt),
        Box::new(timestamped::Timestamped),
    ]
}

const TS_FMT: &str = "%Y-%m-%d %H:%M:%S%.f";

pub(crate) fn split_timestamp(dt: DateTime<FixedOffset>) -> (String, String) {
    (
        dt.naive_local().format(TS_FMT).to_string(),
        dt.format("%:z").to_string(),
    )
}

/// `(wall clock, offset)` from the timestamp spellings logs use: RFC 3339,
/// `2026-09-25 10:00:01,123`, with or without an offset or `Z`.
pub fn parse_timestamp(raw: &str) -> Option<(String, Option<String>)> {
    let s = raw.trim().replace(',', ".");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        let (a, b) = split_timestamp(dt);
        return Some((a, Some(b)));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f%z", "%Y-%m-%dT%H:%M:%S%.f%z"] {
        if let Ok(dt) = DateTime::parse_from_str(&s, fmt) {
            let (a, b) = split_timestamp(dt);
            return Some((a, Some(b)));
        }
    }
    let zulu = s.ends_with('Z');
    let bare = s.trim_end_matches('Z');
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(bare, fmt) {
            return Some((
                n.format(TS_FMT).to_string(),
                zulu.then(|| "+00:00".to_string()),
            ));
        }
    }
    None
}

/// `WARNING`, `warn` and `W` all become `WARN`; unknown levels are just
/// upper-cased.
pub fn normalise_level(raw: &str) -> String {
    let up = raw.trim().to_ascii_uppercase();
    match up.as_str() {
        "WARNING" | "WARN" | "W" => "WARN",
        "ERROR" | "ERR" | "E" => "ERROR",
        "INFO" | "INFORMATION" | "I" | "NOTICE" => "INFO",
        "DEBUG" | "DBG" | "D" => "DEBUG",
        "TRACE" | "T" => "TRACE",
        "FATAL" | "CRITICAL" | "CRIT" | "F" | "PANIC" | "EMERG" | "EMERGENCY" | "ALERT" => "FATAL",
        _ => return up,
    }
    .to_string()
}

/// A line that continues the entry above it: indented, or the start of a
/// Java / Python stack trace.
pub fn is_continuation(line: &str) -> bool {
    line.starts_with([' ', '\t'])
        || ["at ", "Caused by:", "Traceback", "..."]
            .iter()
            .any(|p| line.starts_with(p))
}

/// Syslog severity (PRI mod 8) as a level.
pub(crate) fn severity_level(pri: u32) -> String {
    match pri % 8 {
        0..=2 => "FATAL",
        3 => "ERROR",
        4 => "WARN",
        5 | 6 => "INFO",
        _ => "DEBUG",
    }
    .to_string()
}

/// A bare value as a typed cell: whole number, number, true/false, text.
pub(crate) fn typed(s: &str) -> CellValue {
    if s.is_empty() {
        CellValue::Null
    } else if let Ok(i) = s.parse::<i64>() {
        CellValue::Int(i)
    } else if let Ok(f) = s.parse::<f64>() {
        CellValue::Float(f)
    } else if s == "true" || s == "false" {
        CellValue::Bool(s == "true")
    } else {
        CellValue::String(s.to_string())
    }
}

/// Epoch seconds or milliseconds, the two a JSON log's `ts` usually holds.
fn epoch(v: f64) -> Option<(String, Option<String>)> {
    let secs = if (1e9..1e10).contains(&v) {
        v
    } else if (1e12..1e13).contains(&v) {
        v / 1000.0
    } else {
        return None;
    };
    let dt = DateTime::from_timestamp(secs.trunc() as i64, ((secs.fract()) * 1e9) as u32)?;
    Some((
        dt.naive_utc().format(TS_FMT).to_string(),
        Some("+00:00".into()),
    ))
}

/// Key/value pairs (logfmt, JSON) to an entry: the usual names for time,
/// level and message become those columns, everything else a field.
pub(crate) fn entry_from_pairs(pairs: Vec<(String, CellValue)>) -> LogEntry {
    let mut e = LogEntry::default();
    for (k, v) in pairs {
        match k.to_ascii_lowercase().as_str() {
            "time" | "ts" | "timestamp" | "@timestamp" | "t" if e.timestamp.is_none() => {
                let parsed = match &v {
                    CellValue::Int(i) => epoch(*i as f64),
                    CellValue::Float(f) => epoch(*f),
                    other => parse_timestamp(&other.to_string()),
                };
                match parsed {
                    Some((ts, off)) => {
                        e.timestamp = Some(ts);
                        e.utc_offset = off;
                    }
                    None => e.fields.push((k, v)),
                }
            }
            "level" | "lvl" | "severity" | "loglevel" if e.level.is_none() => {
                e.level = Some(normalise_level(&v.to_string()));
            }
            "msg" | "message" if e.message.is_empty() => e.message = v.to_string(),
            _ => e.fields.push((k, v)),
        }
    }
    e
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
