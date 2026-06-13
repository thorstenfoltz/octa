//! Extract a regex match from each cell into a new column.

use regex::Regex;

use super::cell_text;
use crate::data::{CellValue, DataTable};

/// For each cell in `col`, extract the first capture group of `re` (or the
/// whole match when the pattern has no capture group). Cells that don't match
/// become `Null`. The caller compiles `re` so it can surface a friendly error
/// for a bad pattern.
pub fn extract_pattern(table: &DataTable, col: usize, re: &Regex) -> Vec<CellValue> {
    let n = table.row_count();
    (0..n)
        .map(|r| {
            let text = cell_text(table, r, col);
            match re.captures(&text) {
                Some(caps) => caps
                    .get(1)
                    .or_else(|| caps.get(0))
                    .map(|m| CellValue::String(m.as_str().to_string()))
                    .unwrap_or(CellValue::Null),
                None => CellValue::Null,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{ColumnInfo, DataTable};

    fn table(values: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns.push(ColumnInfo {
            name: "src".to_string(),
            data_type: "Utf8".to_string(),
        });
        t.rows = values
            .iter()
            .map(|v| vec![CellValue::String(v.to_string())])
            .collect();
        t
    }

    #[test]
    fn extracts_capture_group() {
        let t = table(&["order #1234 ok", "no number here"]);
        let re = Regex::new(r"#(\d+)").unwrap();
        let out = extract_pattern(&t, 0, &re);
        assert_eq!(out[0], CellValue::String("1234".into()));
        assert_eq!(out[1], CellValue::Null);
    }

    #[test]
    fn whole_match_when_no_group() {
        let t = table(&["abc123def"]);
        let re = Regex::new(r"\d+").unwrap();
        let out = extract_pattern(&t, 0, &re);
        assert_eq!(out[0], CellValue::String("123".into()));
    }
}
