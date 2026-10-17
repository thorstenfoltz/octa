//! What a binary cell holds, told by its first bytes (the "magic number"
//! every format starts with), never by a file or column name; and the small
//! readers the binary viewer needs for each kind. Pure: no egui.

use std::fmt::Write as _;
use std::io::Read as _;

/// Largest side a picture may declare: the cell is untrusted, so a tiny file
/// cannot ask for a huge canvas.
const MAX_IMAGE_SIDE: u32 = 16_384;
/// Largest decode buffer a picture may need, in bytes.
const MAX_IMAGE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
    Pdf,
    Zip,
    Gzip,
    Json,
    Unknown,
}

impl BinaryKind {
    /// The file extension a saved copy gets, so the system opens it right.
    pub fn extension(self) -> &'static str {
        match self {
            BinaryKind::Png => "png",
            BinaryKind::Jpeg => "jpg",
            BinaryKind::Gif => "gif",
            BinaryKind::Webp => "webp",
            BinaryKind::Svg => "svg",
            BinaryKind::Pdf => "pdf",
            BinaryKind::Zip => "zip",
            BinaryKind::Gzip => "gz",
            BinaryKind::Json => "json",
            BinaryKind::Unknown => "bin",
        }
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            BinaryKind::Png => "binview.kind_png",
            BinaryKind::Jpeg => "binview.kind_jpeg",
            BinaryKind::Gif => "binview.kind_gif",
            BinaryKind::Webp => "binview.kind_webp",
            BinaryKind::Svg => "binview.kind_svg",
            BinaryKind::Pdf => "binview.kind_pdf",
            BinaryKind::Zip => "binview.kind_zip",
            BinaryKind::Gzip => "binview.kind_gzip",
            BinaryKind::Json => "binview.kind_json",
            BinaryKind::Unknown => "binview.kind_unknown",
        }
    }

    pub fn is_image(self) -> bool {
        matches!(
            self,
            BinaryKind::Png
                | BinaryKind::Jpeg
                | BinaryKind::Gif
                | BinaryKind::Webp
                | BinaryKind::Svg
        )
    }
}

pub fn detect(bytes: &[u8]) -> BinaryKind {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return BinaryKind::Png;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return BinaryKind::Jpeg;
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return BinaryKind::Gif;
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return BinaryKind::Webp;
    }
    if is_svg(bytes) {
        return BinaryKind::Svg;
    }
    if bytes.starts_with(b"%PDF-") {
        return BinaryKind::Pdf;
    }
    // A local file header, or the end record of an empty archive.
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return BinaryKind::Zip;
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return BinaryKind::Gzip;
    }
    if is_json(bytes) {
        return BinaryKind::Json;
    }
    BinaryKind::Unknown
}

/// SVG has no magic number: its first element, after any XML declaration,
/// comments and doctype, is `<svg`.
fn is_svg(bytes: &[u8]) -> bool {
    let mut rest = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    loop {
        rest = rest.trim_ascii_start();
        let close: &[u8] = if rest.starts_with(b"<?") {
            b"?>"
        } else if rest.starts_with(b"<!--") {
            b"-->"
        } else if rest.starts_with(b"<!") {
            b">"
        } else {
            return rest.starts_with(b"<svg");
        };
        match rest.windows(close.len()).position(|w| w == close) {
            Some(i) => rest = &rest[i + close.len()..],
            None => return false,
        }
    }
}

/// An object or array that parses. A bare number or string is JSON too, but
/// calling `42` "JSON" would only confuse.
fn is_json(bytes: &[u8]) -> bool {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let first = body.iter().find(|b| !b.is_ascii_whitespace());
    matches!(first, Some(b'{' | b'['))
        && serde_json::from_slice::<serde::de::IgnoredAny>(body).is_ok()
}

/// Offset, 16 bytes in hex (a gap after 8) and the printable characters, one
/// line per 16 bytes, for at most the first `max` bytes.
pub fn hex_dump(bytes: &[u8], max: usize) -> String {
    let shown = &bytes[..bytes.len().min(max)];
    let mut out = String::with_capacity(shown.len() * 4 + 16);
    for (i, chunk) in shown.chunks(16).enumerate() {
        let _ = write!(out, "{:08x}  ", i * 16);
        for j in 0..16 {
            match chunk.get(j) {
                Some(b) => {
                    let _ = write!(out, "{b:02x} ");
                }
                None => out.push_str("   "),
            }
            if j == 7 {
                out.push(' ');
            }
        }
        out.push(' ');
        out.push('|');
        for &b in chunk {
            out.push(if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            });
        }
        out.push_str("|\n");
    }
    out
}

/// The bytes as text, when they are UTF-8 with no control characters other
/// than line breaks and tabs.
pub fn as_text(bytes: &[u8]) -> Option<&str> {
    let s = std::str::from_utf8(bytes).ok()?;
    (!s.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
    .then_some(s)
}

/// Unpack gzip, stopping at `cap` bytes so a cell built as a zip bomb cannot
/// fill memory. The flag says whether it stopped early.
pub fn gunzip_capped(bytes: &[u8], cap: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(cap.saturating_add(1))
        .read_to_end(&mut out)?;
    let cut = out.len() as u64 > cap;
    if cut {
        out.truncate(usize::try_from(cap).unwrap_or(usize::MAX));
    }
    Ok((out, cut))
}

/// A PDF's page count and text. `pdf-extract` decodes fonts with third-party
/// code, so a panic in it becomes an error here, as in the PDF reader.
pub fn pdf_text(bytes: &[u8]) -> anyhow::Result<(usize, String)> {
    let pages = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes))
        .map_err(|_| anyhow::anyhow!(crate::i18n::t("binview.pdf_unreadable")))?
        .map_err(|e| anyhow::anyhow!("{}: {e}", crate::i18n::t("binview.pdf_unreadable")))?;
    Ok((pages.len(), pages.join("\n\n")))
}

/// The picture in a cell, as RGBA pixels. A cell that starts like a PNG but
/// is cut short fails here, and the viewer falls back to the hex dump.
/// The cell is untrusted, so the canvas and the decode buffer are capped.
pub fn decode_image(bytes: &[u8]) -> anyhow::Result<image::RgbaImage> {
    if is_svg(bytes) {
        return render_svg(bytes);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_BYTES);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits);
    Ok(reader.decode()?.to_rgba8())
}

/// An SVG drawn at its own size, shrunk to the same caps as any picture.
/// The cell is untrusted, so a file or URL it names is never read; text is
/// drawn with the system fonts, scanned once and only for an SVG that has any.
fn render_svg(bytes: &[u8]) -> anyhow::Result<image::RgbaImage> {
    use resvg::{tiny_skia, usvg};
    static FONTS: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    let mut opt = usvg::Options::default();
    opt.image_href_resolver.resolve_string = Box::new(|_, _| None);
    if bytes.windows(5).any(|w| w == b"<text") {
        opt.fontdb = FONTS
            .get_or_init(|| {
                let mut db = usvg::fontdb::Database::new();
                db.load_system_fonts();
                std::sync::Arc::new(db)
            })
            .clone();
    }
    let tree = usvg::Tree::from_data(bytes, &opt)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let max_pixels = (MAX_IMAGE_BYTES / 4) as f32;
    let scale = (MAX_IMAGE_SIDE as f32 / w.max(h))
        .min((max_pixels / (w * h)).sqrt())
        .min(1.0);
    let (pw, ph) = (
        (w * scale).ceil().max(1.0) as u32,
        (h * scale).ceil().max(1.0) as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| anyhow::anyhow!("cannot draw a {pw} x {ph} picture"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(pw, ph, rgba)
        .ok_or_else(|| anyhow::anyhow!("cannot draw a {pw} x {ph} picture"))
}

#[cfg(test)]
#[path = "binary_kind_tests.rs"]
mod tests;
