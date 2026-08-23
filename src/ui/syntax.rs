//! Syntax highlighting helpers powered by `syntect`. Wraps the heavy
//! `SyntaxSet` / `ThemeSet` loads in `OnceLock` so they only happen once
//! per process, and exposes a small function that produces an egui
//! `LayoutJob` ready to feed into `TextEdit::layouter`.
//!
//! Used by the raw-text editor and the Jupyter notebook source-cell
//! renderer. The SQL editor stays on its own simple keyword highlighter.

use std::sync::{Mutex, OnceLock};

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
/// Terraform definition is loaded best-effort - if its YAML ever breaks
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
        static TOML_YAML: &str = include_str!("../../assets/TOML.sublime-syntax");
        match SyntaxDefinition::load_from_str(TOML_YAML, true, Some("source.toml")) {
            Ok(def) => builder.add(def),
            Err(e) => {
                eprintln!(
                    "warning: bundled TOML.sublime-syntax failed to load: {e}; \
                     .toml files will render as plain text"
                );
            }
        }
        static DOCKERFILE_YAML: &str = include_str!("../../assets/Dockerfile.sublime-syntax");
        match SyntaxDefinition::load_from_str(DOCKERFILE_YAML, true, Some("source.dockerfile")) {
            Ok(def) => builder.add(def),
            Err(e) => {
                eprintln!(
                    "warning: bundled Dockerfile.sublime-syntax failed to load: {e}; \
                     Dockerfiles will render as plain text"
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
/// - JSON/YAML/XML/TOML are now coloured here too: they default to the raw
///   view, and the size gate keeps large files fast. Markdown stays excluded
///   (it has its own preview).
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
    "tex", "dart", "ex", "exs", // Terraform / HCL - custom syntax bundled in assets/
    "tf", "tfvars", "hcl",
    // Structured config/data formats. JSON/YAML/XML come from syntect's
    // defaults; TOML from the bundled assets/TOML.sublime-syntax. These now
    // open in the raw view (YAML/TOML/XML default to it), so colouring them
    // there is wanted. Large files still fall back to plain via the size gate.
    "json", "jsonl", "yaml", "yml", "xml", "toml",
];

/// Resolve a file extension (without leading dot, lowercased) to a syntax
/// definition. Returns `None` when the extension isn't on the whitelist or
/// syntect's default set has no match - the caller falls back to plain
/// rendering in either case.
///
/// We deliberately don't trust `syntect`'s extension matcher for *everything*
/// it knows: the whitelist is the list of extensions we have decided look
/// better coloured, and it is what a reviewer reads to see the set.
///
/// JSON/YAML/XML/TOML **are** on it. They were once excluded because
/// colouring them re-ran syntect on every frame and made the raw editor
/// unusable; that is fixed at the source now (see [`HIGHLIGHT_MEMO`]), so
/// the exclusion is gone. Do not re-derive the old rule from an old comment.
pub fn syntax_for_extension(ext: &str) -> Option<&'static SyntaxReference> {
    if !HIGHLIGHT_WHITELIST.contains(&ext) {
        return None;
    }
    syntax_set().find_syntax_by_extension(ext)
}

/// Resolve a bare filename (e.g. `Dockerfile`, `Dockerfile.dev`) to a syntax
/// by the bundled syntax name. Used when the extension yields no syntax.
pub fn syntax_for_filename(file_name: &str) -> Option<&'static SyntaxReference> {
    let stem = file_name.split('.').next().unwrap_or(file_name);
    match stem.to_ascii_lowercase().as_str() {
        "dockerfile" | "containerfile" => syntax_set().find_syntax_by_name("Dockerfile"),
        _ => None,
    }
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

/// Last highlight result, kept so an unchanged buffer is tokenised once
/// rather than once per frame.
///
/// **egui calls a `TextEdit` layouter on every frame, before any galley
/// cache is consulted** - the galley cache is keyed on the `LayoutJob` the
/// layouter returns, so it cannot save the work of producing one. Without a
/// memo here, syntect re-tokenised the entire buffer 60 times a second: a
/// 700 KB JSON cost 4.3 s per frame and the whole app was unusable while the
/// raw view was open. Memoizing is what egui's own `TextEdit::layouter`
/// documentation tells you to do.
///
/// One entry, because one raw editor is on screen at a time. A notebook with
/// many source cells cycles the entry between cells and simply gets no
/// benefit, which is the behaviour it had before this existed.
///
/// The entry outlives the tab that produced it: it holds the buffer plus one
/// section per token until something else is highlighted. The text half is
/// bounded by `syntax_highlight_max_bytes` (1 MB); the sections are not
/// bounded by anything but the token count, so a token-dense megabyte can
/// retain some tens of MB. Replaced, never grown, so it is a ceiling rather
/// than a leak - clear it on tab close if that ever matters.
static HIGHLIGHT_MEMO: Mutex<Option<Memo>> = Mutex::new(None);

struct Memo {
    syntax: String,
    theme: String,
    font: egui::FontId,
    job: egui::text::LayoutJob,
}

impl Memo {
    /// Whether this entry answers the request.
    ///
    /// The text is compared, not hashed: a `LayoutJob` already owns the text
    /// it was built from, so the comparison is a `memcmp` against something
    /// we are storing anyway. That is both cheaper than hashing the buffer
    /// every frame and exact, where a cheap fingerprint would leave stale
    /// colours behind on any edit that preserved the length.
    fn answers(
        &self,
        text: &str,
        syntax: &SyntaxReference,
        theme: &Theme,
        font_id: &egui::FontId,
    ) -> bool {
        self.syntax == syntax.name
            && self.theme == theme.name.as_deref().unwrap_or_default()
            && &self.font == font_id
            && self.job.text == text
    }
}

/// Highlight `text` with the given syntax + theme and produce an egui
/// `LayoutJob`. `font_id` controls glyph size and family - pass whatever
/// font the surrounding TextEdit uses so the highlighted spans align with
/// the editor cursor.
///
/// The result is memoized (see [`HIGHLIGHT_MEMO`]): calling this every frame
/// with an unchanged buffer costs a comparison and a clone, not a
/// re-tokenisation.
///
/// The job has wrapping disabled (`max_width = INFINITY`) which matches the
/// raw editor's no-wrap convention. Long lines scroll horizontally.
pub fn highlight_layout_job(
    text: &str,
    syntax: &SyntaxReference,
    theme: &Theme,
    font_id: egui::FontId,
) -> egui::text::LayoutJob {
    if let Ok(memo) = HIGHLIGHT_MEMO.lock()
        && let Some(entry) = memo.as_ref()
        && entry.answers(text, syntax, theme, &font_id)
    {
        return entry.job.clone();
    }
    let job = highlight_uncached(text, syntax, theme, font_id.clone());
    if let Ok(mut memo) = HIGHLIGHT_MEMO.lock() {
        *memo = Some(Memo {
            syntax: syntax.name.clone(),
            theme: theme.name.clone().unwrap_or_default(),
            font: font_id,
            job: job.clone(),
        });
    }
    job
}

fn highlight_uncached(
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
                // syntect returned an error mid-line. Don't drop the line -
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

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;
