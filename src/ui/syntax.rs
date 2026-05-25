//! Syntax highlighting helpers powered by `syntect`. Wraps the heavy
//! `SyntaxSet` / `ThemeSet` loads in `OnceLock` so they only happen once
//! per process, and exposes a small function that produces an egui
//! `LayoutJob` ready to feed into `TextEdit::layouter`.
//!
//! Used by the raw-text editor and the Jupyter notebook source-cell
//! renderer. The SQL editor stays on its own simple keyword highlighter.

use std::sync::OnceLock;

use eframe::egui;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::syntax_definition::SyntaxDefinition;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::theme::ThemeMode;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Build the syntax set: syntect's bundled defaults plus the hand-written
/// Terraform/HCL definition in `assets/Terraform.sublime-syntax`. The
/// Terraform definition is loaded best-effort — if its YAML ever breaks
/// after a syntect bump, we log a warning and fall back to defaults rather
/// than crash the GUI.
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| {
        let defaults = SyntaxSet::load_defaults_newlines();
        let mut builder = defaults.into_builder();
        static TERRAFORM_YAML: &str = include_str!("../../assets/Terraform.sublime-syntax");
        match SyntaxDefinition::load_from_str(TERRAFORM_YAML, true, Some("source.terraform")) {
            Ok(def) => builder.add(def),
            Err(e) => {
                eprintln!(
                    "warning: bundled Terraform.sublime-syntax failed to load: {e}; \
                     .tf files will render as plain text"
                );
            }
        }
        builder.build()
    })
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Whitelist of file extensions where syntect highlighting is worth its cost.
/// Deliberately narrow:
/// - Languages with no dedicated view in Octa (raw editor is the *only* UI).
/// - JSON/YAML/XML/Markdown/TOML are excluded because they already have
///   tree/preview views; running syntect every frame on a multi-MB JSON in
///   the raw editor made scrolling sluggish.
/// - CSV/TSV are excluded because the existing column-color layouter is more
///   useful for tabular content than per-token syntax coloring.
const HIGHLIGHT_WHITELIST: &[&str] = &[
    // Python and notebooks
    "py", "pyw", "pyi", // Rust
    "rs",  // Shell family
    "sh", "bash", "zsh", "fish", // C / C++ / headers
    "c", "cpp", "cc", "cxx", "h", "hpp", "hxx", // Go
    "go",  // Web languages (server-side and client-side scripting)
    "js", "jsx", "mjs", "cjs", "ts", "tsx", // JVM family
    "java", "kt", "kts", "scala", "groovy", // Scripting
    "rb", "php", "pl", "lua", "swift", // Data-science neighbours
    "r", "jl", // Web markup we *do* highlight (no dedicated viewer)
    "html", "htm", "css", "scss", "sass", // Misc
    "tex", "dart", "ex", "exs", // Terraform / HCL — custom syntax bundled in assets/
    "tf", "tfvars", "hcl",
];

/// Resolve a file extension (without leading dot, lowercased) to a syntax
/// definition. Returns `None` when the extension isn't on the whitelist or
/// syntect's default set has no match — the caller falls back to plain
/// rendering in either case.
///
/// We deliberately don't trust `syntect`'s extension matcher for *everything*
/// it knows because that matcher also covers JSON/YAML/XML/etc., which we
/// want rendered through their dedicated tree views (and which were the
/// source of the raw-editor slowdown).
pub fn syntax_for_extension(ext: &str) -> Option<&'static SyntaxReference> {
    if !HIGHLIGHT_WHITELIST.contains(&ext) {
        return None;
    }
    syntax_set().find_syntax_by_extension(ext)
}

/// Resolve by syntect's syntax name (e.g. `"Python"`, `"JSON"`). Used by
/// the notebook renderer, where the language is stated in the notebook
/// metadata rather than inferable from a file extension.
pub fn syntax_by_name(name: &str) -> Option<&'static SyntaxReference> {
    syntax_set().find_syntax_by_name(name)
}

/// Pick a syntect theme appropriate for Octa's light/dark mode. Themes are
/// bundled with syntect (`InspiredGitHub` for light, `base16-mocha.dark`
/// for dark). Indexing into the BTreeMap returns a reference that lives
/// for `'static` thanks to the OnceLock.
pub fn theme_for_mode(mode: ThemeMode) -> &'static Theme {
    let ts = theme_set();
    let key = match mode {
        ThemeMode::Light => "InspiredGitHub",
        _ => "base16-mocha.dark",
    };
    ts.themes
        .get(key)
        .or_else(|| ts.themes.values().next())
        .expect("syntect bundles at least one theme")
}

/// Highlight `text` with the given syntax + theme and produce an egui
/// `LayoutJob`. `font_id` controls glyph size and family — pass whatever
/// font the surrounding TextEdit uses so the highlighted spans align with
/// the editor cursor.
///
/// The job has wrapping disabled (`max_width = INFINITY`) which matches the
/// raw editor's no-wrap convention. Long lines scroll horizontally.
pub fn highlight_layout_job(
    text: &str,
    syntax: &SyntaxReference,
    theme: &Theme,
    font_id: egui::FontId,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let mut h = HighlightLines::new(syntax, theme);
    let ss = syntax_set();
    for line in LinesWithEndings::from(text) {
        let regions = match h.highlight_line(line, ss) {
            Ok(r) => r,
            Err(_) => {
                // syntect returned an error mid-line. Don't drop the line —
                // append it as plain text so the user still sees their code.
                job.append(
                    line,
                    0.0,
                    egui::text::TextFormat::simple(
                        font_id.clone(),
                        egui::Color32::from_rgb(0xc0, 0xc0, 0xc0),
                    ),
                );
                continue;
            }
        };
        for (style, segment) in regions {
            let fg = style.foreground;
            let color = egui::Color32::from_rgb(fg.r, fg.g, fg.b);
            job.append(
                segment,
                0.0,
                egui::text::TextFormat::simple(font_id.clone(), color),
            );
        }
    }
    job
}
