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

    ctx.set_style(style);
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
