use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

/// Reader for plain text, config, and source-code files. Each line becomes a
/// row with a single "Line" column; the raw editor's syntect pass highlights
/// the recognised languages.
///
/// The source-code / markup extensions mirror `ui::syntax::HIGHLIGHT_WHITELIST`
/// so that every file Octa can syntax-highlight is also a *supported* file
/// (listed in the open dialog's "All Supported" filter) rather than something
/// that only opens via an unadvertised fallback. The
/// `highlight_whitelist_is_supported` test keeps the two lists from drifting.
pub struct TextReader;

impl FormatReader for TextReader {
    fn name(&self) -> &str {
        "Text"
    }

    fn extensions(&self) -> &[&str] {
        &[
            // Plain text / config
            "txt",
            "log",
            "cfg",
            "ini",
            "conf",
            "bat",
            "ps1",
            "env",
            "gitignore",
            "dockerignore",
            "editorconfig",
            "properties",
            // Shell family
            "sh",
            "bash",
            "zsh",
            "fish",
            // Python
            "py",
            "pyw",
            "pyi",
            // Rust
            "rs",
            // C / C++ / headers
            "c",
            "cpp",
            "cc",
            "cxx",
            "h",
            "hpp",
            "hxx",
            // Go
            "go",
            // Web / JS / TS
            "js",
            "jsx",
            "mjs",
            "cjs",
            "ts",
            "tsx",
            "html",
            "htm",
            "css",
            "scss",
            "sass",
            // JVM family
            "java",
            "kt",
            "kts",
            "scala",
            "groovy",
            // Scripting
            "rb",
            "php",
            "pl",
            "lua",
            "swift",
            // Data-science neighbours
            "r",
            "jl",
            // Misc
            "tex",
            "dart",
            "ex",
            "exs",
            // Terraform / HCL - opened as text; the raw editor's syntect
            // pass adds proper highlighting via the bundled .sublime-syntax.
            "tf",
            "tfvars",
            "hcl",
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
