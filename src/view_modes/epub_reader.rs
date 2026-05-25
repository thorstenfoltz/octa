//! EPUB reading view. Renders one chapter at a time, Markdown-formatted,
//! using the same pulldown-cmark backed pipeline as the Markdown view.
//!
//! ## Where the data comes from
//!
//! `apply_loaded_table` calls `octa::formats::epub_reader::read_with_extras`
//! when it sees `format_name == "EPUB"` and stashes the rendered chapter
//! Markdown + image bytes on the tab. This view consumes those — it does
//! not re-parse the EPUB on every paint.
//!
//! ## Image handling (v1)
//!
//! Inline image rendering inside pulldown's event stream is non-trivial
//! (the markdown renderer has to weave them into paragraph layout).
//! For the first release we render the chapter text via `render_pulldown`
//! and append a thumbnail strip beneath it listing the images that appear
//! in this chapter. Each thumbnail is rendered at most 200 px tall.
//!
//! Decoding happens once per image, on first paint, into
//! `tab.epub_image_textures`. The cache survives chapter switches so
//! flipping pages doesn't re-decode.

use eframe::egui;

use crate::app::state::TabState;

/// Entry point invoked from `central_panel::render_central_panel` when the
/// active tab's `view_mode == ViewMode::EpubReader`.
pub fn render_epub_view(ctx: &egui::Context, ui: &mut egui::Ui, tab: &mut TabState) {
    if tab.epub_chapters_md.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "EPUB has no readable chapters. Try switching to the Table view.",
                )
                .weak(),
            );
        });
        return;
    }

    // Clamp the active chapter in case the tab was loaded with a stale
    // index (or the chapter count shrank for any reason).
    if tab.epub_active_chapter >= tab.epub_chapters_md.len() {
        tab.epub_active_chapter = tab.epub_chapters_md.len() - 1;
    }

    draw_chapter_toolbar(ui, tab);
    ui.separator();

    let chapter_idx = tab.epub_active_chapter;
    let chapter_md = tab.epub_chapters_md[chapter_idx].clone();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // Cap the reading column so long lines don't sprawl across
            // ultra-wide windows. Matches the cap the Markdown view uses
            // (see `clamp(200.0, 900.0)` in `markdown.rs::render_preview_pane`).
            let available = ui.available_width();
            let target = available.clamp(200.0, 900.0);
            ui.allocate_ui_with_layout(
                egui::vec2(target, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    super::markdown::render_pulldown(ui, &chapter_md);
                    render_chapter_images(ctx, ui, tab, chapter_idx);
                },
            );
        });
}

fn draw_chapter_toolbar(ui: &mut egui::Ui, tab: &mut TabState) {
    let total = tab.epub_chapters_md.len();
    ui.horizontal(|ui| {
        // Book title (best-effort) goes first so the user knows what
        // they're reading even after they navigate away from chapter 1.
        if let Some(ref title) = tab.epub_title {
            ui.label(egui::RichText::new(title).strong().size(14.0));
            ui.separator();
        }

        let prev_enabled = tab.epub_active_chapter > 0;
        if ui
            .add_enabled(prev_enabled, egui::Button::new("◀ Previous"))
            .clicked()
        {
            tab.epub_active_chapter -= 1;
        }

        // Chapter picker — combo of every chapter label. Selecting a
        // chapter jumps directly to it.
        let selected_label = tab
            .epub_chapter_titles
            .get(tab.epub_active_chapter)
            .cloned()
            .unwrap_or_else(|| format!("Chapter {}", tab.epub_active_chapter + 1));
        egui::ComboBox::from_id_salt("epub_chapter_combo")
            .selected_text(format!(
                "{} ({}/{})",
                selected_label,
                tab.epub_active_chapter + 1,
                total
            ))
            .width(280.0)
            .show_ui(ui, |ui| {
                for (i, title) in tab.epub_chapter_titles.iter().enumerate() {
                    let is_selected = i == tab.epub_active_chapter;
                    let label = format!("{}. {}", i + 1, title);
                    if ui.selectable_label(is_selected, label).clicked() {
                        tab.epub_active_chapter = i;
                    }
                }
            });

        let next_enabled = tab.epub_active_chapter + 1 < total;
        if ui
            .add_enabled(next_enabled, egui::Button::new("Next ▶"))
            .clicked()
        {
            tab.epub_active_chapter += 1;
        }
    });
}

/// Find every image href referenced by the current chapter's Markdown and
/// render them as a thumbnail strip beneath the chapter text. References
/// that don't resolve against `epub_image_bytes` are silently skipped —
/// the alt text already appears via `render_pulldown`'s text-event flow.
fn render_chapter_images(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &mut TabState,
    chapter_idx: usize,
) {
    let chapter_md = match tab.epub_chapters_md.get(chapter_idx) {
        Some(s) => s,
        None => return,
    };
    let refs = extract_image_refs(chapter_md);
    if refs.is_empty() {
        return;
    }
    let resolved: Vec<String> = refs
        .into_iter()
        .filter_map(|href| resolve_image_href(&href, &tab.epub_image_bytes))
        .collect();
    if resolved.is_empty() {
        return;
    }

    ui.add_space(12.0);
    ui.separator();
    ui.label(
        egui::RichText::new(format!("Images ({})", resolved.len()))
            .strong()
            .size(12.0),
    );
    ui.add_space(4.0);

    // Lay images out in a horizontal wrap. Each thumbnail is capped at
    // 200 px on the long axis.
    ui.horizontal_wrapped(|ui| {
        for href in resolved {
            let tex = ensure_texture(ctx, tab, &href);
            if let Some(tex) = tex {
                let size = tex.size_vec2();
                let scale = (200.0 / size.x.max(size.y)).min(1.0);
                let display = size * scale;
                ui.add(egui::Image::new(&tex).fit_to_exact_size(display));
            }
        }
    });
}

/// Decode the image bytes for `href` into an egui texture, caching the
/// result on `tab.epub_image_textures`. Returns `None` when the image
/// can't be decoded (corrupt bytes, unsupported format, etc.) so the
/// caller skips it instead of panicking.
fn ensure_texture(
    ctx: &egui::Context,
    tab: &mut TabState,
    href: &str,
) -> Option<egui::TextureHandle> {
    if let Some(existing) = tab.epub_image_textures.get(href) {
        return Some(existing.clone());
    }
    let bytes = tab.epub_image_bytes.get(href)?;
    let img = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let pixels: Vec<u8> = img.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
    let handle = ctx.load_texture(
        format!("epub_image_{href}"),
        color,
        egui::TextureOptions::LINEAR,
    );
    tab.epub_image_textures
        .insert(href.to_string(), handle.clone());
    Some(handle)
}

/// Walk a markdown string and pull every `![...](href)` href into a list.
/// Uses `pulldown_cmark` so we don't re-implement the parser (and so we
/// match exactly how the rest of the chapter is rendered).
fn extract_image_refs(md: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut out: Vec<String> = Vec::new();
    for event in Parser::new(md) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            out.push(dest_url.to_string());
        }
    }
    out
}

/// Resolve an image href reference (as it appears in the chapter Markdown)
/// against the EPUB's image-bytes map (keyed by manifest href). Tries an
/// exact match first, then a basename match. Returns the *map key* that
/// matched so the caller can re-look up the texture cache by the same
/// key the bytes are stored under.
fn resolve_image_href(
    href: &str,
    image_bytes: &std::collections::HashMap<String, Vec<u8>>,
) -> Option<String> {
    // Exact match.
    if image_bytes.contains_key(href) {
        return Some(href.to_string());
    }
    // Some EPUBs prefix manifest hrefs with `/` (rbook normalises them).
    let with_slash = format!("/{}", href.trim_start_matches('/'));
    if image_bytes.contains_key(&with_slash) {
        return Some(with_slash);
    }
    // Match by filename. `image/cover.png` and `OEBPS/images/cover.png`
    // share `cover.png`. This is best-effort — if multiple images share a
    // filename the first wins.
    let target_name = href.rsplit('/').next().unwrap_or(href);
    image_bytes
        .keys()
        .find(|k| k.rsplit('/').next().unwrap_or(k.as_str()) == target_name)
        .cloned()
}
