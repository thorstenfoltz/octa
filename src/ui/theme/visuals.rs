//! Per-theme egui [`Style`] tweaks and background painters.
//!
//! Split out of [`super`] purely to keep the theme code navigable. Two
//! dispatchers - [`apply_theme_decoration`] (style overrides) and
//! [`paint_background_decoration`] (background art) - both match on
//! [`ThemeMode`] and call into the per-theme builder below. Light / Dark /
//! Rainbow are decoration-free; the rest each pair an `apply_*_decoration`
//! style tweak with a matching `paint_*_background` painter.

use egui::{Color32, CornerRadius, Stroke, Style};

use super::{ThemeColors, ThemeMode};

/// Apply per-theme structural tweaks (corner_radius, button padding, stroke
/// widths, hover expansion) on top of the colorized base style. Light/Dark
/// keep the egui defaults.
///
/// IMPORTANT - egui aliases `RichText::strong()` text color to
/// `widgets.active.fg_stroke.color`. That color must be readable on **both**
/// the panel background (where strong text appears in headings, Settings
/// section titles, Markdown headings) and the active button fill. If the
/// active fill is too pale/saturated, darken it instead of brightening the
/// fg_stroke - otherwise strong text everywhere becomes invisible.
pub(super) fn apply_theme_decoration(style: &mut Style, mode: ThemeMode, colors: &ThemeColors) {
    match mode {
        ThemeMode::Light | ThemeMode::Dark | ThemeMode::Rainbow => {}
        ThemeMode::Manga => apply_manga_decoration(style, colors),
        ThemeMode::North => apply_north_decoration(style, colors),
        ThemeMode::Dracula => apply_dracula_decoration(style, colors),
        ThemeMode::GruvboxDark => apply_gruvbox_decoration(style, colors),
        ThemeMode::HighContrast => apply_high_contrast_decoration(style, colors),
        ThemeMode::Gentleman => apply_gentleman_decoration(style, colors),
        ThemeMode::DeepSea => apply_deep_sea_decoration(style, colors),
        ThemeMode::Frost => apply_frost_decoration(style, colors),
        // The Warm and Forest presets share one understated "soft" treatment:
        // gentle rounding and hairline borders, palette does the rest.
        ThemeMode::Warm | ThemeMode::Forest => apply_soft_decoration(style, colors),
    }
}

/// Understated decoration for the Warm / Forest presets: gentle 8px rounding,
/// hairline accent-tinted borders and a soft hover. Deliberately leaves
/// `fg_stroke`
/// colors to the base style so `RichText::strong()` (which aliases
/// `widgets.active.fg_stroke`) stays readable on every background.
fn apply_soft_decoration(style: &mut Style, colors: &ThemeColors) {
    let radius = CornerRadius::same(8);
    let hairline = Stroke::new(1.0_f32, colors.border);
    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = hairline;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_secondary;
    v.widgets.inactive.weak_bg_fill = colors.bg_secondary;
    v.widgets.inactive.bg_stroke = hairline;

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.18);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.18);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, colors.accent);
    v.widgets.hovered.expansion = 0.5;

    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, colors.accent);

    v.widgets.open.corner_radius = radius;

    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = hairline;

    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
}

fn apply_manga_decoration(style: &mut Style, colors: &ThemeColors) {
    // "Speech bubble" buttons: pure white fill, thick ink border, very rounded.
    // Hover/active flip to sakura pink with **ink** text - ink-on-pink stays
    // legible and doubles as a workable strong-text color on the cream panel.
    let radius = CornerRadius::same(12);
    let ink = colors.text_primary;
    let ink_border = Stroke::new(2.0_f32, ink);
    let ink_thick = Stroke::new(2.5_f32, ink);
    let paper_white = Color32::from_rgb(0xff, 0xff, 0xff);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_fill = colors.bg_secondary;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, ink);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = paper_white;
    v.widgets.inactive.weak_bg_fill = paper_white;
    v.widgets.inactive.bg_stroke = ink_thick;
    v.widgets.inactive.fg_stroke = Stroke::new(1.5_f32, ink);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent;
    v.widgets.hovered.weak_bg_fill = colors.accent;
    v.widgets.hovered.bg_stroke = ink_thick;
    v.widgets.hovered.fg_stroke = Stroke::new(2.0_f32, ink);
    v.widgets.hovered.expansion = 3.0;

    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_fill = colors.accent.linear_multiply(0.82);
    v.widgets.active.weak_bg_fill = colors.accent.linear_multiply(0.82);
    v.widgets.active.bg_stroke = ink_thick;
    // Ink so RichText::strong() stays readable on cream panel bg.
    v.widgets.active.fg_stroke = Stroke::new(2.0_f32, ink);
    v.widgets.active.expansion = 1.0;

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.45);
    v.widgets.open.weak_bg_fill = colors.accent.linear_multiply(0.45);
    v.widgets.open.bg_stroke = ink_border;
    v.widgets.open.fg_stroke = Stroke::new(2.0_f32, ink);

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

fn apply_north_decoration(style: &mut Style, colors: &ThemeColors) {
    // Frosted-glass Scandinavian panels: gently rounded, thin frost-blue
    // borders, soft hover halo. Less aggressive than Manga - North is about
    // calm minimalism, not pop.
    let radius = CornerRadius::same(8);
    let frost = Color32::from_rgb(0xd8, 0xde, 0xe9); // snow text color
    let ice_border = Stroke::new(1.0_f32, colors.border);
    let aurora_border = Stroke::new(1.5_f32, colors.accent);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = ice_border;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = ice_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, frost);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = aurora_border;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, frost);
    v.widgets.hovered.expansion = 1.0;

    v.widgets.active.corner_radius = radius;
    // Darker shade of Nord frost so white text reads on it.
    v.widgets.active.bg_fill = Color32::from_rgb(0x4d, 0x80, 0x8e);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x4d, 0x80, 0x8e);
    v.widgets.active.bg_stroke = aurora_border;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, frost);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = aurora_border;

    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

fn apply_dracula_decoration(style: &mut Style, colors: &ThemeColors) {
    // Cyber-gothic: sharp 4px corners, neon purple borders, glowing pink
    // hover. Thin, cold, edgy - feels like a terminal in a vampire club.
    let radius = CornerRadius::same(4);
    let neon_border = Stroke::new(1.5_f32, colors.accent);
    let pink_border = Stroke::new(2.0_f32, colors.accent_hover);
    let snow = Color32::from_rgb(0xf8, 0xf8, 0xf2);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.border);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = neon_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, snow);

    v.widgets.hovered.corner_radius = radius;
    // Glowing neon-pink hover.
    v.widgets.hovered.bg_fill = colors.accent_hover.linear_multiply(0.5);
    v.widgets.hovered.weak_bg_fill = colors.accent_hover.linear_multiply(0.5);
    v.widgets.hovered.bg_stroke = pink_border;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, snow);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Darker purple so snow text reads strongly (and so does strong text).
    v.widgets.active.bg_fill = Color32::from_rgb(0x6b, 0x4d, 0x9c);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x6b, 0x4d, 0x9c);
    v.widgets.active.bg_stroke = pink_border;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, snow);

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
    let amber_border = Stroke::new(1.5_f32, colors.border);
    let amber_strong = Stroke::new(2.0_f32, colors.accent);
    let cream = Color32::from_rgb(0xfb, 0xf1, 0xc7);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = amber_border;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = amber_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, cream);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = amber_strong;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, cream);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Burnt-orange - light text reads well, plus this is the strong-text
    // color so it must read on dark panel bg too (cream-ish does).
    v.widgets.active.bg_fill = Color32::from_rgb(0xaf, 0x6f, 0x1c);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0xaf, 0x6f, 0x1c);
    v.widgets.active.bg_stroke = amber_strong;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, cream);

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
    let gold_border = Stroke::new(2.0_f32, colors.accent);
    let gold_thick = Stroke::new(3.0_f32, colors.accent);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.5_f32, colors.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_secondary;
    v.widgets.inactive.weak_bg_fill = colors.bg_secondary;
    v.widgets.inactive.bg_stroke = gold_border;
    v.widgets.inactive.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent;
    v.widgets.hovered.weak_bg_fill = colors.accent;
    v.widgets.hovered.bg_stroke = gold_thick;
    v.widgets.hovered.fg_stroke = Stroke::new(2.0_f32, Color32::BLACK);
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_fill = colors.accent;
    v.widgets.active.weak_bg_fill = colors.accent;
    v.widgets.active.bg_stroke = gold_thick;
    // Black on gold: max contrast for both pressed buttons and strong text.
    // (Strong text appears on gold-yellow Heading panels in this theme,
    // matching `text_header` already.)
    v.widgets.active.fg_stroke = Stroke::new(2.0_f32, Color32::BLACK);

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
    let gold_thin = Stroke::new(1.0_f32, colors.border);
    let gold_strong = Stroke::new(1.5_f32, colors.accent);
    let parchment = colors.text_primary;

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = gold_thin;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, colors.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, parchment);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = gold_strong;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, parchment);
    v.widgets.hovered.expansion = 1.5;

    v.widgets.active.corner_radius = radius;
    // Deep walnut accent - gold text reads on it, and gold is the strong-text
    // color so it works on dark walnut panel bg too.
    v.widgets.active.bg_fill = Color32::from_rgb(0x4a, 0x35, 0x1f);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x4a, 0x35, 0x1f);
    v.widgets.active.bg_stroke = gold_strong;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, colors.accent);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = gold_strong;

    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = gold_strong;

    style.spacing.button_padding = egui::vec2(12.0, 6.0);
}

fn apply_deep_sea_decoration(style: &mut Style, colors: &ThemeColors) {
    // Calm rounded panels with a thin lagoon-blue rim. Less aggressive than
    // Nord's frosted glass; the feel is "something glowing under the water".
    let radius = CornerRadius::same(8);
    let rim = Stroke::new(1.0_f32, colors.border);
    let glow = Stroke::new(1.5_f32, colors.accent);
    let foam = Color32::from_rgb(0xe6, 0xf0, 0xf7);

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = rim;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_tertiary;
    v.widgets.inactive.weak_bg_fill = colors.bg_tertiary;
    v.widgets.inactive.bg_stroke = rim;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, foam);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.35);
    v.widgets.hovered.bg_stroke = glow;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, foam);
    v.widgets.hovered.expansion = 1.0;

    v.widgets.active.corner_radius = radius;
    // Darker lagoon shade so foam-white text stays legible.
    v.widgets.active.bg_fill = Color32::from_rgb(0x29, 0x65, 0x86);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x29, 0x65, 0x86);
    v.widgets.active.bg_stroke = glow;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, foam);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.4);
    v.widgets.open.bg_stroke = glow;

    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

fn apply_frost_decoration(style: &mut Style, colors: &ThemeColors) {
    // Crisp, near-monochrome ice palette. Slightly larger corners, very thin
    // borders, and almost no hover expansion - keep it pristine.
    let radius = CornerRadius::same(8);
    let rim = Stroke::new(0.8_f32, colors.border);
    let chill = Stroke::new(1.5_f32, colors.accent);
    let slate = colors.text_primary;

    let v = &mut style.visuals;

    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = rim;

    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.bg_fill = colors.bg_secondary;
    v.widgets.inactive.weak_bg_fill = colors.bg_secondary;
    v.widgets.inactive.bg_stroke = rim;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, slate);

    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.bg_fill = colors.accent.linear_multiply(0.25);
    v.widgets.hovered.weak_bg_fill = colors.accent.linear_multiply(0.25);
    v.widgets.hovered.bg_stroke = chill;
    v.widgets.hovered.fg_stroke = Stroke::new(1.2_f32, slate);
    v.widgets.hovered.expansion = 0.5;

    // egui aliases `RichText::strong()` text color to `widgets.active.fg_stroke.color`.
    // Frost's panel background is near-white, so a white fg_stroke makes every
    // strong heading (Settings section titles, Markdown headings, CollapsingHeader
    // main labels) invisible. Use slate text instead, and darken the active fill
    // to keep contrast on actual pressed buttons.
    v.widgets.active.corner_radius = radius;
    v.widgets.active.bg_fill = Color32::from_rgb(0x29, 0x6e, 0x90);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x29, 0x6e, 0x90);
    v.widgets.active.bg_stroke = chill;
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, slate);

    v.widgets.open.corner_radius = radius;
    v.widgets.open.bg_fill = colors.accent.linear_multiply(0.3);
    v.widgets.open.bg_stroke = chill;

    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    style.spacing.button_padding = egui::vec2(10.0, 5.0);
}

/// Paint the per-theme background decoration onto `painter` clipped to `rect`.
/// Called by the central panel before rendering content, so widgets sit on
/// top. Themes without a decoration are a no-op - this is the *only* place
/// background graphics live; it keeps the renderer thin and theme-aware.
pub fn paint_background_decoration(painter: &egui::Painter, rect: egui::Rect, mode: ThemeMode) {
    match mode {
        ThemeMode::Manga => paint_manga_background(painter, rect),
        ThemeMode::North => paint_north_background(painter, rect),
        ThemeMode::Dracula => paint_dracula_background(painter, rect),
        ThemeMode::GruvboxDark => paint_gruvbox_background(painter, rect),
        ThemeMode::Gentleman => paint_gentleman_background(painter, rect),
        ThemeMode::DeepSea => paint_deep_sea_background(painter, rect),
        ThemeMode::Frost => paint_frost_background(painter, rect),
        // Warm and Forest stay clean: no background art, palette only.
        ThemeMode::Light
        | ThemeMode::Dark
        | ThemeMode::HighContrast
        | ThemeMode::Warm
        | ThemeMode::Forest
        | ThemeMode::Rainbow => {}
    }
}

fn paint_manga_background(painter: &egui::Painter, rect: egui::Rect) {
    // Layer 1: halftone screentone. Triangular lattice (offset every other
    // row) of small ink dots - classic manga screen-tone texture. Stronger
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
        y += STEP * 0.866; // sin(60°) - equilateral spacing.
        row_idx += 1;
    }

    // Layer 2: speed-line burst from the top-right corner - the "action
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
        // Slightly thicker at the origin, thinner at the tip - emulate brush
        // taper with a single straight segment by varying stroke width per
        // line index.
        let width = 0.6 + 0.4 * (1.0 - t);
        painter.line_segment([origin, end], Stroke::new(width, line_color));
    }
}

fn paint_north_background(painter: &egui::Painter, rect: egui::Rect) {
    // Aurora bands: three soft horizontal stripes at varying alpha - a nod to
    // North's polar inspiration without obscuring content. The bands fade in
    // and out via piecewise-linear alpha so the seam isn't a hard line.
    let bands = [
        (
            0.18_f32,
            Color32::from_rgba_unmultiplied(0x88, 0xc0, 0xd0, 14),
        ),
        (
            0.55_f32,
            Color32::from_rgba_unmultiplied(0x8f, 0xbc, 0xbb, 12),
        ),
        (
            0.82_f32,
            Color32::from_rgba_unmultiplied(0x5e, 0x81, 0xac, 14),
        ),
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
    // CRT scanlines + faint vertical neon glow on the right edge - a slightly
    // cyberpunk vibe without becoming visual noise.
    let line_color = Color32::from_rgba_unmultiplied(0x0a, 0x06, 0x10, 120);
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0_f32, line_color),
        );
        y += 3.0;
    }

    // Right-edge neon glow (purple -> pink).
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
    // Diamond / lozenge lattice in faint champagne gold - old-world wallpaper
    // without competing for the reader's attention. Diagonals only, drawn at
    // wide spacing so the eye reads "pattern" not "noise".
    let line_color = Color32::from_rgba_unmultiplied(0xc8, 0x9b, 0x3c, 14);
    let stroke = Stroke::new(0.7_f32, line_color);
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
    // Down-left diagonals - same spacing, opposite slope, completes the
    // lozenge mesh.
    let mut start = rect.left() - rect.height();
    while start < end {
        let p1 = egui::pos2(start + rect.height(), rect.top());
        let p2 = egui::pos2(start, rect.bottom());
        painter.line_segment([p1, p2], stroke);
        start += step;
    }
}

fn paint_deep_sea_background(painter: &egui::Painter, rect: egui::Rect) {
    // Slow caustic-style horizontal bands rising from the depths. Very faint
    // - readability of dark text on dark navy beats decoration.
    let bands = [
        (
            0.20_f32,
            Color32::from_rgba_unmultiplied(0x5f, 0xb1, 0xd4, 12),
        ),
        (
            0.55_f32,
            Color32::from_rgba_unmultiplied(0x3a, 0x8f, 0xb7, 14),
        ),
        (
            0.85_f32,
            Color32::from_rgba_unmultiplied(0x21, 0x4a, 0x6a, 18),
        ),
    ];
    let height = rect.height();
    let band_h = height * 0.25;
    for (center_t, color) in bands {
        let cy = rect.top() + height * center_t;
        let n = 22;
        for i in 0..n {
            let dt = (i as f32 / n as f32 - 0.5) * 2.0;
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

fn paint_frost_background(painter: &egui::Painter, rect: egui::Rect) {
    // Sparse drifting snowflake field. Deterministic LCG so flakes don't
    // shimmer between frames. Two sizes for a tiny depth cue.
    let mut seed: u32 = 0x5f3759df;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed
    };
    let area = rect.width() * rect.height();
    let count = (area / 1800.0) as usize;
    let dot_color = Color32::from_rgba_unmultiplied(0x7f, 0xb4, 0xd4, 40);
    let dot_color_dim = Color32::from_rgba_unmultiplied(0xb6, 0xc9, 0xd7, 30);
    for i in 0..count {
        let r1 = next();
        let r2 = next();
        let r3 = next();
        let x = rect.left() + (r1 % 10_000) as f32 / 10_000.0 * rect.width();
        let y = rect.top() + (r2 % 10_000) as f32 / 10_000.0 * rect.height();
        let big = (r3 & 3) == 0;
        let radius = if big { 1.6 } else { 0.9 };
        let color = if i & 1 == 0 { dot_color } else { dot_color_dim };
        painter.circle_filled(egui::pos2(x, y), radius, color);
    }
}
