//! Plain application logs that start with a timestamp and usually a level:
//! what Java (logback, log4j) and Python logging write by default.

use std::sync::LazyLock;

use regex::Regex;

use super::{LogEntry, LogFormat, ParseCtx, normalise_level, parse_timestamp};

static LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\s+",
        r"(?:\[?(?i:(trace|debug|info|notice|warn|warning|error|err|fatal|critical|crit))\]?:?\s+)?",
        r"(.*)$"
    ))
    .expect("valid regex")
});

pub struct Timestamped;

impl LogFormat for Timestamped {
    fn name(&self) -> &'static str {
        "timestamped text"
    }
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        let c = LINE.captures(line)?;
        let (ts, off) = parse_timestamp(&c[1])?;
        Some(LogEntry {
            timestamp: Some(ts),
            utc_offset: off,
            level: c.get(2).map(|l| normalise_level(l.as_str())),
            fields: Vec::new(),
            message: c[3].to_string(),
        })
    }
}
