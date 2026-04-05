use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

/// Reader for Markdown files (.md, .markdown).
/// Each line becomes a row with a single "Line" column (for table view).
/// The primary view is the rendered Markdown view.
pub struct MarkdownReader;

impl FormatReader for MarkdownReader {
    fn name(&self) -> &str {
        "Markdown"
    }

    fn extensions(&self) -> &[&str] {
        &["md", "markdown", "mdown", "mkd"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let mut lines = Vec::with_capacity(table.row_count());
        for row in 0..table.row_count() {
            match table.get(row, 0) {
                Some(CellValue::String(s)) => lines.push(s.clone()),
                Some(v) => lines.push(v.to_string()),
                None => lines.push(String::new()),
            }
        }
        std::fs::write(path, lines.join("\n"))?;
        Ok(())
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();

        let columns = vec![ColumnInfo {
            name: "Line".to_string(),
            data_type: "Utf8".to_string(),
        }];

        let rows: Vec<Vec<CellValue>> = lines
            .iter()
            .map(|line| vec![CellValue::String(line.to_string())])
            .collect();

        let mut table = DataTable::empty();
        table.columns = columns;
        table.rows = rows;
        table.source_path = Some(path.to_string_lossy().to_string());
        table.format_name = Some("Markdown".to_string());
        Ok(table)
    }
}
