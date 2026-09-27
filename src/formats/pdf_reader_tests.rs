//! Unit tests for the PDF reader. Included via `#[path]`.

use super::*;
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

/// Glyphs for `text` starting at `x` on baseline `y`, 10pt, 5pt per char.
fn word(x: f64, y: f64, text: &str) -> Vec<Glyph> {
    text.chars()
        .enumerate()
        .map(|(i, c)| Glyph {
            x: x + 5.0 * i as f64,
            y,
            width: 5.0,
            size: 10.0,
            text: c.to_string(),
        })
        .collect()
}

#[test]
fn aligned_columns_become_one_table_and_prose_does_not() {
    let mut g = Vec::new();
    g.extend(word(50.0, 700.0, "Quarterly report for the region"));
    for (y, cells) in [
        (660.0, ["Name", "Qty", "Price"]),
        (645.0, ["Anna", "3", "9.50"]),
        (630.0, ["Bob Smith", "12", "10.00"]),
    ] {
        g.extend(word(50.0, y, cells[0]));
        g.extend(word(150.0, y, cells[1]));
        g.extend(word(220.0, y, cells[2]));
    }
    let tables = detect_tables(&g);
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(
        tables[0],
        vec![
            vec!["Name", "Qty", "Price"],
            vec!["Anna", "3", "9.50"],
            vec!["Bob Smith", "12", "10.00"],
        ]
    );
}

#[test]
fn a_single_column_of_text_is_not_a_table() {
    let mut g = Vec::new();
    for (i, line) in ["First line of prose", "second line", "third"]
        .iter()
        .enumerate()
    {
        g.extend(word(50.0, 700.0 - 14.0 * i as f64, line));
    }
    assert!(detect_tables(&g).is_empty());
}

#[test]
fn a_big_vertical_gap_splits_two_tables() {
    let mut g = Vec::new();
    for y in [700.0, 686.0, 400.0, 386.0] {
        g.extend(word(50.0, y, "a"));
        g.extend(word(150.0, y, "b"));
    }
    assert_eq!(detect_tables(&g).len(), 2);
}

/// A one-page PDF with text drawn at the given positions in Helvetica.
fn text_pdf(lines: &[(f32, f32, &str)]) -> Vec<u8> {
    let (catalog, tree, page, font, contents) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
    );
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
        .parent(tree)
        .contents(contents);
    p.resources().fonts().pair(Name(b"F1"), font);
    p.finish();
    pdf.type1_font(font).base_font(Name(b"Helvetica"));
    let mut c = Content::new();
    for (x, y, s) in lines {
        c.begin_text()
            .set_font(Name(b"F1"), 10.0)
            .next_line(*x, *y)
            .show(Str(s.as_bytes()))
            .end_text();
    }
    pdf.stream(contents, &c.finish());
    pdf.finish()
}

/// A one-page PDF that is only an image, like a scan.
fn image_pdf() -> Vec<u8> {
    let (catalog, tree, page, image, contents) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
    );
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
        .parent(tree)
        .contents(contents);
    p.resources().x_objects().pair(Name(b"Im1"), image);
    p.finish();
    let pixels = vec![128u8; 4];
    let mut img = pdf.image_xobject(image, &pixels);
    img.width(2).height(2).bits_per_component(8);
    img.color_space().device_gray();
    img.finish();
    let mut c = Content::new();
    c.save_state()
        .transform([500.0, 0.0, 0.0, 700.0, 40.0, 60.0])
        .x_object(Name(b"Im1"))
        .restore_state();
    pdf.stream(contents, &c.finish());
    pdf.finish()
}

fn write_tmp(bytes: &[u8]) -> tempfile::NamedTempFile {
    let f = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    std::fs::write(f.path(), bytes).unwrap();
    f
}

#[test]
fn a_real_pdf_text_table_reads_back_as_a_table() {
    let mut lines = vec![(50.0, 780.0, "Invoice 2026-09")];
    for (y, [a, b, c]) in [
        (740.0, ["Item", "Qty", "Amount"]),
        (725.0, ["Paper", "2", "4.00"]),
        (710.0, ["Ink", "1", "19.90"]),
    ] {
        lines.push((50.0, y, a));
        lines.push((200.0, y, b));
        lines.push((300.0, y, c));
    }
    let f = write_tmp(&text_pdf(&lines));
    let t = PdfReader.read_file(f.path()).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Item", "Qty", "Amount"]);
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.get(1, 2).unwrap().to_string(), "19.90");
    let list = PdfReader.list_tables(f.path()).unwrap().unwrap();
    assert_eq!(list[0].name, "Page 1, table 1");
    assert!(scan_note(f.path()).is_none());
}

#[test]
fn a_scanned_page_is_named_as_a_picture_not_reported_as_empty() {
    let f = write_tmp(&image_pdf());
    let err = PdfReader.read_file(f.path()).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("scanned images"), "{msg}");
    assert!(msg.contains("OCR"), "{msg}");
}

#[test]
fn text_without_columns_says_so() {
    let f = write_tmp(&text_pdf(&[
        (50.0, 780.0, "Just a letter."),
        (50.0, 765.0, "Kind regards"),
    ]));
    let msg = format!("{:#}", PdfReader.read_file(f.path()).unwrap_err());
    assert!(msg.contains("No table found"), "{msg}");
}

/// Writes `samples/documents/invoice.pdf` and `scanned.pdf`. Run by hand:
/// `cargo test --lib write_pdf_samples -- --ignored`.
#[test]
#[ignore]
fn write_pdf_samples() {
    let mut lines = vec![(50.0, 790.0, "Coffee Roasters Ltd - Invoice 2026-09")];
    let items = [
        ["Item", "Qty", "Unit price", "Amount"],
        ["Espresso beans 1kg", "3", "18.50", "55.50"],
        ["Filter papers", "10", "2.20", "22.00"],
        ["Milk jug", "1", "14.90", "14.90"],
        ["Grinder burr set", "1", "39.00", "39.00"],
    ];
    for (i, row) in items.iter().enumerate() {
        let y = 740.0 - 15.0 * i as f32;
        for (x, cell) in [50.0, 230.0, 300.0, 400.0].iter().zip(row) {
            lines.push((*x, y, cell));
        }
    }
    lines.push((50.0, 620.0, "Payments received"));
    for (i, row) in [
        ["Date", "Method", "Amount"],
        ["2026-09-02", "Card", "80.00"],
        ["2026-09-15", "Transfer", "51.40"],
    ]
    .iter()
    .enumerate()
    {
        let y = 590.0 - 15.0 * i as f32;
        for (x, cell) in [50.0, 170.0, 300.0].iter().zip(row) {
            lines.push((*x, y, cell));
        }
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/documents");
    std::fs::write(dir.join("invoice.pdf"), text_pdf(&lines)).unwrap();
    std::fs::write(dir.join("scanned.pdf"), image_pdf()).unwrap();
    let t = PdfReader
        .list_tables(&dir.join("invoice.pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(t.len(), 2);
}
