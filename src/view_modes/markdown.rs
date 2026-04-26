use crate::app::state::TabState;

use eframe::egui;
use egui::RichText;
use regex::Regex;
use std::sync::LazyLock;

/// Translate a small set of inline HTML tags into CommonMark equivalents so
/// `egui_commonmark` (which has no HTML pass-through) renders them correctly.
fn pre_render_html(md: &str) -> String {
    static BR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
    static BOLD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<(?:b|strong)>(.*?)</(?:b|strong)>").unwrap());
    static ITALIC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<(?:i|em)>(.*?)</(?:i|em)>").unwrap());
    static UNDERLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<u>(.*?)</u>").unwrap());
    static CODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<code>(.*?)</code>").unwrap());
    static LINK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<a\s+[^>]*href\s*=\s*["']([^"']*)["'][^>]*>(.*?)</a>"#).unwrap()
    });
    static HR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<hr\s*/?>").unwrap());
    // Anchor tags that are pure named-anchor targets (no href). The body, if
    // any, is preserved as plain text.
    static ANCHOR_NO_HREF: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<a\b[^>]*>(.*?)</a>").unwrap());
    // Self-closing or unknown void tags we want to strip silently rather
    // than render literally.
    static VOID_TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<(?:span|div|section|article|nav|header|footer|aside|small|sup|sub|kbd|mark|abbr|cite|q|s|del|ins|wbr)\b[^>]*>|</(?:span|div|section|article|nav|header|footer|aside|small|sup|sub|kbd|mark|abbr|cite|q|s|del|ins|wbr)>").unwrap()
    });

    let s = BR.replace_all(md, "  \n").into_owned();
    let s = HR.replace_all(&s, "\n\n---\n\n").into_owned();
    let s = BOLD.replace_all(&s, "**$1**").into_owned();
    let s = ITALIC.replace_all(&s, "*$1*").into_owned();
    let s = UNDERLINE.replace_all(&s, "__$1__").into_owned();
    let s = CODE.replace_all(&s, "`$1`").into_owned();
    let s = LINK.replace_all(&s, "[$2]($1)").into_owned();
    let s = ANCHOR_NO_HREF.replace_all(&s, "$1").into_owned();
    VOID_TAG.replace_all(&s, "").into_owned()
}

/// Slugify heading text the way GitHub-flavored Markdown does, so a heading
/// `## Project Description` resolves the link `#project-description`.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if ch == '-' || ch == '_' {
            out.push(ch);
            prev_dash = false;
        } else if ch.is_whitespace() {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    out.trim_matches('-').to_string()
}

/// Render the Markdown view using commonmark.
pub fn render_markdown_view(ui: &mut egui::Ui, tab: &mut TabState) {
    if let Some(ref content) = tab.raw_content {
        let raw = content.clone();
        let md_content = pre_render_html(&raw);

        // Collect fragment-link destinations (`[text](#anchor)`) and register
        // them as commonmark link hooks. Hooked links render as in-page
        // clickable text rather than as OS hyperlinks, so clicking them no
        // longer opens a browser. We then jump-scroll to the matching
        // heading by computing a heading-slug → line-index map.
        static FRAG_LINK: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\]\((#[^)\s]+)\)").unwrap());
        let fragments: Vec<String> = FRAG_LINK
            .captures_iter(&md_content)
            .map(|c| c[1].to_string())
            .collect();
        for f in &fragments {
            tab.commonmark_cache.add_link_hook(f);
        }

        // Slug → fractional document position. We approximate the scroll
        // position of each heading by its line index relative to total lines
        // — close enough to land near the section without measuring rects.
        let total_lines = md_content.lines().count().max(1) as f32;
        let mut slug_positions: Vec<(String, f32)> = Vec::new();
        for (idx, line) in md_content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let text = trimmed.trim_start_matches('#').trim_start();
                slug_positions.push((slugify(text), idx as f32 / total_lines));
            }
        }

        let bg_response = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("markdown_bg"),
            egui::Sense::click(),
        );
        bg_response.context_menu(|ui| {
            if ui.button("Copy Markdown").clicked() {
                ui.ctx().copy_text(raw.clone());
                ui.close_menu();
            }
        });

        let pending_offset = tab.markdown_scroll_target.take();
        let mut scroll_area = egui::ScrollArea::vertical()
            .id_salt("markdown_scroll")
            .auto_shrink([false, false]);
        if let Some(offset) = pending_offset {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
        let scroll_output = scroll_area.show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_max_width(900.0);
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut tab.commonmark_cache,
                        &md_content,
                    );
                });
            });
        });

        let clicked_fragment = tab
            .commonmark_cache
            .link_hooks()
            .iter()
            .find_map(|(k, v)| if *v { Some(k.clone()) } else { None });
        if let Some(frag) = clicked_fragment {
            tab.commonmark_cache
                .link_hooks_mut()
                .insert(frag.clone(), false);
            let target_slug = frag.trim_start_matches('#').to_string();
            if let Some((_, frac)) = slug_positions.iter().find(|(s, _)| s == &target_slug) {
                let total_h = scroll_output.content_size.y;
                tab.markdown_scroll_target = Some((total_h * frac).max(0.0));
            }
        }
    } else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Markdown content not available")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }
}
