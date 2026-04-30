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
    ];

    /// Whether the preset has a dark background. Drives base egui visuals
    /// and any view-mode logic that wants to swap text colors per brightness.
    pub fn is_dark(self) -> bool {
        match self {
            Self::Light | Self::Manga => false,
            Self::Dark
            | Self::Nord
            | Self::Dracula
            | Self::GruvboxDark
            | Self::HighContrast
            | Self::Gentleman => true,
        }
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
#[allow(dead_code)]
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
        }
    }

    fn dark() -> Self {
        Self {
            bg_primary: Color32::from_rgb(24, 24, 27),
            bg_secondary: Color32::from_rgb(39, 39, 42),
            bg_tertiary: Color32::from_rgb(52, 52, 56),
            bg_header: Color32::from_rgb(33, 33, 40),
            bg_selected: Color32::from_rgba_unmultiplied(99, 102, 241, 100),
            bg_hover: Color32::from_rgb(45, 45, 50),
            bg_edited: Color32::from_rgb(50, 40, 20),

            text_primary: Color32::from_rgb(244, 244, 245),
            text_secondary: Color32::from_rgb(161, 161, 170),
            text_muted: Color32::from_rgb(113, 113, 122),
            text_header: Color32::from_rgb(228, 228, 231),

            accent: Color32::from_rgb(99, 102, 241),
            accent_hover: Color32::from_rgb(129, 140, 248),
            border: Color32::from_rgb(63, 63, 70),
            border_subtle: Color32::from_rgb(39, 39, 42),

            success: Color32::from_rgb(34, 197, 94),
            warning: Color32::from_rgb(234, 179, 8),
            error: Color32::from_rgb(239, 68, 68),

            row_even: Color32::from_rgb(24, 24, 27),
            row_odd: Color32::from_rgb(36, 36, 42),
            row_number_bg: Color32::from_rgb(32, 32, 36),
            row_number_text: Color32::from_rgb(113, 113, 122),

            scrollbar_track: Color32::from_rgb(40, 40, 45),
            scrollbar_thumb: Color32::from_rgb(80, 80, 90),
            scrollbar_thumb_hover: Color32::from_rgb(110, 110, 120),
        }
    }

    fn light() -> Self {
        Self {
            bg_primary: Color32::from_rgb(255, 255, 255),
            bg_secondary: Color32::from_rgb(249, 250, 251),
            bg_tertiary: Color32::from_rgb(243, 244, 246),
            bg_header: Color32::from_rgb(238, 240, 248),
            bg_selected: Color32::from_rgb(191, 219, 254),
            bg_hover: Color32::from_rgb(243, 244, 246),
            bg_edited: Color32::from_rgb(255, 249, 219),

            text_primary: Color32::from_rgb(17, 24, 39),
            text_secondary: Color32::from_rgb(107, 114, 128),
            text_muted: Color32::from_rgb(156, 163, 175),
            text_header: Color32::from_rgb(31, 41, 55),

            accent: Color32::from_rgb(79, 70, 229),
            accent_hover: Color32::from_rgb(99, 102, 241),
            border: Color32::from_rgb(229, 231, 235),
            border_subtle: Color32::from_rgb(243, 244, 246),

            success: Color32::from_rgb(22, 163, 74),
            warning: Color32::from_rgb(202, 138, 4),
            error: Color32::from_rgb(220, 38, 38),

            row_even: Color32::from_rgb(255, 255, 255),
            row_odd: Color32::from_rgb(240, 242, 245),
            row_number_bg: Color32::from_rgb(243, 244, 246),
            row_number_text: Color32::from_rgb(156, 163, 175),

            scrollbar_track: Color32::from_rgb(230, 230, 235),
            scrollbar_thumb: Color32::from_rgb(180, 180, 190),
            scrollbar_thumb_hover: Color32::from_rgb(140, 140, 155),
        }
    }

    fn nord() -> Self {
        // Arctic / north-bluish.
        Self {
            bg_primary: Color32::from_rgb(0x2e, 0x34, 0x40),
            bg_secondary: Color32::from_rgb(0x3b, 0x42, 0x52),
            bg_tertiary: Color32::from_rgb(0x43, 0x4c, 0x5e),
            bg_header: Color32::from_rgb(0x36, 0x3d, 0x4c),
            bg_selected: Color32::from_rgba_unmultiplied(0x88, 0xc0, 0xd0, 100),
            bg_hover: Color32::from_rgb(0x4c, 0x56, 0x6a),
            bg_edited: Color32::from_rgb(0x4c, 0x40, 0x28),

            text_primary: Color32::from_rgb(0xec, 0xef, 0xf4),
            text_secondary: Color32::from_rgb(0xd8, 0xde, 0xe9),
            text_muted: Color32::from_rgb(0x81, 0x8e, 0xa3),
            text_header: Color32::from_rgb(0xec, 0xef, 0xf4),

            accent: Color32::from_rgb(0x88, 0xc0, 0xd0),
            accent_hover: Color32::from_rgb(0x8f, 0xbc, 0xbb),
            border: Color32::from_rgb(0x43, 0x4c, 0x5e),
            border_subtle: Color32::from_rgb(0x3b, 0x42, 0x52),

            success: Color32::from_rgb(0xa3, 0xbe, 0x8c),
            warning: Color32::from_rgb(0xeb, 0xcb, 0x8b),
            error: Color32::from_rgb(0xbf, 0x61, 0x6a),

            row_even: Color32::from_rgb(0x2e, 0x34, 0x40),
            row_odd: Color32::from_rgb(0x36, 0x3d, 0x4a),
            row_number_bg: Color32::from_rgb(0x36, 0x3d, 0x4c),
            row_number_text: Color32::from_rgb(0x81, 0x8e, 0xa3),

            scrollbar_track: Color32::from_rgb(0x3b, 0x42, 0x52),
            scrollbar_thumb: Color32::from_rgb(0x4c, 0x56, 0x6a),
            scrollbar_thumb_hover: Color32::from_rgb(0x81, 0x8e, 0xa3),
        }
    }

    fn dracula() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0x28, 0x2a, 0x36),
            bg_secondary: Color32::from_rgb(0x32, 0x34, 0x42),
            bg_tertiary: Color32::from_rgb(0x44, 0x47, 0x5a),
            bg_header: Color32::from_rgb(0x36, 0x39, 0x48),
            bg_selected: Color32::from_rgba_unmultiplied(0xbd, 0x93, 0xf9, 90),
            bg_hover: Color32::from_rgb(0x44, 0x47, 0x5a),
            bg_edited: Color32::from_rgb(0x4a, 0x44, 0x1c),

            text_primary: Color32::from_rgb(0xf8, 0xf8, 0xf2),
            text_secondary: Color32::from_rgb(0xbd, 0xc1, 0xd1),
            text_muted: Color32::from_rgb(0x6c, 0x70, 0x88),
            text_header: Color32::from_rgb(0xf8, 0xf8, 0xf2),

            accent: Color32::from_rgb(0xbd, 0x93, 0xf9),
            accent_hover: Color32::from_rgb(0xff, 0x79, 0xc6),
            border: Color32::from_rgb(0x44, 0x47, 0x5a),
            border_subtle: Color32::from_rgb(0x32, 0x34, 0x42),

            success: Color32::from_rgb(0x50, 0xfa, 0x7b),
            warning: Color32::from_rgb(0xf1, 0xfa, 0x8c),
            error: Color32::from_rgb(0xff, 0x55, 0x55),

            row_even: Color32::from_rgb(0x28, 0x2a, 0x36),
            row_odd: Color32::from_rgb(0x30, 0x32, 0x40),
            row_number_bg: Color32::from_rgb(0x32, 0x34, 0x42),
            row_number_text: Color32::from_rgb(0x6c, 0x70, 0x88),

            scrollbar_track: Color32::from_rgb(0x32, 0x34, 0x42),
            scrollbar_thumb: Color32::from_rgb(0x44, 0x47, 0x5a),
            scrollbar_thumb_hover: Color32::from_rgb(0x6c, 0x70, 0x88),
        }
    }

    fn gruvbox_dark() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0x28, 0x28, 0x28),
            bg_secondary: Color32::from_rgb(0x32, 0x30, 0x2f),
            bg_tertiary: Color32::from_rgb(0x3c, 0x38, 0x36),
            bg_header: Color32::from_rgb(0x32, 0x30, 0x2f),
            bg_selected: Color32::from_rgba_unmultiplied(0xfa, 0xbd, 0x2f, 90),
            bg_hover: Color32::from_rgb(0x50, 0x49, 0x45),
            bg_edited: Color32::from_rgb(0x4a, 0x40, 0x14),

            text_primary: Color32::from_rgb(0xeb, 0xdb, 0xb2),
            text_secondary: Color32::from_rgb(0xd5, 0xc4, 0xa1),
            text_muted: Color32::from_rgb(0x92, 0x83, 0x74),
            text_header: Color32::from_rgb(0xfb, 0xf1, 0xc7),

            accent: Color32::from_rgb(0xfa, 0xbd, 0x2f),
            accent_hover: Color32::from_rgb(0xfe, 0x80, 0x19),
            border: Color32::from_rgb(0x50, 0x49, 0x45),
            border_subtle: Color32::from_rgb(0x3c, 0x38, 0x36),

            success: Color32::from_rgb(0xb8, 0xbb, 0x26),
            warning: Color32::from_rgb(0xfa, 0xbd, 0x2f),
            error: Color32::from_rgb(0xfb, 0x49, 0x34),

            row_even: Color32::from_rgb(0x28, 0x28, 0x28),
            row_odd: Color32::from_rgb(0x32, 0x2e, 0x2c),
            row_number_bg: Color32::from_rgb(0x32, 0x30, 0x2f),
            row_number_text: Color32::from_rgb(0x92, 0x83, 0x74),

            scrollbar_track: Color32::from_rgb(0x32, 0x30, 0x2f),
            scrollbar_thumb: Color32::from_rgb(0x50, 0x49, 0x45),
            scrollbar_thumb_hover: Color32::from_rgb(0x92, 0x83, 0x74),
        }
    }

    fn high_contrast() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0x00, 0x00, 0x00),
            bg_secondary: Color32::from_rgb(0x10, 0x10, 0x10),
            bg_tertiary: Color32::from_rgb(0x1c, 0x1c, 0x1c),
            bg_header: Color32::from_rgb(0x18, 0x18, 0x18),
            bg_selected: Color32::from_rgba_unmultiplied(0xff, 0xd7, 0x00, 130),
            bg_hover: Color32::from_rgb(0x2a, 0x2a, 0x2a),
            bg_edited: Color32::from_rgb(0x55, 0x44, 0x00),

            text_primary: Color32::from_rgb(0xff, 0xff, 0xff),
            text_secondary: Color32::from_rgb(0xe0, 0xe0, 0xe0),
            text_muted: Color32::from_rgb(0xb0, 0xb0, 0xb0),
            text_header: Color32::from_rgb(0xff, 0xd7, 0x00),

            accent: Color32::from_rgb(0xff, 0xd7, 0x00),
            accent_hover: Color32::from_rgb(0xff, 0xff, 0x40),
            border: Color32::from_rgb(0x80, 0x80, 0x80),
            border_subtle: Color32::from_rgb(0x40, 0x40, 0x40),

            success: Color32::from_rgb(0x00, 0xff, 0x80),
            warning: Color32::from_rgb(0xff, 0xd7, 0x00),
            error: Color32::from_rgb(0xff, 0x40, 0x40),

            row_even: Color32::from_rgb(0x00, 0x00, 0x00),
            row_odd: Color32::from_rgb(0x16, 0x16, 0x16),
            row_number_bg: Color32::from_rgb(0x10, 0x10, 0x10),
            row_number_text: Color32::from_rgb(0xb0, 0xb0, 0xb0),

            scrollbar_track: Color32::from_rgb(0x18, 0x18, 0x18),
            scrollbar_thumb: Color32::from_rgb(0x60, 0x60, 0x60),
            scrollbar_thumb_hover: Color32::from_rgb(0xa0, 0xa0, 0xa0),
        }
    }

    /// Bright manga / shōnen-page palette: cream paper, ink-black text,
    /// sakura pink primary accent, sky blue hover, with warm peach hovers
    /// and cherry-red selection — a colorful comic feel without sacrificing
    /// readability.
    fn manga() -> Self {
        Self {
            // Warm cream "paper" backgrounds.
            bg_primary: Color32::from_rgb(0xfb, 0xf7, 0xee),
            bg_secondary: Color32::from_rgb(0xf5, 0xef, 0xde),
            bg_tertiary: Color32::from_rgb(0xeb, 0xe4, 0xd2),
            bg_header: Color32::from_rgb(0xfc, 0xe2, 0xe7),
            bg_selected: Color32::from_rgba_unmultiplied(0xff, 0x4f, 0x9a, 90),
            bg_hover: Color32::from_rgb(0xfd, 0xe4, 0xc8),
            bg_edited: Color32::from_rgb(0xff, 0xe9, 0xc0),

            // Strong ink contrast like manga line work.
            text_primary: Color32::from_rgb(0x1c, 0x19, 0x17),
            text_secondary: Color32::from_rgb(0x3f, 0x3a, 0x36),
            text_muted: Color32::from_rgb(0x7a, 0x71, 0x6a),
            text_header: Color32::from_rgb(0xc2, 0x18, 0x5b),

            // Cherry-blossom pink primary, sky-blue secondary.
            accent: Color32::from_rgb(0xe9, 0x1e, 0x63),
            accent_hover: Color32::from_rgb(0x03, 0xa9, 0xf4),
            border: Color32::from_rgb(0xcd, 0xb9, 0x97),
            border_subtle: Color32::from_rgb(0xe7, 0xd8, 0xb8),

            success: Color32::from_rgb(0x4c, 0xaf, 0x50),
            warning: Color32::from_rgb(0xff, 0x98, 0x00),
            error: Color32::from_rgb(0xd5, 0x00, 0x00),

            // Subtle alternation; the second band picks up a peach tint.
            row_even: Color32::from_rgb(0xfb, 0xf7, 0xee),
            row_odd: Color32::from_rgb(0xfc, 0xee, 0xd9),
            row_number_bg: Color32::from_rgb(0xfc, 0xe2, 0xe7),
            row_number_text: Color32::from_rgb(0x88, 0x32, 0x60),

            scrollbar_track: Color32::from_rgb(0xeb, 0xe4, 0xd2),
            scrollbar_thumb: Color32::from_rgb(0xd8, 0xa6, 0xb6),
            scrollbar_thumb_hover: Color32::from_rgb(0xc2, 0x18, 0x5b),
        }
    }

    /// Gentleman palette: deep walnut and burgundy, parchment text, with
    /// champagne-gold accents — a refined, library-after-dark feel.
    fn gentleman() -> Self {
        Self {
            // Warm dark walnut backgrounds with a hint of leather.
            bg_primary: Color32::from_rgb(0x1a, 0x14, 0x10),
            bg_secondary: Color32::from_rgb(0x23, 0x1b, 0x15),
            bg_tertiary: Color32::from_rgb(0x2d, 0x24, 0x19),
            bg_header: Color32::from_rgb(0x2a, 0x18, 0x18),
            bg_selected: Color32::from_rgba_unmultiplied(0xc8, 0x9b, 0x3c, 110),
            bg_hover: Color32::from_rgb(0x3a, 0x2c, 0x1e),
            bg_edited: Color32::from_rgb(0x3d, 0x2e, 0x16),

            // Aged parchment text with champagne-gold for headings.
            text_primary: Color32::from_rgb(0xf0, 0xe6, 0xd2),
            text_secondary: Color32::from_rgb(0xc9, 0xb8, 0x96),
            text_muted: Color32::from_rgb(0x8a, 0x7a, 0x5e),
            text_header: Color32::from_rgb(0xd4, 0xa8, 0x5c),

            accent: Color32::from_rgb(0xc8, 0x9b, 0x3c),
            accent_hover: Color32::from_rgb(0xe5, 0xb8, 0x5f),
            border: Color32::from_rgb(0x5e, 0x4a, 0x30),
            border_subtle: Color32::from_rgb(0x3a, 0x2c, 0x1e),

            success: Color32::from_rgb(0x3e, 0x8e, 0x6e),
            warning: Color32::from_rgb(0xd4, 0xa8, 0x5c),
            error: Color32::from_rgb(0xa8, 0x30, 0x2d),

            row_even: Color32::from_rgb(0x1a, 0x14, 0x10),
            row_odd: Color32::from_rgb(0x21, 0x1a, 0x14),
            row_number_bg: Color32::from_rgb(0x23, 0x1b, 0x15),
            row_number_text: Color32::from_rgb(0x8a, 0x7a, 0x5e),

            scrollbar_track: Color32::from_rgb(0x23, 0x1b, 0x15),
            scrollbar_thumb: Color32::from_rgb(0x5e, 0x4a, 0x30),
            scrollbar_thumb_hover: Color32::from_rgb(0xc8, 0x9b, 0x3c),
        }
    }
}

/// Apply the theme to an egui context.
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode, font: FontSettings) {
    let colors = ThemeColors::for_mode(mode);
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
    // accent stroke so the whole hit-target lights up — the previous bg_hover
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
    visuals.selection.stroke = Stroke::new(1.0, colors.accent);

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
    // swap. Keep edits minimal — anything done here ripples across every view.
    apply_theme_decoration(&mut style, mode, &colors);

    ctx.set_style(style);
}

/// Apply per-theme structural tweaks (corner_radius, button padding, stroke
/// widths, hover expansion) on top of the colorized base style. Light/Dark
/// keep the egui defaults.
///
/// IMPORTANT — egui aliases `RichText::strong()` text color to
/// `widgets.active.fg_stroke.color`. That color must be readable on **both**
/// the panel background (where strong text appears in headings, Settings
/// section titles, Markdown headings) and the active button fill. If the
/// active fill is too pale/saturated, darken it instead of brightening the
/// fg_stroke — otherwise strong text everywhere becomes invisible.
fn apply_theme_decoration(style: &mut Style, mode: ThemeMode, colors: &ThemeColors) {
    match mode {
        ThemeMode::Light | ThemeMode::Dark => {}
        ThemeMode::Manga => apply_manga_decoration(style, colors),
        ThemeMode::Nord => apply_nord_decoration(style, colors),
        ThemeMode::Dracula => apply_dracula_decoration(style, colors),
        ThemeMode::GruvboxDark => apply_gruvbox_decoration(style, colors),
        ThemeMode::HighContrast => apply_high_contrast_decoration(style, colors),
        ThemeMode::Gentleman => apply_gentleman_decoration(style, colors),
    }
}

fn apply_manga_decoration(style: &mut Style, colors: &ThemeColors) {
    // "Speech bubble" buttons: pure white fill, thick ink border, very rounded.
    // Hover/active flip to sakura pink with **ink** text — ink-on-pink stays
    // legible and doubles as a workable strong-text color on the cream panel.
    let radius = CornerRadius::same(12);
    let ink = colors.text_primary;
    let ink_border = Stroke::new(2.0, ink);
    let ink_thick = Stroke::new(2.5, ink);
    let paper_white = Color32::from_rgb(0xff, 0xff, 0xff);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_fill = colors.bg_secondary;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = paper_white;
    v.widgets.inactive.weak_bg_fill = paper_white;
    v.widgets.inactive.bg_stroke = ink_thick;
    v.widgets.inactive.fg_stroke = Stroke::new(1.5, ink);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent;
    v.widgets.hovered.weak_bg_fill = colors.accent;
    v.widgets.hovered.bg_stroke = ink_thick;
    v.widgets.hovered.fg_stroke = Stroke::new(2.0, ink);
    v.widgets.hovered.expansion = 3.0;

    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_fill = colors.accent.linear_multiply(0.82);
    v.widgets.active.weak_bg_fill = colors.accent.linear_multiply(0.82);
    v.widgets.active.bg_stroke = ink_thick;
    // Ink so RichText::strong() stays readable on cream panel bg.
    v.widgets.active.fg_stroke = Stroke::new(2.0, ink);
    v.widgets.active.expansion = 1.0;

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.45);
    v.widgets.open.weak_bg_fill = colors.accent.linear_multiply(0.45);
    v.widgets.open.bg_stroke = ink_border;
    v.widgets.open.fg_stroke = Stroke::new(2.0, ink);

    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = ink_border;

    v.selection.bg_fill = colors.accent.linear_multiply(0.55);
    v.selection.stroke = ink_border;

    // Warm cream code background instead of the default cool gray.
    v.code_bg_color = Color32::from_rgb(0xff, 0xed, 0xd9);

    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.item_spacing = egui::vec2(10.0, 6.0);
}

fn apply_nord_decoration(style: &mut Style, colors: &ThemeColors) {
    // Frosted-glass Scandinavian panels: gently rounded, thin frost-blue
    // borders, soft hover halo. Less aggressive than Manga — Nord is about
    // calm minimalism, not pop.
    let radius = CornerRadius::same(8);
    let frost = Color32::from_rgb(0xd8, 0xde, 0xe9); // snow text color
    let ice_border = Stroke::new(1.0, colors.border);
    let aurora_border = Stroke::new(1.5, colors.accent);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = ice_border;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = ice_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, frost);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = aurora_border;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, frost);
    v.widgets.hovered.expansion = 1.0;

    v.widgets.active.corner_radius = radius;
    // Darker shade of Nord frost so white text reads on it.
    v.widgets.active.bg_fill = Color32::from_rgb(0x4d, 0x80, 0x8e);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x4d, 0x80, 0x8e);
    v.widgets.active.bg_stroke = aurora_border;
    v.widgets.active.fg_stroke = Stroke::new(1.5, frost);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = aurora_border;

    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

fn apply_dracula_decoration(style: &mut Style, colors: &ThemeColors) {
    // Cyber-gothic: sharp 4px corners, neon purple borders, glowing pink
    // hover. Thin, cold, edgy — feels like a terminal in a vampire club.
    let radius = CornerRadius::same(4);
    let neon_border = Stroke::new(1.5, colors.accent);
    let pink_border = Stroke::new(2.0, colors.accent_hover);
    let snow = Color32::from_rgb(0xf8, 0xf8, 0xf2);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.border);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = neon_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, snow);

    v.widgets.hovered.corner_radius = radius;
    // Glowing neon-pink hover.
    v.widgets.hovered.bg_fill = colors.accent_hover.linear_multiply(0.5);
    v.widgets.hovered.weak_bg_fill = colors.accent_hover.linear_multiply(0.5);
    v.widgets.hovered.bg_stroke = pink_border;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, snow);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Darker purple so snow text reads strongly (and so does strong text).
    v.widgets.active.bg_fill = Color32::from_rgb(0x6b, 0x4d, 0x9c);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x6b, 0x4d, 0x9c);
    v.widgets.active.bg_stroke = pink_border;
    v.widgets.active.fg_stroke = Stroke::new(1.5, snow);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.5);
    v.widgets.open.bg_stroke = neon_border;

    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

fn apply_gruvbox_decoration(style: &mut Style, colors: &ThemeColors) {
    // Retro terminal warmth: 2px chunky amber borders, soft rounded corners,
    // warm hover. Feels like a 90s text-mode UI.
    let radius = CornerRadius::same(5);
    let amber_border = Stroke::new(1.5, colors.border);
    let amber_strong = Stroke::new(2.0, colors.accent);
    let cream = Color32::from_rgb(0xfb, 0xf1, 0xc7);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = amber_border;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = amber_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, cream);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = amber_strong;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, cream);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Burnt-orange — light text reads well, plus this is the strong-text
    // color so it must read on dark panel bg too (cream-ish does).
    v.widgets.active.bg_fill = Color32::from_rgb(0xaf, 0x6f, 0x1c);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0xaf, 0x6f, 0x1c);
    v.widgets.active.bg_stroke = amber_strong;
    v.widgets.active.fg_stroke = Stroke::new(1.5, cream);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = amber_strong;

    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

fn apply_high_contrast_decoration(style: &mut Style, colors: &ThemeColors) {
    // Sharp, no-nonsense: zero corner radius, thick borders, no expansion
    // (motion is distracting in an accessibility theme).
    let radius = CornerRadius::same(0);
    let gold_border = Stroke::new(2.0, colors.accent);
    let gold_thick = Stroke::new(3.0, colors.accent);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.5, colors.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.5, Color32::WHITE);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_secondary;
    v.widgets.inactive.weak_bg_fill = colors.bg_secondary;
    v.widgets.inactive.bg_stroke = gold_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.5, Color32::WHITE);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent;
    v.widgets.hovered.weak_bg_fill = colors.accent;
    v.widgets.hovered.bg_stroke = gold_thick;
    v.widgets.hovered.fg_stroke = Stroke::new(2.0, Color32::BLACK);
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_fill = colors.accent;
    v.widgets.active.weak_bg_fill = colors.accent;
    v.widgets.active.bg_stroke = gold_thick;
    // Black on gold: max contrast for both pressed buttons and strong text.
    // (Strong text appears on gold-yellow Heading panels in this theme,
    // matching `text_header` already.)
    v.widgets.active.fg_stroke = Stroke::new(2.0, Color32::BLACK);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = gold_thick;

    v.window_corner_radius = CornerRadius::same(0);
    v.menu_corner_radius = CornerRadius::same(0);
    v.window_stroke = gold_border;

    v.selection.bg_fill = colors.bg_selected;
    v.selection.stroke = gold_thick;

    style.spacing.button_padding = egui::vec2(12.0, 6.0);
}

fn apply_gentleman_decoration(style: &mut Style, colors: &ThemeColors) {
    // Refined library / smoking-room: pill-shaped buttons, thin gold borders,
    // warm gold-glow hover. Old-world, restrained, never garish.
    let radius = CornerRadius::same(10);
    let gold_thin = Stroke::new(1.0, colors.border);
    let gold_strong = Stroke::new(1.5, colors.accent);
    let parchment = colors.text_primary;

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = gold_thin;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, colors.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, parchment);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = gold_strong;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, parchment);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Deep walnut accent — gold text reads on it, and gold is the strong-text
    // color so it works on dark walnut panel bg too.
    v.widgets.active.bg_fill = Color32::from_rgb(0x4a, 0x35, 0x1f);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x4a, 0x35, 0x1f);
    v.widgets.active.bg_stroke = gold_strong;
    v.widgets.active.fg_stroke = Stroke::new(1.5, colors.accent);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = gold_strong;

    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = gold_strong;

    style.spacing.button_padding = egui::vec2(12.0, 6.0);
}

/// Paint the per-theme background decoration onto `painter` clipped to `rect`.
/// Called by the central panel before rendering content, so widgets sit on
/// top. Themes without a decoration are a no-op — this is the *only* place
/// background graphics live; it keeps the renderer thin and theme-aware.
pub fn paint_background_decoration(
    painter: &egui::Painter,
    rect: egui::Rect,
    mode: ThemeMode,
) {
    match mode {
        ThemeMode::Manga => paint_manga_background(painter, rect),
        ThemeMode::Nord => paint_nord_background(painter, rect),
        ThemeMode::Dracula => paint_dracula_background(painter, rect),
        ThemeMode::GruvboxDark => paint_gruvbox_background(painter, rect),
        ThemeMode::Gentleman => paint_gentleman_background(painter, rect),
        ThemeMode::Light | ThemeMode::Dark | ThemeMode::HighContrast => {}
    }
}

fn paint_manga_background(painter: &egui::Painter, rect: egui::Rect) {
    // Layer 1: halftone screentone. Triangular lattice (offset every other
    // row) of small ink dots — classic manga screen-tone texture. Stronger
    // than a wallpaper hint; still pales out behind text.
    const STEP: f32 = 16.0;
    const RADIUS: f32 = 1.4;
    let dot_color = Color32::from_rgba_unmultiplied(50, 25, 45, 32);

    let mut row_idx = 0;
    let mut y = rect.top();
    while y < rect.bottom() {
        let x_offset = if row_idx % 2 == 0 { 0.0 } else { STEP * 0.5 };
        let mut x = rect.left() + x_offset;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), RADIUS, dot_color);
            x += STEP;
        }
        y += STEP * 0.866; // sin(60°) — equilateral spacing.
        row_idx += 1;
    }

    // Layer 2: speed-line burst from the top-right corner — the "action
    // panel" cue. Lines fan into a quadrant pointing toward the bottom-left,
    // tapering off well before content density is hit. Very faint so it
    // reads as decoration, not noise.
    let origin = egui::pos2(rect.right(), rect.top());
    let line_color = Color32::from_rgba_unmultiplied(50, 25, 45, 20);
    let max_len = rect.width().max(rect.height()) * 0.9;
    let n_lines = 22;
    for i in 0..n_lines {
        let t = i as f32 / (n_lines - 1).max(1) as f32;
        // Quadrant from straight-down (π/2) to straight-left (π).
        let angle = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::FRAC_PI_2;
        // Vary length per line so the burst looks hand-drawn, not radial.
        let len_jitter = 0.55 + 0.45 * ((i * 7) % 10) as f32 / 10.0;
        let len = max_len * len_jitter;
        let end = origin + egui::vec2(angle.cos() * len, angle.sin() * len);
        // Slightly thicker at the origin, thinner at the tip — emulate brush
        // taper with a single straight segment by varying stroke width per
        // line index.
        let width = 0.6 + 0.4 * (1.0 - t);
        painter.line_segment([origin, end], Stroke::new(width, line_color));
    }
}

fn paint_nord_background(painter: &egui::Painter, rect: egui::Rect) {
    // Aurora bands: three soft horizontal stripes at varying alpha — a nod to
    // Nord's polar inspiration without obscuring content. The bands fade in
    // and out via piecewise-linear alpha so the seam isn't a hard line.
    let bands = [
        (0.18_f32, Color32::from_rgba_unmultiplied(0x88, 0xc0, 0xd0, 14)),
        (0.55_f32, Color32::from_rgba_unmultiplied(0x8f, 0xbc, 0xbb, 12)),
        (0.82_f32, Color32::from_rgba_unmultiplied(0x5e, 0x81, 0xac, 14)),
    ];
    let height = rect.height();
    let band_h = height * 0.22;
    for (center_t, color) in bands {
        let cy = rect.top() + height * center_t;
        // Vertical alpha falloff: paint the band as a stack of thin lines.
        let n = 24;
        for i in 0..n {
            let dt = (i as f32 / n as f32 - 0.5) * 2.0; // -1..1
            let y = cy + dt * band_h * 0.5;
            let alpha_factor = 1.0 - dt.abs();
            let faded = Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                (color.a() as f32 * alpha_factor) as u8,
            );
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(band_h / n as f32 + 0.5, faded),
            );
        }
    }
}

fn paint_dracula_background(painter: &egui::Painter, rect: egui::Rect) {
    // CRT scanlines + faint vertical neon glow on the right edge — a slightly
    // cyberpunk vibe without becoming visual noise.
    let line_color = Color32::from_rgba_unmultiplied(0x0a, 0x06, 0x10, 120);
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, line_color),
        );
        y += 3.0;
    }

    // Right-edge neon glow (purple → pink).
    let glow_w = 60.0_f32.min(rect.width() * 0.25);
    let n = 30;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let x = rect.right() - glow_w * t;
        let alpha = ((1.0 - t).powf(2.0) * 22.0) as u8;
        let color = Color32::from_rgba_unmultiplied(0xbd, 0x70, 0xc6, alpha);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(glow_w / n as f32 + 0.5, color),
        );
    }
}

fn paint_gruvbox_background(painter: &egui::Painter, rect: egui::Rect) {
    // Faint warm-amber film grain on a dark backdrop. A pseudo-random dot
    // field keyed off a deterministic LCG so the pattern stays stable across
    // frames (no shimmering). The seed is fixed, not time-based, on purpose.
    let mut seed: u32 = 0x9e37_79b1;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed
    };
    let area = rect.width() * rect.height();
    // ~ one grain per 280 px².
    let count = (area / 280.0) as usize;
    let dot_color = Color32::from_rgba_unmultiplied(0xd6, 0x5d, 0x0e, 22);
    for _ in 0..count {
        let r1 = next();
        let r2 = next();
        let x = rect.left() + (r1 % 10_000) as f32 / 10_000.0 * rect.width();
        let y = rect.top() + (r2 % 10_000) as f32 / 10_000.0 * rect.height();
        painter.circle_filled(egui::pos2(x, y), 0.9, dot_color);
    }
}

fn paint_gentleman_background(painter: &egui::Painter, rect: egui::Rect) {
    // Diamond / lozenge lattice in faint champagne gold — old-world wallpaper
    // without competing for the reader's attention. Diagonals only, drawn at
    // wide spacing so the eye reads "pattern" not "noise".
    let line_color = Color32::from_rgba_unmultiplied(0xc8, 0x9b, 0x3c, 14);
    let stroke = Stroke::new(0.7, line_color);
    let step = 36.0_f32;
    // Down-right diagonals.
    let mut start = rect.left() - rect.height();
    let end = rect.right() + rect.height();
    while start < end {
        let p1 = egui::pos2(start, rect.top());
        let p2 = egui::pos2(start + rect.height(), rect.bottom());
        painter.line_segment([p1, p2], stroke);
        start += step;
    }
    // Down-left diagonals — same spacing, opposite slope, completes the
    // lozenge mesh.
    let mut start = rect.left() - rect.height();
    while start < end {
        let p1 = egui::pos2(start + rect.height(), rect.top());
        let p2 = egui::pos2(start, rect.bottom());
        painter.line_segment([p1, p2], stroke);
        start += step;
    }
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
    if let Some(path) = font.custom_path.filter(|p| !p.is_empty()) {
        if let Ok(bytes) = std::fs::read(path) {
            defs.font_data
                .insert("custom".into(), Arc::new(FontData::from_owned(bytes)));
            // Custom font becomes a named family that style maps to Body.
            defs.families
                .insert(FontFamily::Name(Arc::from("custom")), vec!["custom".into()]);
        }
    }
    ctx.set_fonts(defs);
}
