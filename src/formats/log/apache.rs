//! Apache / nginx access logs, common and combined.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{LogEntry, LogFormat, ParseCtx, split_timestamp};
use crate::data::CellValue;

const HEAD: &str = r#"^(\S+) (\S+) (\S+) \[([^\]]+)\] "((?:[^"\\]|\\.)*)" (\d{3}) (\S+)"#;
const QUOTED: &str = r#""((?:[^"\\]|\\.)*)""#;

static COMMON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("{HEAD}$")).expect("valid regex"));
// nginx often appends more quoted fields ($http_x_forwarded_for); allow them.
static COMBINED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"{HEAD} {QUOTED} {QUOTED}(?:\s.*)?$")).expect("valid regex")
});

pub struct Common;
pub struct Combined;

impl LogFormat for Common {
    fn name(&self) -> &'static str {
        "Apache/nginx common"
    }
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        entry(&COMMON.captures(line)?, false)
    }
}

impl LogFormat for Combined {
    fn name(&self) -> &'static str {
        "Apache/nginx combined"
    }
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        entry(&COMBINED.captures(line)?, true)
    }
}

fn dash(s: &str) -> CellValue {
    if s == "-" {
        CellValue::Null
    } else {
        CellValue::String(s.to_string())
    }
}

fn entry(c: &Captures, combined: bool) -> Option<LogEntry> {
    let dt = chrono::DateTime::parse_from_str(&c[4], "%d/%b/%Y:%H:%M:%S %z").ok()?;
    let (ts, off) = split_timestamp(dt);
    let request = &c[5];
    let parts: Vec<&str> = request.splitn(3, ' ').collect();
    let (method, path, protocol) = match parts.as_slice() {
        [m, p, v] => (dash(m), dash(p), dash(v)),
        _ => (CellValue::Null, dash(request), CellValue::Null),
    };
    let int = |s: &str| {
        s.parse::<i64>()
            .map(CellValue::Int)
            .unwrap_or(CellValue::Null)
    };
    let mut fields = vec![
        ("client".to_string(), dash(&c[1])),
        ("user".to_string(), dash(&c[3])),
        ("method".to_string(), method),
        ("path".to_string(), path),
        ("protocol".to_string(), protocol),
        ("status".to_string(), int(&c[6])),
        ("bytes".to_string(), int(&c[7])),
    ];
    if combined {
        fields.push(("referer".to_string(), dash(&c[8])));
        fields.push(("user_agent".to_string(), dash(&c[9])));
    }
    Some(LogEntry {
        timestamp: Some(ts),
        utc_offset: Some(off),
        level: None,
        fields,
        message: String::new(),
    })
}
