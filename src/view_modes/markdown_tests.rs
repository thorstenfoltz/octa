use super::*;

fn run(text: &str, link: Option<&str>) -> (String, RunStyle) {
    (
        text.to_string(),
        RunStyle {
            link: link.map(|s| s.to_string()),
            ..Default::default()
        },
    )
}

#[test]
fn link_spans_finds_char_ranges() {
    // "foo " | "bar"(link) | " baz"  ->  link covers chars 4..7.
    let runs = vec![
        run("foo ", None),
        run("bar", Some("https://example.com")),
        run(" baz", None),
    ];
    let spans = link_spans(&runs);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, 4..7);
    assert_eq!(spans[0].1, "https://example.com");
}

#[test]
fn link_spans_empty_when_no_links() {
    let runs = vec![run("plain text", None)];
    assert!(link_spans(&runs).is_empty());
}

/// The preview follows a wide window: a table spreads across it instead of
/// stopping at a fixed reading width and wrapping its cells (it was capped at
/// 900 pt).
#[test]
fn the_preview_and_its_tables_grow_with_the_window() {
    let mut tab = crate::app::state::TabState::new(octa::data::SearchMode::Plain);
    tab.raw_content = Some("| a | b |\n|---|---|\n| one | two |\n".to_string());
    tab.markdown_layout = MarkdownLayout::Preview;
    let ctx = egui::Context::default();
    use octa::ui::theme::{BodyFont, FontSettings, ThemeMode, apply_theme};
    let font = FontSettings {
        size: 14.0,
        body: BodyFont::Proportional,
        custom_path: None,
    };
    apply_theme(&ctx, ThemeMode::Light, font);
    let mut right = 0.0_f32;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(2000.0, 800.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| render_markdown_view(ui, &mut tab, false, 4));
        out.textures_delta.clear();
        right = out
            .shapes
            .iter()
            .map(|c| c.shape.visual_bounding_rect())
            .filter(|r| r.is_finite())
            .map(|r| r.max.x)
            .fold(0.0, f32::max);
    }
    assert!(right > 1500.0, "the table stops at x = {right}");
}
