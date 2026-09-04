//! HTML table reader: the surfaces the GUI and the CLI actually go through.
//! The parsing rules themselves are unit-tested in the reader.

use octa::data::CellValue;
use octa::formats::html_reader::HtmlReader;
use octa::formats::{FormatReader, FormatRegistry};

/// A page shaped like the ones people open: a layout table wrapping the real
/// one, a caption, a spanning header, and a second table further down.
const PAGE: &str = r#"<!doctype html>
<html><body>
<table class="layout"><tr><td>
  <table>
    <caption>Population by country</caption>
    <tr><th>Country</th><th colspan="2">People</th></tr>
    <tr><td>India</td><td>1438069596</td><td>+0.89%</td></tr>
    <tr><td>China</td><td>1422584933</td><td>&minus;0.18%</td></tr>
  </table>
</td></tr></table>
<p>Some prose.</p>
<table><tr><th>note</th></tr><tr><td>see also</td></tr></table>
</body></html>"#;

fn write_page(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("page.html");
    std::fs::write(&path, body).unwrap();
    path
}

/// `.html` must reach the HTML reader, not the text reader that also claims
/// the extension for "Reopen as".
#[test]
fn registry_opens_html_as_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_page(dir.path(), PAGE);
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&path).expect("a reader for .html");
    assert_eq!(reader.name(), "HTML");
    assert!(reader.opens_all_tables(), "every table should open");
}

/// The layout table wrapping the real one must not become a table of its own,
/// and the caption names the one that matters.
#[test]
fn tables_are_listed_by_caption() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_page(dir.path(), PAGE);
    let tables = HtmlReader
        .list_tables(&path)
        .unwrap()
        .expect("a multi-table listing");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["Population by country", "Table 2"]);
    assert_eq!(tables[0].row_count, Some(2));
    assert_eq!(tables[1].row_count, Some(1));
}

/// Reading by name is what the CLI's `--table` and the GUI's picker do.
#[test]
fn a_named_table_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_page(dir.path(), PAGE);
    let t = HtmlReader.read_table(&path, "Table 2").unwrap();
    assert_eq!(t.columns[0].name, "note");
    assert_eq!(t.get(0, 0), Some(&CellValue::String("see also".into())));

    let err = HtmlReader.read_table(&path, "Nope").unwrap_err();
    assert!(format!("{err:#}").contains("no table called 'Nope'"));
}

/// `read_file` takes the first table, with its header row and its spans
/// expanded, and entities decoded the way a browser decodes them.
#[test]
fn the_first_table_is_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_page(dir.path(), PAGE);
    let t = HtmlReader.read_file(&path).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    // `colspan="2"` repeats the header, which is what keeps the grid square.
    assert_eq!(names, vec!["Country", "People", "People"]);
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.get(0, 0), Some(&CellValue::String("India".into())));
    // &minus; is a real character once parsed, not the entity text.
    let change = t.get(1, 2).map(|c| c.to_string()).unwrap_or_default();
    assert!(
        change.starts_with('\u{2212}') || change.starts_with('-'),
        "{change}"
    );
    assert_eq!(t.format_name.as_deref(), Some("HTML"));
}

/// A page with no table at all says so, and says where to look instead,
/// rather than opening an empty grid.
#[test]
fn a_page_without_tables_explains_itself() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_page(
        dir.path(),
        "<html><body><p>no tables here</p></body></html>",
    );
    let err = HtmlReader.read_file(&path).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("no <table>"), "{text}");
    assert!(text.contains("Reopen as"), "{text}");
    assert!(HtmlReader.list_tables(&path).unwrap().is_none());
}
