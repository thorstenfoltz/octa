//! One JSON object per line (structured logging from most frameworks).

use super::{LogEntry, LogFormat, ParseCtx, entry_from_pairs};
use crate::data::CellValue;

pub struct JsonLines;

fn cell(v: serde_json::Value) -> CellValue {
    use serde_json::Value as J;
    match v {
        J::Null => CellValue::Null,
        J::Bool(b) => CellValue::Bool(b),
        J::Number(n) => n
            .as_i64()
            .map(CellValue::Int)
            .unwrap_or_else(|| CellValue::Float(n.as_f64().unwrap_or(f64::NAN))),
        J::String(s) => CellValue::String(s),
        other => CellValue::Nested(other.to_string()),
    }
}

impl LogFormat for JsonLines {
    fn name(&self) -> &'static str {
        "JSON lines"
    }
    fn parse_line(&self, line: &str, _: &ParseCtx) -> Option<LogEntry> {
        let t = line.trim();
        if !t.starts_with('{') {
            return None;
        }
        let serde_json::Value::Object(map) = serde_json::from_str(t).ok()? else {
            return None;
        };
        Some(entry_from_pairs(
            map.into_iter().map(|(k, v)| (k, cell(v))).collect(),
        ))
    }
}
