use crate::data::MarkColor;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn toggle(&self) -> Self {
        match self {
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            ThemeMode::Light => "🌙",
            ThemeMode::Dark => "☀",
        }
    }
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
            MarkColor::Red => Color32::from_rgba_unmultiplied(239, 68, 68, 80),
            MarkColor::Orange => Color32::from_rgba_unmultiplied(255, 140, 0, 80),
            MarkColor::Yellow => Color32::from_rgba_unmultiplied(234, 179, 8, 80),
            MarkColor::Green => Color32::from_rgba_unmultiplied(34, 197, 94, 80),
            MarkColor::Blue => Color32::from_rgba_unmultiplied(59, 130, 246, 80),
            MarkColor::Purple => Color32::from_rgba_unmultiplied(168, 85, 247, 80),
        }
    }

    /// Get a solid color swatch for the color picker.
    pub fn mark_swatch(mark: MarkColor) -> Color32 {
        match mark {
            MarkColor::Red => Color32::from_rgb(239, 68, 68),
            MarkColor::Orange => Color32::from_rgb(249, 115, 22),
            MarkColor::Yellow => Color32::from_rgb(234, 179, 8),
            MarkColor::Green => Color32::from_rgb(34, 197, 94),
            MarkColor::Blue => Color32::from_rgb(59, 130, 246),
            MarkColor::Purple => Color32::from_rgb(168, 85, 247),
        }
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }

    fn dark() -> Self {
        Self {
            bg_primary: Color32::from_rgb(24, 24, 27),   // zinc-900
            bg_secondary: Color32::from_rgb(39, 39, 42), // zinc-800
            bg_tertiary: Color32::from_rgb(52, 52, 56),  // zinc-700
            bg_header: Color32::from_rgb(30, 30, 35),
            bg_selected: Color32::from_rgba_unmultiplied(99, 102, 241, 40), // indigo-500 subtle
            bg_hover: Color32::from_rgb(45, 45, 50),
            bg_edited: Color32::from_rgb(50, 40, 20),

            text_primary: Color32::from_rgb(244, 244, 245), // zinc-100
            text_secondary: Color32::from_rgb(161, 161, 170), // zinc-400
            text_muted: Color32::from_rgb(113, 113, 122),   // zinc-500
            text_header: Color32::from_rgb(228, 228, 231),  // zinc-200

            accent: Color32::from_rgb(99, 102, 241), // indigo-500
            accent_hover: Color32::from_rgb(129, 140, 248), // indigo-400
            border: Color32::from_rgb(63, 63, 70),   // zinc-700
            border_subtle: Color32::from_rgb(39, 39, 42), // zinc-800

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
            bg_secondary: Color32::from_rgb(249, 250, 251), // gray-50
            bg_tertiary: Color32::from_rgb(243, 244, 246),  // gray-100
            bg_header: Color32::from_rgb(248, 248, 252),
            bg_selected: Color32::from_rgb(219, 234, 254), // blue-100
            bg_hover: Color32::from_rgb(243, 244, 246),
            bg_edited: Color32::from_rgb(255, 249, 219),

            text_primary: Color32::from_rgb(17, 24, 39), // gray-900
            text_secondary: Color32::from_rgb(107, 114, 128), // gray-500
            text_muted: Color32::from_rgb(156, 163, 175), // gray-400
            text_header: Color32::from_rgb(31, 41, 55),  // gray-800

            accent: Color32::from_rgb(79, 70, 229), // indigo-600
            accent_hover: Color32::from_rgb(99, 102, 241), // indigo-500
            border: Color32::from_rgb(229, 231, 235), // gray-200
            border_subtle: Color32::from_rgb(243, 244, 246), // gray-100

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
}

/// Apply the theme to an egui context
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode, font_size: f32) {
    let colors = ThemeColors::for_mode(mode);

    let mut style = Style::default();

    // Set visuals based on mode
    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    // Override specific colors
    visuals.window_fill = colors.bg_primary;
    visuals.panel_fill = colors.bg_primary;
    visuals.extreme_bg_color = match mode {
        ThemeMode::Dark => colors.bg_secondary,
        ThemeMode::Light => Color32::from_rgb(230, 233, 240),
    };
    visuals.faint_bg_color = match mode {
        ThemeMode::Dark => colors.bg_tertiary,
        ThemeMode::Light => Color32::from_rgb(237, 240, 245),
    };
    visuals.window_stroke = Stroke::new(1.0, colors.border);

    // Widget visuals
    visuals.widgets.noninteractive.bg_fill = colors.bg_secondary;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text_primary);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, colors.border);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = colors.bg_secondary;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, colors.text_primary);
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, colors.border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = colors.bg_hover;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, colors.text_primary);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, colors.accent);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = colors.accent;
    visuals.widgets.active.fg_stroke = Stroke::new(
        1.0,
        match mode {
            ThemeMode::Dark => Color32::WHITE,
            ThemeMode::Light => colors.text_primary,
        },
    );
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, colors.accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.selection.bg_fill = colors.bg_selected;
    visuals.selection.stroke = Stroke::new(1.0, colors.accent);

    // Ensure strong text, hyperlinks, and code have good contrast in both modes
    visuals.hyperlink_color = colors.accent;
    visuals.warn_fg_color = colors.warning;
    visuals.error_fg_color = colors.error;
    visuals.code_bg_color = match mode {
        ThemeMode::Dark => Color32::from_rgb(40, 40, 48),
        ThemeMode::Light => Color32::from_rgb(230, 233, 240),
    };
    visuals.override_text_color = None;

    style.visuals = visuals;

    // Font sizes — scaled from the user-configured base size
    let small = (font_size * 0.85).round();
    let heading = (font_size * 1.38).round();
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(small, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(font_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(font_size, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(font_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(heading, FontFamily::Proportional),
        ),
    ]
    .into();

    // Spacing
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);

    ctx.set_style(style);
}
