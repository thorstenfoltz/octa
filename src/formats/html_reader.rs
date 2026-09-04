//! HTML tables. Every `<table>` in the document becomes one Octa table, the
//! way every sheet of a workbook does, so a page of forty tables opens through
//! the same picker and the same auto-open cap as an Excel file.
//!
//! Parsed with `html5ever`, the browser-grade parser already in the tree (via
//! `htmd`, the chat panel's HTML-to-Markdown converter), so the tag soup that
//! real pages are made of parses the way a browser would rather than the way a
//! regex would.
//!
//! Two rules worth stating:
//!
//! - **`rowspan` and `colspan` expand into repeated cells**, because a grid
//!   with holes in it is not a table anyone can filter or sort. A cell
//!   spanning three columns becomes the same value three times.
//! - **A nested table is listed too, and its text also stays in the cell that
//!   holds it.** Pages built around layout tables wrap the real table in an
//!   outer one, so taking only the outermost would swallow the data into a
//!   single cell. What is dropped instead is a **pure wrapper**: a table whose
//!   own cells hold nothing but other tables, which is layout markup and never
//!   data.
//!
//! There is no separate download step: **Open URL** already fetches to a temp
//! file and hands it to the registry, so a Wikipedia article opens as its
//! tables.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{FormatReader, TableInfo};

pub struct HtmlReader;

/// One row of a parsed table, and whether every cell in it was a `<th>`.
struct HtmlRow {
    cells: Vec<String>,
    all_headers: bool,
}

/// One `<table>`: its caption (when it has one) and its expanded grid.
struct HtmlTable {
    caption: Option<String>,
    rows: Vec<HtmlRow>,
    /// Whether any cell holds text of its own rather than only a nested
    /// table. A pure wrapper is layout, and is not listed.
    has_own_content: bool,
}

impl FormatReader for HtmlReader {
    fn name(&self) -> &str {
        "HTML"
    }

    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let tables = parse_tables(&read_lossy(path)?);
        match tables.into_iter().next() {
            Some(t) => Ok(to_data_table(&t)),
            None => anyhow::bail!(
                "{} has no <table> in it. Reopen it as Text (View -> Reopen as) to see the \
                 page itself.",
                path.display()
            ),
        }
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let tables = parse_tables(&read_lossy(path)?);
        if tables.is_empty() {
            return Ok(None);
        }
        let names = table_names(&tables);
        Ok(Some(
            tables
                .iter()
                .zip(names)
                .map(|(t, name)| {
                    let table = to_data_table(t);
                    TableInfo {
                        name,
                        schema: None,
                        columns: table.columns.clone(),
                        row_count: Some(table.row_count()),
                    }
                })
                .collect(),
        ))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let tables = parse_tables(&read_lossy(path)?);
        let names = table_names(&tables);
        match names.iter().position(|n| n == table) {
            Some(i) => Ok(to_data_table(&tables[i])),
            None => anyhow::bail!("{} has no table called '{table}'", path.display()),
        }
    }

    /// Like a workbook: every table opens, subject to the app's own auto-open
    /// cap and multi-select picker above it.
    fn opens_all_tables(&self) -> bool {
        true
    }
}

/// Read as text, replacing anything that is not UTF-8 rather than refusing the
/// file: a page served as Latin-1 is still a page full of tables.
fn read_lossy(path: &Path) -> Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).into_owned())
}

/// Display names for the parsed tables: the caption when there is one, else
/// `Table N`, with a numeric suffix when two captions collide (the name is
/// also the identifier [`FormatReader::read_table`] is handed back).
fn table_names(tables: &[HtmlTable]) -> Vec<String> {
    let mut used: HashMap<String, usize> = HashMap::new();
    tables
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let base = match t
                .caption
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                Some(c) => shorten(c),
                None => format!("Table {}", i + 1),
            };
            let seen = used.entry(base.clone()).or_insert(0);
            *seen += 1;
            if *seen == 1 {
                base
            } else {
                format!("{base} ({seen})")
            }
        })
        .collect()
}

/// A caption cut to tab-title length, on a word boundary: a Wikipedia
/// caption is a sentence, and cutting it mid-word reads like a bug.
fn shorten(caption: &str) -> String {
    const MAX: usize = 60;
    if caption.chars().count() <= MAX {
        return caption.to_string();
    }
    let cut: String = caption.chars().take(MAX).collect();
    match cut.rfind(' ') {
        Some(i) if i > MAX / 3 => cut[..i].to_string(),
        _ => cut,
    }
}

/// Every outermost `<table>` in the document, in document order.
fn parse_tables(html: &str) -> Vec<HtmlTable> {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(html);
    let mut out = Vec::new();
    collect_tables(&dom.document, &mut out);
    out
}

fn collect_tables(node: &Handle, out: &mut Vec<HtmlTable>) {
    if is_element(node, "table") {
        let table = extract_table(node);
        if table.has_own_content {
            out.push(table);
        }
        // Then keep walking: a nested table is a table in its own right, and
        // on a layout-driven page it is the one holding the data.
    }
    for child in node.children.borrow().iter() {
        collect_tables(child, out);
    }
}

/// Pull one table's caption and rows out of its element.
fn extract_table(table: &Handle) -> HtmlTable {
    let mut caption = None;
    let mut raw_rows: Vec<(Vec<Cell>, bool)> = Vec::new();
    collect_rows(table, &mut caption, &mut raw_rows, true);
    let has_own_content = raw_rows
        .iter()
        .any(|(cells, _)| cells.iter().any(|c| c.has_own_text));
    HtmlTable {
        caption,
        rows: expand_spans(raw_rows),
        has_own_content,
    }
}

/// One cell before the grid is squared off.
struct Cell {
    text: String,
    /// Whether this cell has text outside any table nested inside it. A cell
    /// that only wraps another table is layout, not data.
    has_own_text: bool,
    colspan: usize,
    rowspan: usize,
}

/// Walk a table for its caption and its `<tr>`s, without descending into a
/// nested table (`root` marks the table element the walk started at).
fn collect_rows(
    node: &Handle,
    caption: &mut Option<String>,
    rows: &mut Vec<(Vec<Cell>, bool)>,
    root: bool,
) {
    if !root && is_element(node, "table") {
        return;
    }
    if is_element(node, "caption") && caption.is_none() {
        *caption = Some(text_of(node));
        return;
    }
    if is_element(node, "tr") {
        let mut cells = Vec::new();
        collect_cells(node, &mut cells, true);
        if !cells.is_empty() {
            let all_headers = header_only(node);
            rows.push((cells, all_headers));
        }
        return;
    }
    for child in node.children.borrow().iter() {
        collect_rows(child, caption, rows, false);
    }
}

/// The `<th>` / `<td>` of one row, again without entering a nested table.
fn collect_cells(node: &Handle, out: &mut Vec<Cell>, root: bool) {
    if !root && (is_element(node, "table") || is_element(node, "tr")) {
        return;
    }
    if is_element(node, "th") || is_element(node, "td") {
        out.push(Cell {
            text: text_of(node),
            has_own_text: !own_text_of(node).is_empty(),
            colspan: span_attr(node, "colspan"),
            rowspan: span_attr(node, "rowspan"),
        });
        return;
    }
    for child in node.children.borrow().iter() {
        collect_cells(child, out, false);
    }
}

/// Whether every cell of this row is a `<th>`, which is what makes it a
/// header row rather than data.
fn header_only(row: &Handle) -> bool {
    let mut cells = Vec::new();
    collect_cell_names(row, &mut cells, true);
    !cells.is_empty() && cells.iter().all(|n| n == "th")
}

fn collect_cell_names(node: &Handle, out: &mut Vec<String>, root: bool) {
    if !root && (is_element(node, "table") || is_element(node, "tr")) {
        return;
    }
    if is_element(node, "th") {
        out.push("th".into());
        return;
    }
    if is_element(node, "td") {
        out.push("td".into());
        return;
    }
    for child in node.children.borrow().iter() {
        collect_cell_names(child, out, false);
    }
}

/// `colspan` / `rowspan` as a sane number: absent, unparseable or zero all
/// mean one, and the cap keeps a hostile `rowspan="99999999"` from turning
/// into a memory accident.
fn span_attr(node: &Handle, name: &str) -> usize {
    let NodeData::Element { attrs, .. } = &node.data else {
        return 1;
    };
    attrs
        .borrow()
        .iter()
        .find(|a| a.name.local.as_ref().eq_ignore_ascii_case(name))
        .and_then(|a| a.value.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1)
        .min(1000)
}

/// Square the grid off: a spanning cell is repeated into every position it
/// covers, and short rows are padded so every row has the same width.
fn expand_spans(raw: Vec<(Vec<Cell>, bool)>) -> Vec<HtmlRow> {
    // column -> (rows still to fill, text)
    let mut carry: HashMap<usize, (usize, String)> = HashMap::new();
    let mut rows: Vec<HtmlRow> = Vec::with_capacity(raw.len());
    for (cells, all_headers) in raw {
        let mut out: Vec<String> = Vec::new();
        let mut col = 0usize;
        let take_carry = |out: &mut Vec<String>,
                          col: &mut usize,
                          carry: &mut HashMap<usize, (usize, String)>| {
            while let Some((left, text)) = carry.remove(col) {
                out.push(text.clone());
                if left > 1 {
                    carry.insert(*col, (left - 1, text));
                }
                *col += 1;
            }
        };
        for cell in cells {
            take_carry(&mut out, &mut col, &mut carry);
            for _ in 0..cell.colspan {
                out.push(cell.text.clone());
                if cell.rowspan > 1 {
                    carry.insert(col, (cell.rowspan - 1, cell.text.clone()));
                }
                col += 1;
            }
        }
        take_carry(&mut out, &mut col, &mut carry);
        rows.push(HtmlRow {
            cells: out,
            all_headers,
        });
    }
    let width = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    for row in &mut rows {
        row.cells.resize(width, String::new());
    }
    rows
}

/// Turn a parsed table into a [`DataTable`]: a leading all-`<th>` row becomes
/// the header, otherwise the columns are numbered.
fn to_data_table(t: &HtmlTable) -> DataTable {
    let mut table = DataTable::empty();
    table.format_name = Some("HTML".to_string());
    let mut rows = t.rows.iter();
    let header: Option<&HtmlRow> = t.rows.first().filter(|r| r.all_headers);
    if header.is_some() {
        rows.next();
    }
    let width = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    table.columns = (0..width)
        .map(|i| ColumnInfo {
            name: header
                .and_then(|h| h.cells.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Column {}", i + 1)),
            // Text, like the other document readers: the app's load passes
            // promote dates and numbers afterwards, the same way they do for
            // XML or Markdown.
            data_type: "Utf8".to_string(),
        })
        .collect();
    let cap = crate::formats::initial_load_rows();
    table.rows = rows
        .take(cap)
        .map(|r| {
            (0..width)
                .map(|i| match r.cells.get(i) {
                    Some(s) if !s.is_empty() => CellValue::String(s.clone()),
                    _ => CellValue::Null,
                })
                .collect()
        })
        .collect();
    table
}

fn is_element(node: &Handle, name: &str) -> bool {
    matches!(&node.data, NodeData::Element { name: n, .. } if n.local.as_ref() == name)
}

/// All text under a node, with runs of whitespace collapsed. `<script>` and
/// `<style>` contents are skipped: they are code that happens to sit in the
/// markup, never a cell's value.
fn text_of(node: &Handle) -> String {
    let mut buf = String::new();
    push_text(node, &mut buf, false, true);
    buf.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text this node holds *outside* any table nested inside it, which is
/// what tells a data cell from a layout wrapper.
fn own_text_of(node: &Handle) -> String {
    let mut buf = String::new();
    push_text(node, &mut buf, true, true);
    buf.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_text(node: &Handle, buf: &mut String, stop_at_tables: bool, root: bool) {
    if is_element(node, "script") || is_element(node, "style") {
        return;
    }
    if stop_at_tables && !root && is_element(node, "table") {
        return;
    }
    if let NodeData::Text { contents } = &node.data {
        buf.push_str(&contents.borrow());
    }
    // A line break inside a cell is a space, not a join: "a<br>b" is two
    // words, never "ab".
    if is_element(node, "br") {
        buf.push(' ');
    }
    for child in node.children.borrow().iter() {
        push_text(child, buf, stop_at_tables, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_row_becomes_the_columns() {
        let t = parse_tables(
            "<table><tr><th>id</th><th>name</th></tr>\
             <tr><td>1</td><td>ada</td></tr></table>",
        );
        assert_eq!(t.len(), 1);
        let dt = to_data_table(&t[0]);
        let names: Vec<&str> = dt.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name"]);
        assert_eq!(dt.row_count(), 1);
        assert_eq!(dt.rows[0][1], CellValue::String("ada".into()));
    }

    /// Without a header row the columns are numbered and no data is eaten.
    #[test]
    fn a_table_without_th_keeps_every_row() {
        let dt = to_data_table(&parse_tables("<table><tr><td>1</td><td>2</td></tr></table>")[0]);
        assert_eq!(dt.columns[0].name, "Column 1");
        assert_eq!(dt.row_count(), 1);
    }

    /// A spanning cell is repeated into every position it covers: a grid with
    /// holes cannot be sorted or filtered.
    #[test]
    fn spans_expand_into_repeated_cells() {
        let dt = to_data_table(
            &parse_tables(
                "<table>\
                 <tr><td colspan=\"2\">wide</td><td>x</td></tr>\
                 <tr><td rowspan=\"2\">tall</td><td>a</td><td>b</td></tr>\
                 <tr><td>c</td><td>d</td></tr></table>",
            )[0],
        );
        assert_eq!(dt.rows[0][0], CellValue::String("wide".into()));
        assert_eq!(dt.rows[0][1], CellValue::String("wide".into()));
        assert_eq!(dt.rows[1][0], CellValue::String("tall".into()));
        // The third row starts with the carried "tall", then its own cells.
        assert_eq!(dt.rows[2][0], CellValue::String("tall".into()));
        assert_eq!(dt.rows[2][1], CellValue::String("c".into()));
        assert_eq!(dt.rows[2][2], CellValue::String("d".into()));
    }

    /// A nested table is listed in its own right *and* its text stays in the
    /// cell that holds it, so neither reading of the page is lost.
    #[test]
    fn nested_tables_are_listed_and_stay_in_their_cell() {
        let tables = parse_tables(
            "<table><tr><td>outer <table><tr><td>inner</td></tr></table></td></tr></table>",
        );
        assert_eq!(tables.len(), 2);
        assert_eq!(
            to_data_table(&tables[0]).rows[0][0],
            CellValue::String("outer inner".into())
        );
        assert_eq!(
            to_data_table(&tables[1]).rows[0][0],
            CellValue::String("inner".into())
        );
    }

    /// The classic layout page: a table whose only job is to position another
    /// one. Listing it would bury the real table's data in a single cell.
    #[test]
    fn a_pure_layout_wrapper_is_dropped() {
        let tables = parse_tables(
            "<table class=\"layout\"><tr><td>\
             <table><tr><th>id</th></tr><tr><td>1</td></tr></table>\
             </td></tr></table>",
        );
        assert_eq!(tables.len(), 1);
        let dt = to_data_table(&tables[0]);
        assert_eq!(dt.columns[0].name, "id");
        assert_eq!(dt.row_count(), 1);
    }

    /// Real pages are tag soup; the parser is the browser's, so an unclosed
    /// row or a missing `<tbody>` is not an error.
    #[test]
    fn unclosed_markup_still_parses() {
        let dt = to_data_table(&parse_tables("<table><tr><td>a<tr><td>b</table>")[0]);
        assert_eq!(dt.row_count(), 2);
        assert_eq!(dt.rows[1][0], CellValue::String("b".into()));
    }

    /// A long caption is cut on a word boundary: the tab title is the whole
    /// identity of the table in the picker.
    #[test]
    fn long_captions_are_cut_between_words() {
        let name = shorten("List of countries and inhabited territories by total population");
        assert_eq!(name, "List of countries and inhabited territories by total");
        assert_eq!(shorten("Short one"), "Short one");
    }

    #[test]
    fn names_come_from_captions_and_stay_unique() {
        let tables = parse_tables(
            "<table><caption>Population</caption><tr><td>1</td></tr></table>\
             <table><caption>Population</caption><tr><td>2</td></tr></table>\
             <table><tr><td>3</td></tr></table>",
        );
        assert_eq!(
            table_names(&tables),
            vec!["Population", "Population (2)", "Table 3"]
        );
    }

    /// Script and style are code that happens to be in the markup, and a
    /// `<br>` separates words rather than joining them.
    #[test]
    fn cell_text_is_the_text_a_reader_sees() {
        let dt = to_data_table(
            &parse_tables(
                "<table><tr><td>a<br>b<script>var x=1;</script></td>\
                 <td>  spaced   out </td></tr></table>",
            )[0],
        );
        assert_eq!(dt.rows[0][0], CellValue::String("a b".into()));
        assert_eq!(dt.rows[0][1], CellValue::String("spaced out".into()));
    }
}

#[cfg(test)]
mod real_page_tests {
    /// Point `OCTA_TEST_HTML` at a real page and this reports what the reader
    /// makes of it: table count, names, and how long the parse took. Skipped
    /// when unset, so it costs nothing in CI.
    #[test]
    fn report_on_a_real_page() {
        let Ok(path) = std::env::var("OCTA_TEST_HTML") else {
            return;
        };
        let html = super::read_lossy(std::path::Path::new(&path)).unwrap();
        let started = std::time::Instant::now();
        let tables = super::parse_tables(&html);
        let elapsed = started.elapsed();
        let names = super::table_names(&tables);
        eprintln!(
            "{} bytes, {} tables, parsed in {elapsed:?}",
            html.len(),
            tables.len()
        );
        let mut widest = 0;
        for (t, name) in tables.iter().zip(&names) {
            let dt = super::to_data_table(t);
            widest = widest.max(dt.columns.len());
            eprintln!(
                "  {name}: {} cols x {} rows",
                dt.columns.len(),
                dt.row_count()
            );
        }
        assert!(!tables.is_empty(), "{path} has no table Octa can see");
        assert!(widest > 1, "every table came out one column wide");
        // A page is hundreds of kilobytes at most; anything near a second
        // means the walk has gone quadratic.
        assert!(elapsed.as_secs() < 5, "parsing took {elapsed:?}");
    }
}
