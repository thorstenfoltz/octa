use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

/// Render all pages of a PDF to RGBA images for visual display.
pub fn render_pdf_pages(path: &Path, scale: f32) -> Result<Vec<egui::ColorImage>> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path encoding"))?;
    let doc = mupdf::Document::open(path_str)
        .map_err(|e| anyhow::anyhow!("Failed to open PDF: {}", e))?;
    let page_count = doc
        .page_count()
        .map_err(|e| anyhow::anyhow!("Failed to get page count: {}", e))?;

    let mut images = Vec::with_capacity(page_count as usize);
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
    }

    Ok(images)
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
        let text = pdf_extract::extract_text(path)
            .map_err(|e| anyhow::anyhow!("Failed to extract PDF text: {}", e))?;

        // Split into lines and create a single-column table
        let lines: Vec<&str> = text.lines().collect();

        let columns = vec![
            ColumnInfo {
                name: "line".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "text".to_string(),
                data_type: "Utf8".to_string(),
            },
        ];

        let rows: Vec<Vec<CellValue>> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                vec![
                    CellValue::Int((i + 1) as i64),
                    CellValue::String(line.to_string()),
                ]
            })
            .collect();

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
        })
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        use printpdf::*;

        let (doc, page1, layer1) = PdfDocument::new("Octo Export", Mm(210.0), Mm(297.0), "Layer 1");

        let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;

        let mut current_page = page1;
        let mut current_layer = layer1;
        let mut y_pos: f32 = 280.0; // Start near top of A4 page
        let line_height: f32 = 5.0;
        let margin_bottom: f32 = 15.0;

        // Determine the text column index (prefer column named "text", fallback to last)
        let text_col = table
            .columns
            .iter()
            .position(|c| c.name == "text")
            .unwrap_or(table.col_count().saturating_sub(1));

        for row_idx in 0..table.row_count() {
            if y_pos < margin_bottom {
                // New page
                let (new_page, new_layer) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
                current_page = new_page;
                current_layer = new_layer;
                y_pos = 280.0;
            }

            let text = table
                .get(row_idx, text_col)
                .map(|v| v.to_string())
                .unwrap_or_default();

            let layer = doc.get_page(current_page).get_layer(current_layer);
            layer.use_text(&text, 10.0, Mm(15.0), Mm(y_pos), &font);
            y_pos -= line_height;
        }

        doc.save(&mut std::io::BufWriter::new(std::fs::File::create(path)?))?;
        Ok(())
    }
}
