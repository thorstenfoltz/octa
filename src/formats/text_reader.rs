use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

/// Reader for plain text files (.txt, .log, .cfg, .ini, .conf, .sh, .bat, .ps1, etc.)
/// Each line becomes a row with a single "Line" column.
pub struct TextReader;

impl FormatReader for TextReader {
    fn name(&self) -> &str {
        "Text"
    }

    fn extensions(&self) -> &[&str] {
        &[
            "txt",
            "log",
            "cfg",
            "ini",
            "conf",
            "sh",
            "bat",
            "ps1",
            "env",
            "gitignore",
            "dockerignore",
            "editorconfig",
            "properties",
        ]
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
        read_text_file(path)
    }
}

fn read_text_file(path: &Path) -> Result<DataTable> {
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
    table.format_name = Some("Text".to_string());
    Ok(table)
}
