//! Syslog, classic (RFC 3164, no year) and structured (RFC 5424).

use std::sync::LazyLock;

use chrono::{DateTime, NaiveDateTime};
use regex::Regex;

use super::{LogEntry, LogFormat, ParseCtx, severity_level, split_timestamp, typed};
use crate::data::CellValue;

static RFC3164: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:<(\d{1,3})>)?([A-Z][a-z]{2}) ([ \d]\d) (\d{2}:\d{2}:\d{2}) (\S+) ([^:\[\s]+)(?:\[(\d+)\])?: ?(.*)$")
        .expect("valid regex")
});
static RFC5424: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^<(\d{1,3})>1 (\S+) (\S+) (\S+) (\S+) (\S+) (-|(?:\[(?:[^\]\\]|\\.)*\])+)(?: (.*))?$",
    )
    .expect("valid regex")
});

pub struct Rfc3164;
pub struct Rfc5424;

fn nil(s: &str) -> CellValue {
    if s == "-" { CellValue::Null } else { typed(s) }
}

impl LogFormat for Rfc3164 {
    fn name(&self) -> &'static str {
        "syslog (RFC 3164)"
    }
    fn parse_line(&self, line: &str, ctx: &ParseCtx) -> Option<LogEntry> {
        let c = RFC3164.captures(line)?;
        let stamp = format!("{} {} {} {}", ctx.year, &c[2], c[3].trim(), &c[4]);
        let ts = NaiveDateTime::parse_from_str(&stamp, "%Y %b %d %H:%M:%S").ok()?;
        Some(LogEntry {
            timestamp: Some(ts.format("%Y-%m-%d %H:%M:%S").to_string()),
            utc_offset: None,
            level: c
                .get(1)
                .and_then(|p| p.as_str().parse().ok())
                .map(severity_level),
            fields: vec![
                ("host".into(), CellValue::String(c[5].to_string())),
                ("app".into(), CellValue::String(c[6].to_string())),
                (
                    "pid".into(),
                    c.get(7).map_or(CellValue::Null, |p| typed(p.as_str())),
                ),
            ],
            message: c[8].to_string(),
        })
    }
}

impl LogFormat for Rfc5424 {
    fn name(&self) -> &'static str {
        "syslog (RFC 5424)"
    }
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        let c = RFC5424.captures(line)?;
        let (timestamp, utc_offset) = match &c[2] {
            "-" => (None, None),
            s => {
                let (a, b) = split_timestamp(DateTime::parse_from_rfc3339(s).ok()?);
                (Some(a), Some(b))
            }
        };
        let mut fields = vec![
            ("host".into(), nil(&c[3])),
            ("app".into(), nil(&c[4])),
            ("pid".into(), nil(&c[5])),
            ("msgid".into(), nil(&c[6])),
        ];
        if &c[7] != "-" {
            fields.push((
                "structured_data".into(),
                CellValue::String(c[7].to_string()),
            ));
        }
        Some(LogEntry {
            timestamp,
            utc_offset,
            level: c[1].parse().ok().map(severity_level),
            fields,
            message: c
                .get(8)
                .map_or("", |m| m.as_str())
                .trim_start_matches('\u{feff}')
                .to_string(),
        })
    }
}
