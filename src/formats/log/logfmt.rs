//! logfmt: `time=... level=info msg="..." key=value`.

use std::sync::LazyLock;

use regex::Regex;

use super::{LogEntry, LogFormat, ParseCtx, entry_from_pairs, typed};
use crate::data::CellValue;

static PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z_@][\w.\-]*)=("(?:[^"\\]|\\.)*"|\S*)"#).expect("valid regex")
});

pub struct Logfmt;

fn unquote(v: &str) -> CellValue {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        CellValue::String(
            v[1..v.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        )
    } else {
        typed(v)
    }
}

impl LogFormat for Logfmt {
    fn name(&self) -> &'static str {
        "logfmt"
    }
    /// Only a line made entirely of pairs, at least two of them, counts:
    /// prose with one `a=b` in it is not logfmt.
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        let mut pos = 0;
        let mut pairs = Vec::new();
        for c in PAIR.captures_iter(line) {
            let whole = c.get(0)?;
            if !line[pos..whole.start()].trim().is_empty() {
                return None;
            }
            pos = whole.end();
            pairs.push((c[1].to_string(), unquote(&c[2])));
        }
        (line[pos..].trim().is_empty() && pairs.len() >= 2).then(|| entry_from_pairs(pairs))
    }
}
