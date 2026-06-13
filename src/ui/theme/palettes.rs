//! Per-theme [`ThemeColors`] palette tables. Each function returns a fully
//! populated `ThemeColors` value for one [`ThemeMode`] variant. The dispatcher
//! [`super::ThemeColors::for_mode`] picks the right builder.

use egui::Color32;

use super::ThemeColors;

impl ThemeColors {
    pub(super) fn dark() -> Self {
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

    pub(super) fn light() -> Self {
        Self {
            bg_primary: Color32::from_rgb(255, 255, 255),
            bg_secondary: Color32::from_rgb(244, 246, 250),
            bg_tertiary: Color32::from_rgb(232, 236, 244),
            bg_header: Color32::from_rgb(218, 224, 238),
            bg_selected: Color32::from_rgb(191, 219, 254),
            bg_hover: Color32::from_rgb(232, 236, 244),
            bg_edited: Color32::from_rgb(255, 249, 219),

            text_primary: Color32::from_rgb(7, 12, 22),
            text_secondary: Color32::from_rgb(45, 52, 66),
            text_muted: Color32::from_rgb(120, 128, 142),
            text_header: Color32::from_rgb(15, 22, 35),

            accent: Color32::from_rgb(79, 70, 229),
            accent_hover: Color32::from_rgb(99, 102, 241),
            border: Color32::from_rgb(200, 207, 220),
            border_subtle: Color32::from_rgb(218, 224, 238),

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

    pub(super) fn north() -> Self {
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

    pub(super) fn dracula() -> Self {
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

    pub(super) fn gruvbox_dark() -> Self {
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

    pub(super) fn high_contrast() -> Self {
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
    /// and cherry-red selection - a colorful comic feel without sacrificing
    /// readability.
    pub(super) fn manga() -> Self {
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
    /// champagne-gold accents - a refined, library-after-dark feel.
    pub(super) fn gentleman() -> Self {
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

    /// Deep Sea: a Nord-inspired palette pushed deeper and bluer. Abyssal navy
    /// backgrounds, lagoon-blue accents, near-white text. Reads like Nord at
    /// 200 m depth.
    pub(super) fn deep_sea() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0x0e, 0x1f, 0x2e),
            bg_secondary: Color32::from_rgb(0x14, 0x2a, 0x3f),
            bg_tertiary: Color32::from_rgb(0x1c, 0x36, 0x52),
            bg_header: Color32::from_rgb(0x10, 0x25, 0x38),
            bg_selected: Color32::from_rgba_unmultiplied(0x5f, 0xb1, 0xd4, 110),
            bg_hover: Color32::from_rgb(0x22, 0x3f, 0x5e),
            bg_edited: Color32::from_rgb(0x2e, 0x36, 0x1a),

            text_primary: Color32::from_rgb(0xe6, 0xf0, 0xf7),
            text_secondary: Color32::from_rgb(0xb6, 0xc8, 0xd8),
            text_muted: Color32::from_rgb(0x6f, 0x8a, 0xa1),
            text_header: Color32::from_rgb(0x8f, 0xc6, 0xe2),

            accent: Color32::from_rgb(0x3a, 0x8f, 0xb7),
            accent_hover: Color32::from_rgb(0x5f, 0xb1, 0xd4),
            border: Color32::from_rgb(0x1c, 0x36, 0x52),
            border_subtle: Color32::from_rgb(0x14, 0x2a, 0x3f),

            success: Color32::from_rgb(0x6e, 0xc4, 0xa8),
            warning: Color32::from_rgb(0xe7, 0xc7, 0x6f),
            error: Color32::from_rgb(0xd0, 0x6a, 0x6a),

            row_even: Color32::from_rgb(0x0e, 0x1f, 0x2e),
            row_odd: Color32::from_rgb(0x12, 0x26, 0x38),
            row_number_bg: Color32::from_rgb(0x10, 0x25, 0x38),
            row_number_text: Color32::from_rgb(0x6f, 0x8a, 0xa1),

            scrollbar_track: Color32::from_rgb(0x14, 0x2a, 0x3f),
            scrollbar_thumb: Color32::from_rgb(0x22, 0x3f, 0x5e),
            scrollbar_thumb_hover: Color32::from_rgb(0x5f, 0xb1, 0xd4),
        }
    }

    /// Frost: cool, near-white "snowfield" background with pale ice-blue
    /// accents and dark slate text. The light counterpart to Deep Sea.
    pub(super) fn frost() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0xf4, 0xf8, 0xfb),
            bg_secondary: Color32::from_rgb(0xea, 0xf2, 0xf8),
            bg_tertiary: Color32::from_rgb(0xdc, 0xe9, 0xf2),
            bg_header: Color32::from_rgb(0xe3, 0xee, 0xf6),
            bg_selected: Color32::from_rgb(0xbf, 0xdc, 0xee),
            bg_hover: Color32::from_rgb(0xd6, 0xe6, 0xf0),
            bg_edited: Color32::from_rgb(0xff, 0xf3, 0xc4),

            text_primary: Color32::from_rgb(0x1f, 0x29, 0x33),
            text_secondary: Color32::from_rgb(0x4a, 0x5b, 0x6c),
            text_muted: Color32::from_rgb(0x8a, 0x9b, 0xa9),
            text_header: Color32::from_rgb(0x29, 0x4f, 0x6b),

            accent: Color32::from_rgb(0x4d, 0x95, 0xb8),
            accent_hover: Color32::from_rgb(0x7f, 0xb4, 0xd4),
            border: Color32::from_rgb(0xc6, 0xd6, 0xe2),
            border_subtle: Color32::from_rgb(0xdc, 0xe9, 0xf2),

            success: Color32::from_rgb(0x46, 0x99, 0x70),
            warning: Color32::from_rgb(0xb8, 0x84, 0x1f),
            error: Color32::from_rgb(0xb6, 0x3a, 0x3a),

            row_even: Color32::from_rgb(0xf4, 0xf8, 0xfb),
            row_odd: Color32::from_rgb(0xe6, 0xee, 0xf5),
            row_number_bg: Color32::from_rgb(0xea, 0xf2, 0xf8),
            row_number_text: Color32::from_rgb(0x8a, 0x9b, 0xa9),

            scrollbar_track: Color32::from_rgb(0xdc, 0xe9, 0xf2),
            scrollbar_thumb: Color32::from_rgb(0xb6, 0xc9, 0xd7),
            scrollbar_thumb_hover: Color32::from_rgb(0x7f, 0xb4, 0xd4),
        }
    }

    /// Warm: a friendly light theme. Cream and blush backgrounds, soft
    /// rose/mauve accents, dark plum text. A warm counterweight to the cooler
    /// presets, kept high-contrast enough to stay readable.
    pub(super) fn warm() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0xff, 0xf8, 0xf6),
            bg_secondary: Color32::from_rgb(0xfc, 0xef, 0xec),
            bg_tertiary: Color32::from_rgb(0xf7, 0xe3, 0xde),
            bg_header: Color32::from_rgb(0xfb, 0xe4, 0xe7),
            bg_selected: Color32::from_rgb(0xf2, 0xcc, 0xda),
            bg_hover: Color32::from_rgb(0xfb, 0xe7, 0xdd),
            bg_edited: Color32::from_rgb(0xff, 0xf1, 0xcc),

            text_primary: Color32::from_rgb(0x3a, 0x22, 0x30),
            text_secondary: Color32::from_rgb(0x6e, 0x4a, 0x5c),
            text_muted: Color32::from_rgb(0xa9, 0x87, 0x94),
            text_header: Color32::from_rgb(0xb1, 0x4a, 0x77),

            accent: Color32::from_rgb(0xc2, 0x5b, 0x8b),
            accent_hover: Color32::from_rgb(0xd7, 0x7b, 0xa6),
            border: Color32::from_rgb(0xe9, 0xcb, 0xd3),
            border_subtle: Color32::from_rgb(0xf4, 0xdd, 0xe2),

            success: Color32::from_rgb(0x4f, 0xa5, 0x72),
            warning: Color32::from_rgb(0xe0, 0x96, 0x2f),
            error: Color32::from_rgb(0xc7, 0x40, 0x5a),

            row_even: Color32::from_rgb(0xff, 0xf8, 0xf6),
            row_odd: Color32::from_rgb(0xfc, 0xea, 0xe6),
            row_number_bg: Color32::from_rgb(0xfb, 0xe4, 0xe7),
            row_number_text: Color32::from_rgb(0xb0, 0x7e, 0x8e),

            scrollbar_track: Color32::from_rgb(0xf7, 0xe3, 0xde),
            scrollbar_thumb: Color32::from_rgb(0xe6, 0xbf, 0xc9),
            scrollbar_thumb_hover: Color32::from_rgb(0xc2, 0x5b, 0x8b),
        }
    }

    /// Forest: a deep woodland dark theme. Dark forest-green canvas, mossy /
    /// leaf-green accents, warm bark tones for edits, and pale parchment text.
    pub(super) fn forest() -> Self {
        Self {
            bg_primary: Color32::from_rgb(0x14, 0x20, 0x1a),
            bg_secondary: Color32::from_rgb(0x1c, 0x2c, 0x23),
            bg_tertiary: Color32::from_rgb(0x26, 0x39, 0x2d),
            bg_header: Color32::from_rgb(0x18, 0x26, 0x1e),
            bg_selected: Color32::from_rgba_unmultiplied(0x6f, 0xbf, 0x73, 100),
            bg_hover: Color32::from_rgb(0x2e, 0x44, 0x35),
            bg_edited: Color32::from_rgb(0x3a, 0x33, 0x18),

            text_primary: Color32::from_rgb(0xe6, 0xef, 0xe2),
            text_secondary: Color32::from_rgb(0xb6, 0xc9, 0xb6),
            text_muted: Color32::from_rgb(0x7e, 0x94, 0x7e),
            text_header: Color32::from_rgb(0xa8, 0xd5, 0xa2),

            accent: Color32::from_rgb(0x6f, 0xbf, 0x73),
            accent_hover: Color32::from_rgb(0x8e, 0xd8, 0x8f),
            border: Color32::from_rgb(0x2e, 0x44, 0x35),
            border_subtle: Color32::from_rgb(0x1c, 0x2c, 0x23),

            success: Color32::from_rgb(0x7c, 0xc4, 0x6a),
            warning: Color32::from_rgb(0xd8, 0xa2, 0x3a),
            error: Color32::from_rgb(0xc7, 0x5a, 0x5a),

            row_even: Color32::from_rgb(0x14, 0x20, 0x1a),
            row_odd: Color32::from_rgb(0x18, 0x26, 0x1f),
            row_number_bg: Color32::from_rgb(0x18, 0x26, 0x1e),
            row_number_text: Color32::from_rgb(0x7e, 0x94, 0x7e),

            scrollbar_track: Color32::from_rgb(0x1c, 0x2c, 0x23),
            scrollbar_thumb: Color32::from_rgb(0x3a, 0x52, 0x40),
            scrollbar_thumb_hover: Color32::from_rgb(0x6f, 0xbf, 0x73),
        }
    }

    /// Hidden rainbow theme. Returns the Dark base palette with a placeholder
    /// accent - the live accent is rotated per-frame in
    /// [`super::apply_theme`] when the mode is `Rainbow`, so any caller that
    /// reads this palette directly only needs sane defaults for non-accent
    /// colors.
    pub(super) fn rainbow() -> Self {
        let mut base = Self::dark();
        base.accent = Color32::from_rgb(0xff, 0x00, 0x88);
        base.accent_hover = Color32::from_rgb(0x88, 0xff, 0xff);
        base
    }
}
