//! PDF reader: the tables inside a PDF, one per entry in the table picker
//! ("Page 2, table 1"). Read-only.
//!
//! A PDF has no tables, only glyphs at positions. `pdf-extract` (which also
//! decodes the fonts) hands every glyph over with its position; this file
//! groups them into lines, lines into cells at wide horizontal gaps, and runs
//! of lines with two or more cells into tables whose columns are the x
//! ranges those cells share ([`detect_tables`], pure and unit-tested).
//!
//! **Pictures are not text.** A scanned page is one image with no text layer,
//! and Octa does no OCR. Such pages are found (images present, next to no
//! text) and named, so the user is told why a table they can see was not
//! read, instead of getting an empty result. [`scan_note`] carries the same
//! message for a PDF where some pages had tables and others were scanned.
//!
//! ponytail: the detector is a layout heuristic. A header spanning two
//! columns merges them, a wrapped cell or a one-cell line ends the table, and
//! ruled tables are read by text position only (the lines are ignored). A
//! ruling-aware pass is the upgrade if real files need it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};

use super::{FormatReader, TableInfo};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::i18n::t;

/// One glyph as the PDF placed it, in page units (y grows upwards).
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub size: f64,
    pub text: String,
}

/// A table found on a page: rows of cell texts, the first row the header.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundTable {
    pub page: u32,
    pub rows: Vec<Vec<String>>,
}

/// Everything one read of a PDF found.
#[derive(Debug, Clone, Default)]
pub struct PdfScan {
    pub tables: Vec<FoundTable>,
    /// Pages that hold images and (next to) no text: scanned pages.
    pub image_pages: Vec<u32>,
    pub page_count: usize,
}

/// A page with fewer text characters than this, but an image, reads as
/// scanned. A scan often carries a stray page number or stamp as text.
const SCANNED_MAX_CHARS: usize = 20;

pub struct PdfReader;

impl FormatReader for PdfReader {
    fn name(&self) -> &str {
        "PDF"
    }

    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let scan = scan_cached(path)?;
        let first = scan.tables.first().ok_or_else(|| no_table_error(&scan))?;
        Ok(to_table(first))
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let scan = scan_cached(path)?;
        if scan.tables.is_empty() {
            return Err(no_table_error(&scan));
        }
        let mut seen: HashMap<u32, usize> = HashMap::new();
        Ok(Some(
            scan.tables
                .iter()
                .map(|t| {
                    let table = to_table(t);
                    TableInfo {
                        name: table_name(t.page, &mut seen),
                        schema: None,
                        row_count: Some(table.row_count()),
                        columns: table.columns,
                    }
                })
                .collect(),
        ))
    }

    fn read_table(&self, path: &Path, name: &str) -> Result<DataTable> {
        let scan = scan_cached(path)?;
        let mut seen: HashMap<u32, usize> = HashMap::new();
        scan.tables
            .iter()
            .find(|t| table_name(t.page, &mut seen) == name)
            .map(to_table)
            .ok_or_else(|| anyhow::anyhow!("no table named `{name}` in this PDF"))
    }
}

/// `Page 2, table 1`. Stable across calls because tables come back in page
/// order.
fn table_name(page: u32, seen: &mut HashMap<u32, usize>) -> String {
    let n = seen.entry(page).or_insert(0);
    *n += 1;
    format!("Page {page}, table {n}")
}

/// The honest answer when nothing could be read, naming the reason.
fn no_table_error(scan: &PdfScan) -> anyhow::Error {
    let msg = if !scan.image_pages.is_empty() && scan.image_pages.len() == scan.page_count {
        t("pdfread.scanned_all")
    } else if !scan.image_pages.is_empty() {
        t("pdfread.scanned_some").replace("{pages}", &page_list(&scan.image_pages))
    } else {
        t("pdfread.no_table")
    };
    anyhow::anyhow!(msg)
}

fn page_list(pages: &[u32]) -> String {
    pages
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A note for a PDF that DID give tables but also has scanned pages, so the
/// user knows those pages were not read. `None` when there is nothing to say
/// or `path` is not a PDF.
pub fn scan_note(path: &Path) -> Option<String> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return None;
    }
    let scan = scan_cached(path).ok()?;
    (!scan.tables.is_empty() && !scan.image_pages.is_empty())
        .then(|| t("pdfread.note_scanned").replace("{pages}", &page_list(&scan.image_pages)))
}

/// Header from the first row (blank or repeated names made unique), text
/// cells below it, padded to the widest row.
fn to_table(found: &FoundTable) -> DataTable {
    let width = found.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut names: Vec<String> = Vec::with_capacity(width);
    for i in 0..width {
        let raw = found
            .rows
            .first()
            .and_then(|r| r.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("column_{}", i + 1));
        let name = crate::data::recipe::unique_name(&names, &raw);
        names.push(name);
    }
    let mut t = DataTable::empty();
    t.columns = names
        .into_iter()
        .map(|name| ColumnInfo {
            name,
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = found
        .rows
        .iter()
        .skip(1)
        .map(|r| {
            (0..width)
                .map(|i| match r.get(i) {
                    Some(s) if !s.is_empty() => CellValue::String(s.clone()),
                    _ => CellValue::Null,
                })
                .collect()
        })
        .collect();
    t
}

/// Scans keyed by path, dropped when the file changes. The table picker asks
/// for the list and then for one table, and the GUI asks for the note: one
/// parse serves all three.
fn scan_cached(path: &Path) -> Result<PdfScan> {
    static CACHE: Mutex<Option<(PathBuf, std::time::SystemTime, PdfScan)>> = Mutex::new(None);
    let mtime = std::fs::metadata(path)?.modified()?;
    if let Ok(g) = CACHE.lock()
        && let Some((p, m, scan)) = g.as_ref()
        && p == path
        && *m == mtime
    {
        return Ok(scan.clone());
    }
    let scan = scan(path)?;
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((path.to_path_buf(), mtime, scan.clone()));
    }
    Ok(scan)
}

/// Read every page: its glyphs through `pdf-extract`, its images through
/// lopdf.
pub fn scan(path: &Path) -> Result<PdfScan> {
    let mut doc = pdf_extract::Document::load(path)
        .map_err(|e| anyhow::anyhow!("not a readable PDF: {e}"))?;
    if doc.is_encrypted() {
        // Many PDFs are "encrypted" with an empty password just to carry
        // permission flags; those open without asking.
        doc.decrypt("")
            .map_err(|_| anyhow::anyhow!(t("pdfread.protected")))?;
    }
    let pages = doc.get_pages();
    let mut collector = Collector::default();
    // The font decoding lives in a third-party crate that has panicked on
    // unusual fonts before; one bad PDF must not take the app down.
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::output_doc(&doc, &mut collector)
    }));
    match run {
        Ok(Ok(())) => {}
        Ok(Err(e)) => anyhow::bail!("could not read the text of this PDF: {e:?}"),
        Err(_) => anyhow::bail!("could not read the text of this PDF (unsupported font data)"),
    }

    let mut out = PdfScan {
        page_count: pages.len(),
        ..PdfScan::default()
    };
    for (&num, &id) in &pages {
        let glyphs = collector.pages.remove(&num).unwrap_or_default();
        let chars: usize = glyphs.iter().map(|g| g.text.trim().chars().count()).sum();
        let has_image = doc.get_page_images(id).is_ok_and(|v| !v.is_empty());
        if has_image && chars < SCANNED_MAX_CHARS {
            out.image_pages.push(num);
        }
        out.tables.extend(
            detect_tables(&glyphs)
                .into_iter()
                .map(|rows| FoundTable { page: num, rows }),
        );
    }
    Ok(out)
}

/// Gathers glyphs per page from `pdf-extract`.
#[derive(Default)]
struct Collector {
    page: u32,
    pages: HashMap<u32, Vec<Glyph>>,
}

impl OutputDev for Collector {
    fn begin_page(
        &mut self,
        page_num: u32,
        _media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.page = page_num;
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        char: &str,
    ) -> Result<(), OutputError> {
        // `trm` maps glyph space to the page; its scale is the rendered size.
        let scale = (trm.m11 * trm.m11 + trm.m12 * trm.m12).sqrt();
        let size = (font_size * scale).abs().max(1.0);
        self.pages.entry(self.page).or_default().push(Glyph {
            x: trm.m31,
            y: trm.m32,
            width: width * size,
            size,
            text: char.to_string(),
        });
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// A run of text on one line, between two wide gaps.
#[derive(Debug, Clone)]
struct Segment {
    x0: f64,
    x1: f64,
    text: String,
}

/// Group `glyphs` (one page) into tables: lines by baseline, cells at gaps
/// wider than most word spaces, and runs of consecutive lines with two or
/// more cells into one table whose columns are the x ranges the cells share.
pub fn detect_tables(glyphs: &[Glyph]) -> Vec<Vec<Vec<String>>> {
    let lines = lines(glyphs);
    let mut tables = Vec::new();
    let mut run: Vec<(f64, f64, Vec<Segment>)> = Vec::new();
    let flush = |run: &mut Vec<(f64, f64, Vec<Segment>)>, tables: &mut Vec<_>| {
        if run.len() >= 2
            && let Some(t) = to_grid(run)
        {
            tables.push(t);
        }
        run.clear();
    };
    for (y, size, segs) in lines {
        // A line with one cell is prose or a title, never a table row; a
        // big vertical gap ends a table too.
        let far = run
            .last()
            .is_some_and(|(py, psize, _)| py - y > 2.6 * psize.max(size));
        if segs.len() < 2 || far {
            flush(&mut run, &mut tables);
        }
        if segs.len() >= 2 {
            run.push((y, size, segs));
        }
    }
    flush(&mut run, &mut tables);
    tables
}

/// Lines top to bottom, each as its cell segments left to right.
fn lines(glyphs: &[Glyph]) -> Vec<(f64, f64, Vec<Segment>)> {
    let mut gs: Vec<&Glyph> = glyphs
        .iter()
        .filter(|g| !g.text.trim().is_empty())
        .collect();
    gs.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut rows: Vec<Vec<&Glyph>> = Vec::new();
    for g in gs {
        match rows.last_mut() {
            Some(row) if (row[0].y - g.y).abs() <= 0.4 * row[0].size.max(g.size) => row.push(g),
            _ => rows.push(vec![g]),
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            let y = row[0].y;
            let size = row.iter().map(|g| g.size).fold(0.0, f64::max);
            let mut segs: Vec<Segment> = Vec::new();
            let mut prev_end: Option<f64> = None;
            for g in row {
                let gap = prev_end.map(|e| g.x - e);
                match (segs.last_mut(), gap) {
                    // A gap wider than most word spaces starts a new cell.
                    (Some(s), Some(gap)) if gap < 0.9 * g.size => {
                        if gap > 0.15 * g.size {
                            s.text.push(' ');
                        }
                        s.text.push_str(&g.text);
                        s.x1 = g.x + g.width;
                    }
                    _ => segs.push(Segment {
                        x0: g.x,
                        x1: g.x + g.width,
                        text: g.text.clone(),
                    }),
                }
                prev_end = Some(g.x + g.width);
            }
            (y, size, segs)
        })
        .collect()
}

/// Columns = the merged x ranges of every cell in the run; each cell lands in
/// the column its range falls in (two cells in one column join with a
/// space). `None` when the run collapses to fewer than two columns.
fn to_grid(run: &[(f64, f64, Vec<Segment>)]) -> Option<Vec<Vec<String>>> {
    let mut spans: Vec<(f64, f64)> = run
        .iter()
        .flat_map(|(_, _, segs)| segs.iter().map(|s| (s.x0, s.x1)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cols: Vec<(f64, f64)> = Vec::new();
    for (a, b) in spans {
        match cols.last_mut() {
            Some(c) if a <= c.1 + 1.0 => c.1 = c.1.max(b),
            _ => cols.push((a, b)),
        }
    }
    if cols.len() < 2 {
        return None;
    }
    Some(
        run.iter()
            .map(|(_, _, segs)| {
                let mut row = vec![String::new(); cols.len()];
                for s in segs {
                    let i = cols
                        .iter()
                        .position(|&(a, b)| s.x0 >= a - 1.0 && s.x0 <= b)
                        .unwrap_or(cols.len() - 1);
                    if !row[i].is_empty() {
                        row[i].push(' ');
                    }
                    row[i].push_str(&s.text);
                }
                row
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "pdf_reader_tests.rs"]
mod tests;
