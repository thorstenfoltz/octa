//! Unit tests for [`binary_kind`](super).

use super::*;
use std::io::Write as _;

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn zip_with(name: &str, body: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(body).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

/// A one-page PDF with one line of Helvetica text.
fn text_pdf(text: &str) -> Vec<u8> {
    use pdf_writer::{Content, Finish as _, Name, Pdf, Rect, Ref, Str};
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
    c.begin_text()
        .set_font(Name(b"F1"), 12.0)
        .next_line(72.0, 720.0)
        .show(Str(text.as_bytes()))
        .end_text();
    pdf.stream(contents, &c.finish());
    pdf.finish()
}

#[test]
fn each_kind_is_told_by_its_first_bytes() {
    assert_eq!(detect(b"\x89PNG\r\n\x1a\n...."), BinaryKind::Png);
    assert_eq!(detect(&[0xFF, 0xD8, 0xFF, 0xE0]), BinaryKind::Jpeg);
    assert_eq!(detect(b"GIF89a..."), BinaryKind::Gif);
    assert_eq!(detect(b"RIFF\x10\x00\x00\x00WEBPVP8 "), BinaryKind::Webp);
    assert_eq!(detect(&text_pdf("x")), BinaryKind::Pdf);
    assert_eq!(detect(&zip_with("a.txt", b"hi")), BinaryKind::Zip);
    assert_eq!(detect(&gzip(b"hello")), BinaryKind::Gzip);
    assert_eq!(detect(b"  {\"a\": [1, 2]}"), BinaryKind::Json);
    assert_eq!(detect(b"\x00\x01\x02"), BinaryKind::Unknown);
}

#[test]
fn near_misses_are_unknown() {
    assert_eq!(detect(b"%PD"), BinaryKind::Unknown);
    assert_eq!(detect(b"PK"), BinaryKind::Unknown);
    assert_eq!(detect(b"{"), BinaryKind::Unknown);
    assert_eq!(detect(b"RIFF\x10\x00\x00\x00WAVE"), BinaryKind::Unknown);
}

#[test]
fn the_hex_dump_reads_like_a_hex_editor_and_stops_at_max() {
    let dump = hex_dump(b"\x89PNG\r\n\x1a\nABCDEFGHIJ", 64);
    let first = dump.lines().next().unwrap();
    assert_eq!(
        first,
        "00000000  89 50 4e 47 0d 0a 1a 0a  41 42 43 44 45 46 47 48  |.PNG....ABCDEFGH|"
    );
    assert!(dump.lines().nth(1).unwrap().starts_with("00000010  49 4a "));
    assert_eq!(hex_dump(&[0u8; 100], 32).lines().count(), 2);
}

#[test]
fn text_is_utf8_without_control_characters() {
    assert_eq!(as_text(b"line\none\ttab"), Some("line\none\ttab"));
    assert_eq!(as_text(b"bell\x07"), None);
    assert_eq!(as_text(&[0xFF, 0xFE]), None);
}

#[test]
fn gunzip_stops_at_the_cap_and_says_so() {
    let packed = gzip(&[b'a'; 10_000]);
    let (all, cut) = gunzip_capped(&packed, 1_000_000).unwrap();
    assert_eq!((all.len(), cut), (10_000, false));
    let (part, cut) = gunzip_capped(&packed, 100).unwrap();
    assert_eq!((part.len(), cut), (100, true));
}

#[test]
fn a_pdf_gives_its_page_count_and_text() {
    let (pages, text) = pdf_text(&text_pdf("Invoice 42")).unwrap();
    assert_eq!(pages, 1);
    assert!(text.contains("Invoice 42"), "{text:?}");
    assert!(pdf_text(b"%PDF-broken").is_err());
}

#[test]
fn a_picture_decodes_and_a_truncated_one_is_an_error() {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::new(3, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    assert_eq!(decode_image(&png).unwrap().dimensions(), (3, 2));
    assert_eq!(detect(&png[..20]), BinaryKind::Png);
    assert!(decode_image(&png[..20]).is_err());
}

#[test]
fn a_zip_in_memory_lists_its_files() {
    let bytes = zip_with("report.csv", b"a,b\n1,2\n");
    let entries =
        crate::formats::archive_reader::list_zip_entries_from(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "report.csv");
    assert_eq!(entries[0].size_bytes, 8);
}

/// A PNG whose IHDR declares a `width` x `height` RGBA canvas. The IDAT chunk
/// (the compressed pixels) is written only when `idat` is non-empty.
fn png_declaring(width: u32, height: u32, idat: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut png, b"IHDR", &ihdr);
    if !idat.is_empty() {
        chunk(&mut png, b"IDAT", idat);
    }
    chunk(&mut png, b"IEND", &[]);
    png
}

#[test]
fn decode_image_refuses_a_huge_declared_canvas() {
    let bytes = png_declaring(100_000, 100_000, &[]);
    assert_eq!(detect(&bytes), BinaryKind::Png);
    assert!(decode_image(&bytes).is_err());

    // A small byte count that the old byte-only limit accepted: a 16_385 x 1
    // strip of zero pixels, fully valid, but over the side cap.
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&vec![0u8; 1 + 16_385 * 4]).unwrap();
    let strip = png_declaring(16_385, 1, &enc.finish().unwrap());
    assert!(decode_image(&strip).is_err());
}

const SVG: &str = r##"<?xml version="1.0" encoding="utf-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<!-- made by hand -->
<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2">
  <rect width="4" height="2" fill="#ff0000"/>
</svg>"##;

#[test]
fn svg_is_told_by_its_first_element() {
    assert_eq!(detect(SVG.as_bytes()), BinaryKind::Svg);
    assert_eq!(detect(b"  <svg/>"), BinaryKind::Svg);
    assert!(BinaryKind::Svg.is_image());
    // A page that merely contains an SVG is not one.
    assert_eq!(
        detect(b"<!DOCTYPE html><html><body><svg/></body></html>"),
        BinaryKind::Unknown
    );
}

#[test]
fn an_svg_decodes_at_its_own_size() {
    let rgba = decode_image(SVG.as_bytes()).unwrap();
    assert_eq!(rgba.dimensions(), (4, 2));
    assert_eq!(rgba.get_pixel(1, 1).0, [255, 0, 0, 255]);
}

#[test]
fn an_svg_never_reads_a_file_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("secret.png");
    image::RgbaImage::from_pixel(4, 2, image::Rgba([0, 0, 255, 255]))
        .save(&png)
        .unwrap();
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="2"><image width="4" height="2" xlink:href="{}"/></svg>"#,
        png.display()
    );
    let rgba = decode_image(svg.as_bytes()).unwrap();
    assert_eq!(rgba.get_pixel(1, 1).0[3], 0, "the named file was drawn");
}
