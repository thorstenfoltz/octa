use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

/// Render all pages of a PDF to RGBA images for visual display.
/// Also returns the extracted text per page for copy support.
pub fn render_pdf_pages(path: &Path, scale: f32) -> Result<(Vec<egui::ColorImage>, Vec<String>)> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path encoding"))?;
    let doc = mupdf::Document::open(path_str)
        .map_err(|e| anyhow::anyhow!("Failed to open PDF: {}", e))?;
    let page_count = doc
        .page_count()
        .map_err(|e| anyhow::anyhow!("Failed to get page count: {}", e))?;

    let mut images = Vec::with_capacity(page_count as usize);
    let mut texts = Vec::with_capacity(page_count as usize);
    let matrix = mupdf::Matrix::new_scale(scale, scale);

    for i in 0..page_count {
        let page = doc
            .load_page(i)
            .map_err(|e| anyhow::anyhow!("Failed to load page {}: {}", i, e))?;
        let pixmap = page
            .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false, true)
            .map_err(|e| anyhow::anyhow!("Failed to render page {}: {}", i, e))?;

        let width = pixmap.width() as usize;
        let height = pixmap.height() as usize;
        let samples = pixmap.samples();

        // mupdf RGB pixmap: 3 bytes per pixel, convert to RGBA
        let n = pixmap.n() as usize; // components per pixel
        let mut rgba = Vec::with_capacity(width * height * 4);
        for pixel in samples.chunks_exact(n) {
            rgba.push(pixel[0]); // R
            rgba.push(pixel[1]); // G
            rgba.push(pixel[2]); // B
            rgba.push(if n >= 4 { pixel[3] } else { 255 }); // A
        }

        let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba);
        images.push(image);

        // Extract text from page for copy support
        let page_text = page
            .to_text_page(mupdf::TextPageFlags::empty())
            .and_then(|tp| tp.to_text())
            .unwrap_or_default();
        texts.push(page_text);
    }

    Ok((images, texts))
}

pub struct PdfReader;

impl FormatReader for PdfReader {
    fn name(&self) -> &str {
        "PDF"
    }

    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        // Use mupdf (the same engine that renders the page bitmaps) so the
        // table-view rows are aligned with the per-page text shown in the
        // PDF view. `pdf_extract` was used historically but it has no page
        // boundary information — every line ended up on a synthetic page 1.
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path encoding"))?;
        let doc = mupdf::Document::open(path_str)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF: {}", e))?;
        let page_count = doc
            .page_count()
            .map_err(|e| anyhow::anyhow!("Failed to get page count: {}", e))?;

        let columns = vec![
            ColumnInfo {
                name: "page".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "line".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "text".to_string(),
                data_type: "Utf8".to_string(),
            },
        ];

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        for i in 0..page_count {
            let page = doc
                .load_page(i)
                .map_err(|e| anyhow::anyhow!("Failed to load page {}: {}", i, e))?;
            let page_text = page
                .to_text_page(mupdf::TextPageFlags::empty())
                .and_then(|tp| tp.to_text())
                .unwrap_or_default();
            for (line_idx, line) in page_text.lines().enumerate() {
                rows.push(vec![
                    CellValue::Int((i + 1) as i64),
                    CellValue::Int((line_idx + 1) as i64),
                    CellValue::String(line.to_string()),
                ]);
            }
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("PDF".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    // PDF is read-only. Octa's previous writer round-tripped only the
    // extracted text, which lost layout/typography/embedded objects from the
    // source document. The trait default returns `false` for `supports_write`
    // so the menu hides Save/Save As for PDF tabs.
}

/// Group the live table rows by `page` and join their `text` cells with
/// newlines, returning one string per page (1-indexed). Used by the PDF
/// view to keep the selectable text frame in sync with table edits without
/// re-rendering the page bitmaps. Returns an empty Vec if the table doesn't
/// have the expected columns.
pub fn page_texts_from_table(table: &DataTable) -> Vec<String> {
    let page_col = match table.columns.iter().position(|c| c.name == "page") {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    let text_col = match table.columns.iter().position(|c| c.name == "text") {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    let mut max_page = 0usize;
    for row_idx in 0..table.row_count() {
        if let Some(CellValue::Int(p)) = table.get(row_idx, page_col) {
            if *p > 0 {
                max_page = max_page.max(*p as usize);
            }
        }
    }
    let mut buckets: Vec<String> = vec![String::new(); max_page];
    for row_idx in 0..table.row_count() {
        if let Some(CellValue::Int(p)) = table.get(row_idx, page_col) {
            if *p > 0 && (*p as usize) <= max_page {
                let bucket = &mut buckets[(*p as usize) - 1];
                if !bucket.is_empty() {
                    bucket.push('\n');
                }
                let text = table
                    .get(row_idx, text_col)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                bucket.push_str(&text);
            }
        }
    }
    buckets
}
