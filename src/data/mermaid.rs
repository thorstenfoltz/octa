//! Mermaid diagrams drawn as pictures, for the Markdown preview.
//!
//! The `mermaid-rs-renderer` crate turns the diagram's text into an SVG and
//! [`binary_kind::decode_image`] draws that like any other SVG: capped in
//! size, and never reading a file or URL the SVG names. Diagram text comes
//! out of a file, so it is untrusted like the rest of it.

use super::binary_kind;

/// A drawing this small is the renderer's answer to text it found no diagram
/// in: it returns an empty canvas, not an error.
const EMPTY_SIDE: u32 = 16;

/// The diagram as RGBA pixels, in the renderer's dark or light colours.
// ponytail: drawn once at the SVG's own size, so it softens on a zoomed or
// high-DPI window. Draw at the window's scale if that is ever reported.
pub fn render(source: &str, dark: bool) -> anyhow::Result<image::RgbaImage> {
    use mermaid_rs_renderer::{RenderOptions, Theme, render_with_options};
    let theme = if dark {
        Theme::dark()
    } else {
        Theme::mermaid_default()
    };
    let options = RenderOptions {
        theme,
        ..RenderOptions::default()
    };
    let svg = render_with_options(source, options)?;
    let picture = binary_kind::decode_image(svg.as_bytes())?;
    if picture.width() <= EMPTY_SIDE && picture.height() <= EMPTY_SIDE {
        anyhow::bail!("no diagram found in the text");
    }
    Ok(picture)
}

#[cfg(test)]
#[path = "mermaid_tests.rs"]
mod tests;
