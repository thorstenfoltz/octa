//! EPUB reader. Pure-Rust parsing via `rbook` (Apache-2.0), HTML→Markdown
//! via `htmd` (Apache-2.0).
//!
//! ## Scope
//!
//! Read-only. The table representation is intentionally simple: one row per
//! paragraph, with `chapter`, `paragraph`, and `text` columns. The richer
//! reading view lives in `src/view_modes/epub_reader.rs` and consumes the
//! per-chapter Markdown + embedded image bytes captured by
//! [`read_with_extras`].
//!
//! The format reader exposes only the table — the side state (rendered
//! chapter Markdown + image bytes) is fetched separately by
//! `app::file_io::apply_loaded_table` and stashed on the `TabState`. This
//! mirrors how `YamlReader` returns a flat table and the YAML tree value
//! is parsed once in `apply_loaded_table`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rbook::Epub;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;

pub struct EpubReader;

/// Side-channel state extracted from an EPUB file beyond what fits in the
/// flat `DataTable` shape. Loaded once when the file opens; consumed by
/// the EPUB reading view.
#[derive(Debug, Default, Clone)]
pub struct EpubExtras {
    /// Markdown-rendered body for each chapter, in spine (reading) order.
    /// Index matches the `chapter` column of the table (1-indexed → off by
    /// one, so `chapters_md[chapter - 1]`).
    pub chapters_md: Vec<String>,
    /// Best-effort per-chapter title, derived from the manifest href's
    /// filename. Falls back to `"Chapter N"` when the href is empty.
    pub chapter_titles: Vec<String>,
    /// Decoded image bytes keyed by the manifest href (e.g. `images/cover.png`).
    /// The Markdown payloads reference these via `![alt](href)` pulled from
    /// the original XHTML; the reading view uploads them to egui textures
    /// on demand and resolves the references at paint time.
    pub image_bytes: HashMap<String, Vec<u8>>,
    /// Best-effort book title from EPUB metadata. Surfaced in the reading
    /// view's chapter list header. `None` when the EPUB has no title meta.
    pub title: Option<String>,
}

impl FormatReader for EpubReader {
    fn name(&self) -> &str {
        "EPUB"
    }

    fn extensions(&self) -> &[&str] {
        &["epub"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read_with_extras(path).map(|(t, _)| t)
    }
}

/// Open an EPUB and return both the flat table representation AND the rich
/// per-chapter Markdown + image bytes used by the reading view.
///
/// Why one entry point with two return values: opening + parsing an EPUB is
/// not free (zip decompression + XML parse for every spine item), so we
/// don't want to do it twice — once via `FormatReader::read_file` and a
/// second time in `apply_loaded_table` for the side state. Callers that
/// only need the table (CLI, MCP `read_table`) can drop the extras.
pub fn read_with_extras(path: &Path) -> Result<(DataTable, EpubExtras)> {
    use rbook::ebook::metadata::TitleKind;

    let epub = Epub::open(path).map_err(|e| anyhow::anyhow!("EPUB open failed: {e}"))?;

    let title = epub
        .metadata()
        .titles()
        .find(|t| t.kind() == TitleKind::Main)
        .or_else(|| epub.metadata().titles().next())
        .map(|t| t.value().to_string());

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut chapters_md: Vec<String> = Vec::new();
    let mut chapter_titles: Vec<String> = Vec::new();

    for (chapter_idx, item) in epub.reader().enumerate() {
        let content = item.map_err(|e| anyhow::anyhow!("EPUB chapter read failed: {e}"))?;
        let href = content.manifest_entry().href().as_str().to_string();
        let xhtml = content.content();
        // htmd preserves headings, lists, emphasis, links, and images. It
        // returns an `io::Error` on malformed input — surface as anyhow.
        let md = htmd::convert(xhtml)
            .map_err(|e| anyhow::anyhow!("EPUB → Markdown failed for {href}: {e}"))?;

        // One row per non-empty paragraph block. A "paragraph" here is a
        // run of text separated by blank lines, which matches what htmd
        // produces (it inserts a blank line between block elements).
        // Chapter index is 1-based to keep the table human-readable.
        let mut paragraph_idx: i64 = 0;
        for para in md.split("\n\n") {
            let trimmed = para.trim();
            if trimmed.is_empty() {
                continue;
            }
            paragraph_idx += 1;
            rows.push(vec![
                CellValue::Int(chapter_idx as i64 + 1),
                CellValue::Int(paragraph_idx),
                CellValue::String(trimmed.to_string()),
            ]);
        }

        // Best-effort chapter title from the manifest href's filename.
        // `c1.xhtml` → "c1.xhtml". When we have no usable href, fall back
        // to "Chapter N" so the dropdown isn't empty.
        let label = href
            .rsplit('/')
            .find(|s| !s.is_empty())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Chapter {}", chapter_idx + 1));
        chapter_titles.push(label);
        chapters_md.push(md);
    }

    // Walk the manifest's image entries. `read_bytes` decompresses the file
    // from the EPUB zip; errors here are non-fatal — we just skip the image
    // and let the renderer fall back to alt text.
    let mut image_bytes: HashMap<String, Vec<u8>> = HashMap::new();
    for img in epub.manifest().images() {
        let href = img.href().as_str().to_string();
        if let Ok(bytes) = img.read_bytes() {
            image_bytes.insert(href, bytes);
        }
    }

    let columns = vec![
        ColumnInfo {
            name: "chapter".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "paragraph".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "text".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("EPUB".to_string());

    let extras = EpubExtras {
        chapters_md,
        chapter_titles,
        image_bytes,
        title,
    };
    Ok((table, extras))
}
