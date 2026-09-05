//! Unit tests for [`mod`](mod). The PDF bytes themselves are checked only for
//! the things a reader depends on (a header, one page object per page); the
//! interesting logic is the pagination, which is pure.

use super::*;
use crate::data::{CellValue, ColumnInfo};

fn table(rows: usize, cols: usize) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = (0..cols)
        .map(|c| ColumnInfo {
            name: format!("col{c}"),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| CellValue::String(format!("r{r}c{c}")))
                .collect()
        })
        .collect();
    t
}

fn view<'a>(t: &'a DataTable, rows: &'a [usize], cols: &'a [usize]) -> PdfTable<'a> {
    PdfTable {
        table: t,
        rows,
        cols,
        frozen: 0,
        rules: &[],
    }
}

#[test]
fn a_column_too_wide_for_the_page_still_gets_a_page() {
    // 500 pt of column against 300 pt of paper: one block, not an empty
    // result and not an infinite loop.
    let blocks = column_blocks(&[500.0], 0, 300.0);
    assert_eq!(blocks, vec![vec![0]]);
}

#[test]
fn frozen_columns_repeat_on_every_block() {
    let widths = [40.0, 40.0, 100.0, 100.0, 100.0];
    let blocks = column_blocks(&widths, 2, 250.0);
    assert!(blocks.len() > 1, "the table has to split to prove anything");
    for block in &blocks {
        assert_eq!(block[0], 0);
        assert_eq!(block[1], 1);
    }
    // Every non-frozen column appears exactly once.
    let mut seen: Vec<usize> = blocks.iter().flat_map(|b| b[2..].iter().copied()).collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![2, 3, 4]);
}

#[test]
fn everything_frozen_is_one_block() {
    assert_eq!(column_blocks(&[80.0, 80.0], 2, 100.0), vec![vec![0, 1]]);
    assert_eq!(column_blocks(&[80.0, 80.0], 9, 100.0), vec![vec![0, 1]]);
}

#[test]
fn a_page_always_holds_at_least_one_row() {
    assert_eq!(rows_per_page(0.0), 1);
    assert!(rows_per_page(400.0) > 10);
}

#[test]
fn pages_go_down_then_across_and_the_count_matches() {
    let t = table(200, 12);
    let rows: Vec<usize> = (0..200).collect();
    let cols: Vec<usize> = (0..12).collect();
    let v = view(&t, &rows, &cols);
    let opts = PdfOptions {
        title: "sales.csv".to_string(),
        ..Default::default()
    };
    let svgs = page_svgs(&v, &opts);
    assert_eq!(svgs.len(), page_count(&v, &opts));
    assert!(svgs.len() > 1);
    // Every page carries the repeated header row and the footer's page number.
    for (i, svg) in svgs.iter().enumerate() {
        assert!(svg.contains("col0"), "page {} lost the header", i + 1);
        assert!(svg.contains(&format!("page {} of {}", i + 1, svgs.len())));
    }
    // The subtitle line is a first-page thing only.
    let opts = PdfOptions {
        subtitle: Some("filter: amount > 100".to_string()),
        ..opts
    };
    let svgs = page_svgs(&v, &opts);
    assert!(svgs[0].contains("filter: amount &gt; 100"));
    assert!(!svgs[1].contains("filter: amount &gt; 100"));
}

#[test]
fn an_empty_view_still_prints_its_header() {
    let t = table(0, 3);
    let cols: Vec<usize> = (0..3).collect();
    let v = view(&t, &[], &cols);
    let svgs = page_svgs(&v, &PdfOptions::default());
    assert_eq!(svgs.len(), 1);
    assert!(svgs[0].contains("col2"));
    assert!(svgs[0].contains("no rows"));
}

#[test]
fn cell_text_is_escaped_and_flattened() {
    let mut t = table(1, 1);
    t.rows[0][0] = CellValue::String("a <b>\nsecond line".to_string());
    let v = view(&t, &[0], &[0]);
    let svg = &page_svgs(&v, &PdfOptions::default())[0];
    assert!(svg.contains("a &lt;b&gt; second line"), "{svg}");
}

#[test]
fn marked_cells_carry_their_colour() {
    use crate::data::{MarkColor, MarkKey};
    let mut t = table(2, 2);
    t.set_mark(MarkKey::Row(1), MarkColor::Green);
    let v = view(&t, &[0, 1], &[0, 1]);
    let svg = &page_svgs(&v, &PdfOptions::default())[0];
    assert!(svg.contains("#22c55e"));
}

#[test]
fn landscape_swaps_the_page_and_fits_more_columns() {
    let portrait = PdfOptions::default();
    let landscape = PdfOptions {
        landscape: true,
        ..Default::default()
    };
    assert_eq!(portrait.page_points(), (595.0, 842.0));
    assert_eq!(landscape.page_points(), (842.0, 595.0));

    let t = table(10, 10);
    let rows: Vec<usize> = (0..10).collect();
    let cols: Vec<usize> = (0..10).collect();
    let v = view(&t, &rows, &cols);
    assert!(page_count(&v, &landscape) <= page_count(&v, &portrait));
}

#[test]
fn the_pdf_is_a_pdf_with_one_page_object_per_page() {
    let t = table(120, 4);
    let rows: Vec<usize> = (0..120).collect();
    let cols: Vec<usize> = (0..4).collect();
    let v = view(&t, &rows, &cols);
    let opts = PdfOptions {
        title: "sales.csv".to_string(),
        ..Default::default()
    };
    let bytes = table_to_pdf(&v, &opts).expect("render");
    assert!(bytes.starts_with(b"%PDF-"));
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text.matches("/Type /Page\n").count(), page_count(&v, &opts));
}
