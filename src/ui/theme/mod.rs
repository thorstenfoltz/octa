mod palettes;
mod visuals;

pub use visuals::paint_background_decoration;

use crate::data::MarkColor;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle,
    Visuals,
};
use std::sync::Arc;

/// Theme presets. Each preset provides a complete color palette and is
/// classified as light or dark for purposes of egui's base visuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    Nord,
    Dracula,
    GruvboxDark,
    HighContrast,
    /// Bright colorful manga / shōnen-page look on cream paper, with sakura
    /// pink and sky-blue accents and high-contrast ink text.
    Manga,
    /// Refined gentleman's-club palette: deep walnut and burgundy backgrounds
    /// with champagne-gold accents and warm parchment text.
    Gentleman,
    /// Deep ocean blue: a Nord-flavoured but bluer and more saturated palette,
    /// with abyssal navy backgrounds and lagoon-blue accents.
    DeepSea,
    /// Frost: cool, near-white backgrounds with pale ice-blue accents and
    /// dark slate text. The light counterpart to Deep Sea.
    Frost,
    /// Hidden easter-egg theme - not listed in `ALL`, only reachable by
    /// clicking the toolbar logo seven times in quick succession. Cycles the
    /// accent hue every frame.
    Rainbow,
}

impl ThemeMode {
    pub const ALL: &[ThemeMode] = &[
        Self::Light,
        Self::Dark,
        Self::Nord,
        Self::Dracula,
        Self::GruvboxDark,
        Self::HighContrast,
        Self::Manga,
        Self::Gentleman,
        Self::DeepSea,
        Self::Frost,
    ];

    /// Whether the preset has a dark background. Drives base egui visuals
    /// and any view-mode logic that wants to swap text colors per brightness.
    pub fn is_dark(self) -> bool {
        match self {
            Self::Light | Self::Manga | Self::Frost => false,
            Self::Dark
            | Self::Nord
            | Self::Dracula
            | Self::GruvboxDark
            | Self::HighContrast
            | Self::Gentleman
            | Self::DeepSea
            | Self::Rainbow => true,
        }
    }

    /// Whether the hidden Rainbow easter-egg theme is active. Used by the
    /// table view to force a fixed high-contrast text colour on marked /
    /// selected cells (the live `text_primary` cycles through hues and
    /// can collide with the mark background).
    pub fn is_rainbow(self) -> bool {
        matches!(self, Self::Rainbow)
    }

    /// Convenience toggle between the basic Light and Dark presets used by
    /// the toolbar quick-toggle button. Custom presets toggle to their
    /// brightness opposite (basic Light or Dark).
    pub fn toggle(&self) -> Self {
        if self.is_dark() {
            Self::Light
        } else {
            Self::Dark
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Nord => "Nord",
            Self::Dracula => "Dracula",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::HighContrast => "High Contrast",
            Self::Manga => "Manga",
            Self::Gentleman => "Gentleman",
            Self::DeepSea => "Deep Sea",
            Self::Frost => "Frost",
            Self::Rainbow => "Rainbow",
        }
    }

    pub fn icon(&self) -> &str {
        if self.is_dark() { "☀" } else { "🌙" }
    }
}

/// Body font choice. `Default` uses egui's built-in proportional font;
/// `Monospace` swaps body, button and heading text to a monospace face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum BodyFont {
    #[default]
    Proportional,
    Monospace,
}

impl BodyFont {
    pub const ALL: &[BodyFont] = &[Self::Proportional, Self::Monospace];

    pub fn label(self) -> &'static str {
        match self {
            Self::Proportional => "Proportional",
            Self::Monospace => "Monospace",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Proportional => "enum.font_proportional",
            Self::Monospace => "enum.font_monospace",
        })
    }
}

/// Bundle of font-related parameters for `apply_theme`.
pub struct FontSettings<'a> {
    pub size: f32,
    pub body: BodyFont,
    /// Optional path to a user-supplied .ttf/.otf file. When set and
    /// readable, this font is used as the primary proportional face.
    pub custom_path: Option<&'a str>,
}

/// Color palette for the application
pub struct ThemeColors {
    // Backgrounds
    pub bg_primary: Color32,
    pub bg_secondary: Color32,
    pub bg_tertiary: Color32,
    pub bg_header: Color32,
    pub bg_selected: Color32,
    pub bg_hover: Color32,
    pub bg_edited: Color32,

    // Text
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_header: Color32,

    // Accents
    pub accent: Color32,
    pub accent_hover: Color32,
    pub border: Color32,
    pub border_subtle: Color32,

    // Status
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,

    // Table specific
    pub row_even: Color32,
    pub row_odd: Color32,
    pub row_number_bg: Color32,
    pub row_number_text: Color32,

    // Scrollbar
    pub scrollbar_track: Color32,
    pub scrollbar_thumb: Color32,
    pub scrollbar_thumb_hover: Color32,
}

impl ThemeColors {
    /// Get the background color for a mark highlight.
    pub fn mark_color(&self, mark: MarkColor) -> Color32 {
        match mark {
            MarkColor::Red => Color32::from_rgba_unmultiplied(220, 38, 38, 90),
            MarkColor::Orange => Color32::from_rgba_unmultiplied(234, 88, 12, 90),
            MarkColor::Yellow => Color32::from_rgba_unmultiplied(250, 204, 21, 90),
            MarkColor::Green => Color32::from_rgba_unmultiplied(34, 197, 94, 90),
            MarkColor::Blue => Color32::from_rgba_unmultiplied(59, 130, 246, 90),
            MarkColor::Purple => Color32::from_rgba_unmultiplied(168, 85, 247, 90),
        }
    }

    /// Get a solid color swatch for the color picker.
    pub fn mark_swatch(mark: MarkColor) -> Color32 {
        match mark {
            MarkColor::Red => Color32::from_rgb(220, 38, 38),
            MarkColor::Orange => Color32::from_rgb(234, 88, 12),
            MarkColor::Yellow => Color32::from_rgb(250, 204, 21),
            MarkColor::Green => Color32::from_rgb(34, 197, 94),
            MarkColor::Blue => Color32::from_rgb(59, 130, 246),
            MarkColor::Purple => Color32::from_rgb(168, 85, 247),
        }
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
            ThemeMode::Nord => Self::nord(),
            ThemeMode::Dracula => Self::dracula(),
            ThemeMode::GruvboxDark => Self::gruvbox_dark(),
            ThemeMode::HighContrast => Self::high_contrast(),
            ThemeMode::Manga => Self::manga(),
            ThemeMode::Gentleman => Self::gentleman(),
            ThemeMode::DeepSea => Self::deep_sea(),
            ThemeMode::Frost => Self::frost(),
            ThemeMode::Rainbow => Self::rainbow(),
        }
    }
}

/// HSV -> RGB helper for the hidden Rainbow theme. `h` in 0..1, s/v in 0..1.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color32 {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Apply the theme to an egui context.
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode, font: FontSettings) {
    let mut colors = ThemeColors::for_mode(mode);
    if mode == ThemeMode::Rainbow {
        // Hidden easter-egg theme: cycle ten palette slots through HSV at
        // staggered phase offsets so accents, text, borders, row stripes and
        // selection all glide through the spectrum simultaneously. ~10s per
        // full hue cycle - fast enough to read as motion, slow enough to not
        // induce a headache. Background fills stay near-black (low saturation)
        // so the table remains readable.
        let t = ctx.input(|i| i.time) as f32 * 0.10;
        let h = |off: f32| ((t + off).fract() + 1.0).fract();
        colors.accent = hsv_to_rgb(h(0.00), 0.85, 0.95);
        colors.accent_hover = hsv_to_rgb(h(0.05), 0.85, 1.00);
        colors.text_header = hsv_to_rgb(h(0.50), 0.70, 1.00);
        colors.text_primary = hsv_to_rgb(h(0.33), 0.30, 0.95);
        colors.text_secondary = hsv_to_rgb(h(0.66), 0.30, 0.85);
        colors.border = hsv_to_rgb(h(0.20), 0.55, 0.70);
        colors.border_subtle = hsv_to_rgb(h(0.20), 0.30, 0.40);
        colors.row_odd = hsv_to_rgb(h(0.40), 0.20, 0.18);
        colors.bg_selected = hsv_to_rgb(h(0.10), 0.45, 0.35);
        colors.warning = hsv_to_rgb(h(0.80), 0.85, 1.00);
        ctx.request_repaint();
    }
    let is_dark = mode.is_dark();

    let mut style = Style::default();

    let mut visuals = if is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.window_fill = colors.bg_primary;
    visuals.panel_fill = colors.bg_primary;
    visuals.extreme_bg_color = if is_dark {
        colors.bg_secondary
    } else {
        Color32::from_rgb(230, 233, 240)
    };
    visuals.faint_bg_color = if is_dark {
        colors.bg_tertiary
    } else {
        Color32::from_rgb(237, 240, 245)
    };
    visuals.window_stroke = Stroke::new(1.0, colors.border);

    visuals.widgets.noninteractive.bg_fill = colors.bg_secondary;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text_primary);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, colors.border);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = colors.bg_secondary;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, colors.text_primary);
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, colors.border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    // Hover highlighting needs to read clearly on small icon-style widgets
    // such as the window close-X. We pair a tinted accent fill with a thicker
    // accent stroke so the whole hit-target lights up - the previous bg_hover
    // fill alone was too subtle to spot on the close button.
    visuals.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.28);
    visuals.widgets.hovered.fg_stroke = Stroke::new(2.0, colors.accent);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, colors.accent);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = colors.accent;
    visuals.widgets.active.fg_stroke = Stroke::new(
        1.0,
        if is_dark {
            Color32::WHITE
        } else {
            colors.text_primary
        },
    );
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, colors.accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.selection.bg_fill = colors.bg_selected;
    // egui's `interact_selectable` aliases the selected widget's *fg_stroke*
    // (the text color) to `selection.stroke`. The accent color reads cleanly
    // on light themes but turns into mid-saturation-on-mid-saturation in
    // dark themes - selected radio/menu/list entries become unreadable.
    // Use a high-contrast color in dark mode and slate in light mode.
    let selection_stroke_color = if is_dark {
        Color32::WHITE
    } else {
        colors.text_primary
    };
    visuals.selection.stroke = Stroke::new(1.0, selection_stroke_color);

    visuals.hyperlink_color = colors.accent;
    visuals.warn_fg_color = colors.warning;
    visuals.error_fg_color = colors.error;
    visuals.code_bg_color = if is_dark {
        Color32::from_rgb(40, 40, 48)
    } else {
        Color32::from_rgb(230, 233, 240)
    };
    visuals.override_text_color = None;

    style.visuals = visuals;

    apply_fonts(ctx, &font);

    let proportional = primary_family(&font);
    let small = (font.size * 0.85).round();
    let heading = (font.size * 1.38).round();
    style.text_styles = [
        (TextStyle::Small, FontId::new(small, proportional.clone())),
        (
            TextStyle::Body,
            FontId::new(font.size, proportional.clone()),
        ),
        (
            TextStyle::Monospace,
            FontId::new(font.size, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(font.size, proportional.clone()),
        ),
        (TextStyle::Heading, FontId::new(heading, proportional)),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);

    // Per-theme structural overrides. These tweak shape/padding/stroke beyond
    // the color subset, so a theme can have a distinctively different *feel*
    // (e.g. chunky rounded "manga sticker" buttons) instead of just a palette
    // swap. Keep edits minimal - anything done here ripples across every view.
    visuals::apply_theme_decoration(&mut style, mode, &colors);

    ctx.set_global_style(style);
}

/// Resolve which font family proportional/body text should use.
fn primary_family(font: &FontSettings) -> FontFamily {
    if font.custom_path.is_some_and(|p| !p.is_empty()) {
        FontFamily::Name(Arc::from("custom"))
    } else if font.body == BodyFont::Monospace {
        FontFamily::Monospace
    } else {
        FontFamily::Proportional
    }
}

/// Register fonts. Called every time `apply_theme` runs (settings/zoom
/// changes), but egui caches by content hash so repeats are cheap.
fn apply_fonts(ctx: &egui::Context, font: &FontSettings) {
    let mut defs = FontDefinitions::default();
    if let Some(path) = font.custom_path.filter(|p| !p.is_empty())
        && let Ok(bytes) = std::fs::read(path)
    {
        defs.font_data
            .insert("custom".into(), Arc::new(FontData::from_owned(bytes)));
        // Custom font becomes a named family that style maps to Body.
        defs.families
            .insert(FontFamily::Name(Arc::from("custom")), vec!["custom".into()]);
    }
    // Bundled bold face used by the markdown preview renderer for `**text**`.
    // egui's default fonts don't include a bold variant, so `RichText::strong()`
    // only tweaks color - the markdown renderer needs a real bold glyph set
    // to show weight contrast.
    static BOLD_BYTES: &[u8] = include_bytes!("../../../assets/Roboto-Bold.ttf");
    defs.font_data
        .insert("bold".into(), Arc::new(FontData::from_static(BOLD_BYTES)));
    defs.families
        .insert(FontFamily::Name(Arc::from("bold")), vec!["bold".into()]);

    // Bundled Roboto Medium becomes the **default proportional** face - it sits
    // between the upstream Ubuntu-Light (too thin for readability on bars / tabs
    // / column headers / docs) and Roboto-Bold (which reads as too heavy for
    // body prose). Apache-2.0, attributed in `licenses/Apache-2.0.txt`.
    static MEDIUM_BYTES: &[u8] = include_bytes!("../../../assets/Roboto-Medium.ttf");
    defs.font_data.insert(
        "medium".into(),
        Arc::new(FontData::from_static(MEDIUM_BYTES)),
    );
    if let Some(prop) = defs.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "medium".into());
    } else {
        defs.families
            .insert(FontFamily::Proportional, vec!["medium".into()]);
    }

    // Bundled JetBrains Mono Regular - opt-in family for the SQL editor and
    // any other monospace-heavy view that the user can switch on in Settings.
    // OFL-1.1, attributed in `licenses/OFL-1.1.txt`.
    static SQL_MONO_BYTES: &[u8] = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");
    defs.font_data.insert(
        "sql_mono".into(),
        Arc::new(FontData::from_static(SQL_MONO_BYTES)),
    );
    defs.families.insert(
        FontFamily::Name(Arc::from("sql_mono")),
        vec!["sql_mono".into()],
    );

    // Broad Unicode fallback faces so non-Latin data (CJK ideographs, Japanese
    // kana, Korean hangul) and miscellaneous symbols render instead of tofu
    // boxes. **Appended** (not prepended) to the families so Roboto stays the
    // primary face for Latin text and these only fill the gaps. The CJK face is
    // a `pyftsubset` cut of Noto Sans CJK SC (OFL-1.1, attributed in
    // `licenses/OFL-1.1.txt`) trimmed to the common BMP CJK/kana/hangul ranges
    // to keep it ~6.5 MB instead of 16 MB; ab_glyph reads its CFF outlines via
    // ttf-parser. Rare CJK Ext-A/B, colour emoji, and full RTL shaping are out
    // of scope - egui renders monochrome glyphs only.
    static CJK_BYTES: &[u8] = include_bytes!("../../../assets/NotoSansCJK-subset.otf");
    defs.font_data
        .insert("cjk".into(), Arc::new(FontData::from_static(CJK_BYTES)));
    static SYMBOLS_BYTES: &[u8] = include_bytes!("../../../assets/NotoSansSymbols2-Regular.ttf");
    defs.font_data.insert(
        "symbols".into(),
        Arc::new(FontData::from_static(SYMBOLS_BYTES)),
    );
    // The bundled monospace face is Latin-only, and neither the CJK nor the
    // symbols face covers Greek or Cyrillic. Append Roboto Medium to the
    // monospace fallback chain so localized monospace text (e.g. Greek /
    // Russian in the SQL editor or gutter) renders instead of tofu - it only
    // fills glyphs the mono face lacks, so ASCII stays monospaced.
    if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
        mono.push("medium".into());
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let fam = defs.families.entry(family).or_default();
        fam.push("cjk".into());
        fam.push("symbols".into());
    }

    ctx.set_fonts(defs);
}
