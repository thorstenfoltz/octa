//! Unit tests for [`syntax`](syntax). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use eframe::egui;

use super::{HIGHLIGHT_WHITELIST, highlight_layout_job, syntax_for_extension, theme_for_mode};
use crate::formats::FormatRegistry;
use crate::ui::theme::ThemeMode;

/// Every extension we bother to syntax-highlight must also be a *supported*
/// file - i.e. claimed by some registered reader so it shows up in the
/// open dialog's "All Supported" filter and isn't silently routed through
/// an unadvertised fallback. This pins `HIGHLIGHT_WHITELIST` and the format
/// registry's extension set together: adding to one without the other
/// fails here.
#[test]
fn highlight_whitelist_is_supported() {
    let registry = FormatRegistry::new();
    let supported = registry.all_extensions();
    let missing: Vec<&str> = HIGHLIGHT_WHITELIST
        .iter()
        .copied()
        .filter(|&ext| !supported.iter().any(|s| s == ext))
        .collect();
    assert!(
        missing.is_empty(),
        "highlighted but not registered as a supported format: {missing:?} \
             - add them to a FormatReader (TextReader for source code)"
    );
}

#[test]
fn structured_formats_resolve_to_a_syntax() {
    for ext in ["json", "yaml", "yml", "xml", "toml"] {
        assert!(
            super::syntax_for_extension(ext).is_some(),
            "raw view should colour .{ext}"
        );
    }
}

#[test]
fn dockerfile_syntax_is_loaded() {
    assert!(
        super::syntax_set()
            .find_syntax_by_name("Dockerfile")
            .is_some()
    );
}

#[test]
fn dockerfile_filename_resolves_syntax() {
    assert!(super::syntax_for_filename("Dockerfile").is_some());
    assert!(super::syntax_for_filename("Dockerfile.dev").is_some());
    assert!(super::syntax_for_filename("Containerfile").is_some());
    assert!(super::syntax_for_filename("data.csv").is_none());
}

/// egui calls a `TextEdit` layouter on **every frame**, before its galley
/// cache is consulted, so an unmemoized highlighter re-tokenises the whole
/// buffer 60 times a second. A 700 KB JSON cost 4.3 s per frame that way and
/// froze the app while the raw view was open.
///
/// This pins the memo by timing: the second call on an unchanged buffer has
/// to be far cheaper than the first. The ratio is deliberately loose (10x on
/// a 20x-plus win) so a slow CI box cannot make it flap.
#[test]
fn an_unchanged_buffer_is_not_re_tokenised() {
    let syntax = syntax_for_extension("json").expect("json syntax");
    let theme = theme_for_mode(ThemeMode::Dark);
    let font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let text = {
        let mut s = String::from("[\n");
        for i in 0..4000 {
            s.push_str(&format!(
                " {{\"id\": {i}, \"name\": \"record_{i}\", \"flag\": true}},\n"
            ));
        }
        s.push(']');
        s
    };

    let t0 = std::time::Instant::now();
    let first = highlight_layout_job(&text, syntax, theme, font.clone());
    let cold = t0.elapsed();

    let t1 = std::time::Instant::now();
    let second = highlight_layout_job(&text, syntax, theme, font.clone());
    let warm = t1.elapsed();

    assert_eq!(first.text, second.text, "the memo returned different text");
    assert_eq!(
        first.sections.len(),
        second.sections.len(),
        "the memo returned a differently-coloured job"
    );
    assert!(
        warm * 10 < cold,
        "second call was not meaningfully cheaper: cold {cold:?}, warm {warm:?}"
    );
}

/// The memo must not answer with the colours of a different buffer. An edit
/// that keeps the length is the case a cheap fingerprint would get wrong.
#[test]
fn an_edited_buffer_is_highlighted_again() {
    let syntax = syntax_for_extension("json").expect("json syntax");
    let theme = theme_for_mode(ThemeMode::Dark);
    let font = egui::FontId::new(13.0, egui::FontFamily::Monospace);

    let a = highlight_layout_job(r#"{"a": 1234}"#, syntax, theme, font.clone());
    // Same length, different content: `1234` becomes a string.
    let b = highlight_layout_job(r#"{"a":"123"}"#, syntax, theme, font.clone());

    assert_eq!(a.text, r#"{"a": 1234}"#);
    assert_eq!(b.text, r#"{"a":"123"}"#);
}

/// Switching theme must re-colour, or the raw view keeps dark-theme colours
/// after the user moves to a light theme.
#[test]
fn switching_theme_bypasses_the_memo() {
    let syntax = syntax_for_extension("json").expect("json syntax");
    let font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let text = r#"{"a": 1}"#;

    let dark = highlight_layout_job(text, syntax, theme_for_mode(ThemeMode::Dark), font.clone());
    let light = highlight_layout_job(text, syntax, theme_for_mode(ThemeMode::Light), font.clone());

    let colour = |j: &egui::text::LayoutJob| j.sections.first().map(|s| s.format.color);
    assert_ne!(
        colour(&dark),
        colour(&light),
        "the memo served dark colours for the light theme"
    );
}
