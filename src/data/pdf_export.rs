//! Paginated PDF export of a table view.
//!
//! One renderer serves every table surface: the grid, the Summary tab, the
//! Quality report and its section tabs, and any other result tab, because all
//! of them are a `DataTable` plus the row and column lists the user is looking
//! at. Nothing is capped: a long table pages down, a wide one pages across,
//! with the frozen columns and the header row repeating on every page.
//!
//! Mechanism: one SVG per page, each converted with `svg2pdf::to_chunk` and
//! assembled into a single document with `pdf-writer`. Both crates were
//! already in the tree behind the chart export ([`super::chart_export::to_pdf`]),
//! which produces a *single* page and so cannot be reused here.
//!
//! The document chrome ("page 2 of 7") is English, like the HTML report's:
//! these are generated artefacts that travel outside the app. The title and
//! subtitle are passed in, so the caller localises what it wants to.

use std::collections::HashMap;
use std::fmt::Write as _;

use pdf_writer::{Chunk, Content, Finish, Name, Pdf, Rect, Ref};

use super::chart_export::escape_xml;
use super::conditional_format::{CondRule, match_color};
use super::{DataTable, MarkColor};

/// Paper size. Brand names, never translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
}

impl PageSize {
    pub const ALL: &'static [PageSize] = &[PageSize::A4, PageSize::Letter];

    pub fn label(self) -> &'static str {
        match self {
            PageSize::A4 => "A4",
            PageSize::Letter => "Letter",
        }
    }

    /// Portrait width and height in points (1/72 inch), which is also the PDF
    /// user-space unit, so no conversion happens anywhere below.
    pub fn points(self) -> (f32, f32) {
        match self {
            PageSize::A4 => (595.0, 842.0),
            PageSize::Letter => (612.0, 792.0),
        }
    }
}

/// What the export looks like on paper.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    pub page: PageSize,
    pub landscape: bool,
    /// Printed at the top of every page. The tab or file name.
    pub title: String,
    /// One line under the title on the first page, saying what the reader is
    /// looking at: the active filter, the row and column counts. `None` leaves
    /// it out.
    pub subtitle: Option<String>,
}

impl PdfOptions {
    /// Page width and height in points, orientation applied.
    pub fn page_points(&self) -> (f32, f32) {
        let (w, h) = self.page.points();
        if self.landscape { (h, w) } else { (w, h) }
    }
}

/// The view to print: the table plus exactly the rows and columns on screen.
///
/// Indices are the table's own, not a pre-sliced copy, so marks, conditional
/// formats and pending cell edits all resolve against the real cell.
#[derive(Clone, Copy)]
pub struct PdfTable<'a> {
    pub table: &'a DataTable,
    /// Display order of the rows: the filtered, sorted view.
    pub rows: &'a [usize],
    /// Visible columns in display order (hidden ones already dropped).
    pub cols: &'a [usize],
    /// How many leading entries of `cols` are frozen. They repeat on every
    /// page across, the way the header row repeats on every page down.
    pub frozen: usize,
    pub rules: &'a [CondRule],
}

const MARGIN: f32 = 36.0;
const FONT: f32 = 8.0;
const TITLE_FONT: f32 = 13.0;
const SUBTITLE_FONT: f32 = 8.5;
const FOOTER_FONT: f32 = 7.0;
/// Average glyph width as a fraction of the font size, for a proportional
/// sans-serif face. Only ever used to *fit* text, never to place it, so being
/// a few percent out costs a few characters of a truncated cell.
const CHAR_W: f32 = 0.55;
const ROW_H: f32 = FONT * 1.7;
const CELL_PAD: f32 = 3.0;
const MIN_COL_W: f32 = 28.0;
const TITLE_BLOCK: f32 = TITLE_FONT + 10.0;
const SUBTITLE_BLOCK: f32 = SUBTITLE_FONT + 6.0;
const FOOTER_BLOCK: f32 = FOOTER_FONT + 8.0;
/// Rows scanned per column when sizing it. The whole column would be a full
/// pass over a multi-million-row table for a width nobody measures.
/// ponytail: first-N sample, widen it if truncated cells show up in practice.
const WIDTH_SAMPLE_ROWS: usize = 200;

/// Estimated width of `s` at `font` points.
fn text_width(s: &str, font: f32) -> f32 {
    s.chars().count() as f32 * font * CHAR_W
}

/// `s` shortened to fit `max_w`, with an ASCII ellipsis when it was cut.
fn fit(s: &str, max_w: f32, font: f32) -> String {
    if text_width(s, font) <= max_w {
        return s.to_string();
    }
    let per_char = font * CHAR_W;
    if per_char <= 0.0 {
        return String::new();
    }
    let keep = ((max_w / per_char).floor() as usize).saturating_sub(3);
    if keep == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// One line of cell text: a newline in a cell would break the SVG line box.
fn cell_text(view: &PdfTable<'_>, row: usize, col: usize) -> String {
    view.table
        .get(row, col)
        .map(|v| v.to_string())
        .unwrap_or_default()
        .replace(['\n', '\r'], " ")
}

/// Background colour for a cell, matching what the grid paints: a colour mark
/// wins over a conditional-format rule, exactly as in the table view.
fn cell_fill(view: &PdfTable<'_>, row: usize, col: usize, text: &str) -> Option<MarkColor> {
    view.table
        .get_mark_color(row, col)
        .or_else(|| match_color(view.rules, col, text))
}

/// The mark palette as CSS, kept identical to `ui::theme`'s `mark_color` so a
/// printed page and the screen agree.
fn mark_css(c: MarkColor) -> &'static str {
    match c {
        MarkColor::Red => "#dc2626",
        MarkColor::Orange => "#ea580c",
        MarkColor::Yellow => "#facc15",
        MarkColor::Green => "#22c55e",
        MarkColor::Blue => "#3b82f6",
        MarkColor::Purple => "#a855f7",
    }
}

/// Width of every visible column, in points.
pub(crate) fn column_widths(view: &PdfTable<'_>, printable_w: f32) -> Vec<f32> {
    // No single column may eat the page: past this it is truncated and the
    // rest of the table still gets printed.
    let max_col = (printable_w * 0.4).max(MIN_COL_W * 2.0);
    view.cols
        .iter()
        .map(|&col| {
            let header = view
                .table
                .columns
                .get(col)
                .map(|c| c.name.as_str())
                .unwrap_or("");
            let mut w = text_width(header, FONT);
            for &row in view.rows.iter().take(WIDTH_SAMPLE_ROWS) {
                w = w.max(text_width(&cell_text(view, row, col), FONT));
            }
            (w + CELL_PAD * 2.0).clamp(MIN_COL_W, max_col)
        })
        .collect()
}

/// Group the columns into pages across. Every block starts with the frozen
/// columns; a column too wide for a page still gets its own block rather than
/// being dropped.
pub(crate) fn column_blocks(widths: &[f32], frozen: usize, avail_w: f32) -> Vec<Vec<usize>> {
    let n = widths.len();
    if n == 0 {
        return Vec::new();
    }
    let frozen = frozen.min(n);
    if frozen == n {
        return vec![(0..n).collect()];
    }
    let frozen_w: f32 = widths[..frozen].iter().sum();
    let mut blocks = Vec::new();
    let mut i = frozen;
    while i < n {
        let mut block: Vec<usize> = (0..frozen).collect();
        let mut w = frozen_w + widths[i];
        block.push(i);
        i += 1;
        while i < n && w + widths[i] <= avail_w {
            w += widths[i];
            block.push(i);
            i += 1;
        }
        blocks.push(block);
    }
    blocks
}

/// How many data rows fit between the repeated header and the footer.
pub(crate) fn rows_per_page(body_h: f32) -> usize {
    (((body_h - ROW_H) / ROW_H).floor() as usize).max(1)
}

/// Page count for a view, so a dialog can say what pressing Export costs
/// before it costs it.
pub fn page_count(view: &PdfTable<'_>, opts: &PdfOptions) -> usize {
    let (pw, ph) = opts.page_points();
    let widths = column_widths(view, pw - MARGIN * 2.0);
    let across = column_blocks(&widths, view.frozen, pw - MARGIN * 2.0).len();
    let per_page = rows_per_page(body_height(ph, opts));
    let down = view.rows.len().div_ceil(per_page).max(1);
    across.max(1) * down
}

/// Height available to the header row plus the data rows.
fn body_height(ph: f32, opts: &PdfOptions) -> f32 {
    let subtitle = if opts.subtitle.is_some() {
        SUBTITLE_BLOCK
    } else {
        0.0
    };
    (ph - MARGIN * 2.0 - TITLE_BLOCK - subtitle - FOOTER_BLOCK).max(ROW_H * 2.0)
}

/// One page's share of the export: which columns, which rows, where in the
/// document. A struct rather than seven positional parameters, four of which
/// are `usize` and would swap silently.
struct PageJob<'a> {
    widths: &'a [f32],
    /// Indices into `widths` / `PdfTable::cols` printed on this page.
    block: &'a [usize],
    rows: &'a [usize],
    /// Position of `rows[0]` in the whole view, for the footer.
    first_row: usize,
    page_no: usize,
    pages: usize,
}

/// Render one page as an SVG document.
fn page_svg(view: &PdfTable<'_>, opts: &PdfOptions, job: PageJob<'_>) -> String {
    let PageJob {
        widths,
        block,
        rows,
        first_row,
        page_no,
        pages,
    } = job;
    // The subtitle says what the whole document is; repeating it on page 40
    // would only cost a row of data.
    let show_subtitle = page_no == 1;
    let (pw, ph) = opts.page_points();
    let mut svg = String::with_capacity(4096);
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="0 0 {pw} {ph}">"#
    );
    let _ = write!(
        svg,
        r##"<rect width="{pw}" height="{ph}" fill="#ffffff"/>"##
    );

    let mut y = MARGIN + TITLE_FONT;
    let _ = write!(
        svg,
        r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="{TITLE_FONT}" font-weight="600" fill="#111111">{t}</text>"##,
        x = MARGIN,
        t = escape_xml(&fit(&opts.title, pw - MARGIN * 2.0, TITLE_FONT)),
    );
    y = MARGIN + TITLE_BLOCK;
    if let Some(sub) = &opts.subtitle {
        if show_subtitle {
            let _ = write!(
                svg,
                r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="{SUBTITLE_FONT}" fill="#555555">{t}</text>"##,
                x = MARGIN,
                y = y + SUBTITLE_FONT,
                t = escape_xml(&fit(sub, pw - MARGIN * 2.0, SUBTITLE_FONT)),
            );
        }
        y += SUBTITLE_BLOCK;
    }

    // Header row.
    let total_w: f32 = block.iter().map(|&i| widths[i]).sum();
    let _ = write!(
        svg,
        r##"<rect x="{MARGIN}" y="{y:.1}" width="{total_w:.1}" height="{ROW_H:.1}" fill="#eeeeee"/>"##
    );
    let mut x = MARGIN;
    for &i in block {
        let name = view
            .table
            .columns
            .get(view.cols[i])
            .map(|c| c.name.as_str())
            .unwrap_or("");
        let _ = write!(
            svg,
            r##"<text x="{tx:.1}" y="{ty:.1}" font-family="sans-serif" font-size="{FONT}" font-weight="600" fill="#111111">{t}</text>"##,
            tx = x + CELL_PAD,
            ty = y + ROW_H - CELL_PAD - 1.0,
            t = escape_xml(&fit(name, widths[i] - CELL_PAD * 2.0, FONT)),
        );
        x += widths[i];
    }
    y += ROW_H;

    // Data rows.
    for (n, &row) in rows.iter().enumerate() {
        if n % 2 == 1 {
            let _ = write!(
                svg,
                r##"<rect x="{MARGIN}" y="{y:.1}" width="{total_w:.1}" height="{ROW_H:.1}" fill="#f7f7f7"/>"##
            );
        }
        let mut x = MARGIN;
        for &i in block {
            let col = view.cols[i];
            let text = cell_text(view, row, col);
            if let Some(mark) = cell_fill(view, row, col, &text) {
                let _ = write!(
                    svg,
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{ROW_H:.1}" fill="{c}" fill-opacity="0.35"/>"##,
                    w = widths[i],
                    c = mark_css(mark),
                );
            }
            let numeric = text.trim().parse::<f64>().is_ok();
            let shown = fit(&text, widths[i] - CELL_PAD * 2.0, FONT);
            let (tx, anchor) = if numeric {
                (x + widths[i] - CELL_PAD, " text-anchor=\"end\"")
            } else {
                (x + CELL_PAD, "")
            };
            let _ = write!(
                svg,
                r##"<text x="{tx:.1}" y="{ty:.1}"{anchor} font-family="sans-serif" font-size="{FONT}" fill="#222222">{t}</text>"##,
                ty = y + ROW_H - CELL_PAD - 1.0,
                t = escape_xml(&shown),
            );
            x += widths[i];
        }
        y += ROW_H;
    }

    // Grid lines: one under the header, one under the last row, and one at
    // each column seam. Enough to read a table by, cheap to draw.
    let _ = write!(
        svg,
        r##"<rect x="{MARGIN}" y="{top:.1}" width="{total_w:.1}" height="{h:.1}" fill="none" stroke="#cccccc" stroke-width="0.5"/>"##,
        top = y - ROW_H * (rows.len() as f32 + 1.0),
        h = ROW_H * (rows.len() as f32 + 1.0),
    );

    // Footer.
    let cols_note = if view.cols.len() > block.len() {
        format!(
            "  -  columns {}-{} of {}",
            block.first().map(|&i| i + 1).unwrap_or(0),
            block.last().map(|&i| i + 1).unwrap_or(0),
            view.cols.len()
        )
    } else {
        String::new()
    };
    let rows_note = if rows.is_empty() {
        "no rows".to_string()
    } else {
        format!(
            "rows {}-{} of {}",
            first_row + 1,
            first_row + rows.len(),
            view.rows.len()
        )
    };
    let _ = write!(
        svg,
        r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="{FOOTER_FONT}" fill="#666666">{t}</text>"##,
        x = MARGIN,
        y = ph - MARGIN,
        t = escape_xml(&fit(
            &format!(
                "{}  -  page {} of {}  -  {rows_note}{cols_note}",
                opts.title, page_no, pages
            ),
            pw - MARGIN * 2.0,
            FOOTER_FONT
        )),
    );

    svg.push_str("</svg>");
    svg
}

/// Every page of the export, as SVG documents. Public for tests and for a
/// caller that wants the pages without a PDF around them.
pub fn page_svgs(view: &PdfTable<'_>, opts: &PdfOptions) -> Vec<String> {
    let (pw, ph) = opts.page_points();
    let printable_w = pw - MARGIN * 2.0;
    let widths = column_widths(view, printable_w);
    let blocks = column_blocks(&widths, view.frozen, printable_w);
    if blocks.is_empty() {
        return Vec::new();
    }
    let per_page = rows_per_page(body_height(ph, opts));

    // An empty view still prints one page per column block: the header row
    // and the "no rows" footer are the finding.
    let row_chunks: Vec<(usize, &[usize])> = if view.rows.is_empty() {
        vec![(0, &[][..])]
    } else {
        view.rows
            .chunks(per_page)
            .enumerate()
            .map(|(i, c)| (i * per_page, c))
            .collect()
    };

    let pages = row_chunks.len() * blocks.len();
    let mut out = Vec::with_capacity(pages);
    for (first_row, chunk) in &row_chunks {
        for block in &blocks {
            let page_no = out.len() + 1;
            out.push(page_svg(
                view,
                opts,
                PageJob {
                    widths: &widths,
                    block,
                    rows: chunk,
                    first_row: *first_row,
                    page_no,
                    pages,
                },
            ));
        }
    }
    out
}

/// Render the view as a paginated PDF document.
pub fn table_to_pdf(view: &PdfTable<'_>, opts: &PdfOptions) -> Result<Vec<u8>, String> {
    let pages = page_svgs(view, opts);
    if pages.is_empty() {
        return Err("nothing to export".to_string());
    }
    let (pw, ph) = opts.page_points();

    let mut svg_opt = svg2pdf::usvg::Options::default();
    svg_opt.fontdb_mut().load_system_fonts();

    // Ids for the whole document are handed out from one allocator, including
    // the ones inside each converted page, which is what `renumber` fixes up.
    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let page_tree_id = alloc.bump();

    struct PageObj {
        page_id: Ref,
        content_id: Ref,
        svg_id: Ref,
        chunk: Chunk,
    }

    let mut objs: Vec<PageObj> = Vec::with_capacity(pages.len());
    for svg in &pages {
        let tree = svg2pdf::usvg::Tree::from_str(svg, &svg_opt).map_err(|e| e.to_string())?;
        let (chunk, svg_ref) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
            .map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let svg_id = *map
            .get(&svg_ref)
            .ok_or_else(|| "page graphic lost its reference".to_string())?;
        objs.push(PageObj {
            page_id: alloc.bump(),
            content_id: alloc.bump(),
            svg_id,
            chunk,
        });
    }

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(objs.iter().map(|o| o.page_id))
        .count(objs.len() as i32);

    for obj in &objs {
        let mut page = pdf.page(obj.page_id);
        page.media_box(Rect::new(0.0, 0.0, pw, ph));
        page.parent(page_tree_id);
        page.contents(obj.content_id);
        let mut resources = page.resources();
        resources.x_objects().pair(Name(b"S1"), obj.svg_id);
        resources.finish();
        page.finish();

        // The XObject is normalised to the unit square, so the transform is
        // the page size itself.
        let mut content = Content::new();
        content
            .transform([pw, 0.0, 0.0, ph, 0.0, 0.0])
            .x_object(Name(b"S1"));
        pdf.stream(obj.content_id, &content.finish());
        pdf.extend(&obj.chunk);
    }

    Ok(pdf.finish())
}

#[cfg(test)]
#[path = "pdf_export_tests.rs"]
mod tests;
