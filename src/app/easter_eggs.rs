//! Hidden delights: Konami-code confetti, the secret Rainbow theme triggered
//! by clicking the toolbar logo seven times, and the empty-file ASCII octopus
//! placeholder. Each is intentionally small and self-contained so it can be
//! ripped out in seconds if it ever gets in the way.

use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, Stroke};

use octa::ui::theme::{FontSettings, ThemeMode, apply_theme};

use super::state::OctaApp;

/// The Konami code. Egui's logical Key names diverge from the historical
/// "↑↑↓↓←→←→BA" — match by `Key`, not by character.
const KONAMI: &[egui::Key] = &[
    egui::Key::ArrowUp,
    egui::Key::ArrowUp,
    egui::Key::ArrowDown,
    egui::Key::ArrowDown,
    egui::Key::ArrowLeft,
    egui::Key::ArrowRight,
    egui::Key::ArrowLeft,
    egui::Key::ArrowRight,
    egui::Key::B,
    egui::Key::A,
];

/// How long the confetti overlay animates after a successful Konami input.
const CONFETTI_DURATION_S: f32 = 3.5;

/// Number of clicks on the toolbar logo required to enable the hidden
/// Rainbow theme.
pub(crate) const LOGO_CLICK_TARGET: u8 = 7;

/// Maximum gap between consecutive logo clicks for the streak to count.
pub(crate) const LOGO_CLICK_WINDOW: Duration = Duration::from_millis(1500);

/// Clicks on the *welcome-screen* logo required to start the snow easter egg.
/// Distinct from the toolbar logo's Rainbow trigger so the two animations
/// can coexist without stealing each other's input.
pub(crate) const WELCOME_LOGO_CLICK_TARGET: u8 = 3;

/// Maximum gap between consecutive welcome-logo clicks for the streak.
pub(crate) const WELCOME_LOGO_CLICK_WINDOW: Duration = Duration::from_millis(1500);

/// How long the snowfall overlay animates after the trigger. Long enough
/// for the snow drift at the bottom to visibly accumulate.
pub(crate) const SNOWFALL_DURATION_S: f32 = 7.0;

impl OctaApp {
    /// Walk this frame's keyboard events and advance the Konami matcher.
    /// Triggers a confetti animation on full match. Safe to call every frame.
    pub(crate) fn update_easter_egg_inputs(&mut self, ctx: &egui::Context) {
        // Don't intercept arrow keys when a TextEdit is focused — the user is
        // navigating text, not entering a code.
        let text_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some();
        if text_focused {
            self.konami_index = 0;
            return;
        }

        let mut matched = false;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = ev
                {
                    if KONAMI[self.konami_index as usize] == *key {
                        self.konami_index += 1;
                        if self.konami_index as usize == KONAMI.len() {
                            matched = true;
                            self.konami_index = 0;
                        }
                    } else if KONAMI[0] == *key {
                        self.konami_index = 1;
                    } else {
                        self.konami_index = 0;
                    }
                }
            }
        });
        if matched {
            self.confetti_until =
                Some(Instant::now() + Duration::from_millis((CONFETTI_DURATION_S * 1000.0) as u64));
            self.status_message = Some(("\u{1f389} Konami!".to_string(), Instant::now()));
        }
    }

    /// Paint the confetti overlay if currently active. No-op otherwise.
    pub(crate) fn render_confetti(&mut self, ctx: &egui::Context) {
        let Some(until) = self.confetti_until else {
            return;
        };
        if Instant::now() >= until {
            self.confetti_until = None;
            return;
        }
        ctx.request_repaint();
        let elapsed = CONFETTI_DURATION_S
            - until
                .saturating_duration_since(Instant::now())
                .as_secs_f32();
        let area = egui::Area::new(egui::Id::new("octa_confetti"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false);
        area.show(ctx, |ui| {
            let screen = ctx.content_rect();
            let painter = ui.painter();
            paint_confetti(painter, screen, elapsed);
            paint_konami_banner(painter, screen, elapsed);
        });
    }

    /// Register a click on the toolbar logo. Returns whether this click just
    /// triggered the hidden Rainbow theme (the caller can decide whether to
    /// show a status message).
    pub(crate) fn register_logo_click(&mut self, ctx: &egui::Context) -> bool {
        let now = Instant::now();
        let in_window = self
            .logo_last_click
            .is_some_and(|t| now.saturating_duration_since(t) < LOGO_CLICK_WINDOW);
        self.logo_last_click = Some(now);
        if in_window {
            self.logo_click_count = self.logo_click_count.saturating_add(1);
        } else {
            self.logo_click_count = 1;
        }
        if self.logo_click_count >= LOGO_CLICK_TARGET && !self.rainbow_active {
            self.rainbow_active = true;
            self.theme_mode = ThemeMode::Rainbow;
            apply_theme(
                ctx,
                ThemeMode::Rainbow,
                FontSettings {
                    size: self.settings.font_size * self.zoom_percent as f32 / 100.0,
                    body: self.settings.body_font,
                    custom_path: Some(self.settings.custom_font_path.as_str()),
                },
            );
            self.logo_click_count = 0;
            self.status_message = Some((
                "\u{1f308} Rainbow mode unlocked".to_string(),
                Instant::now(),
            ));
            ctx.request_repaint();
            return true;
        }
        false
    }
}

/// Deterministic-but-cheap confetti animation: 80 particles falling from the
/// top of the screen, each with a fixed seed.
fn paint_confetti(painter: &egui::Painter, screen: egui::Rect, t: f32) {
    const N: usize = 80;
    let palette = [
        Color32::from_rgb(255, 87, 87),
        Color32::from_rgb(255, 191, 64),
        Color32::from_rgb(94, 232, 129),
        Color32::from_rgb(64, 156, 255),
        Color32::from_rgb(186, 104, 255),
        Color32::from_rgb(255, 109, 200),
    ];
    let life = CONFETTI_DURATION_S;
    let fade = ((life - t) / life).clamp(0.0, 1.0);
    for i in 0..N {
        let seed = (i as u32).wrapping_mul(2654435761) ^ 0xa3c1d2e4;
        let x_seed = ((seed >> 8) & 0xffff) as f32 / 65535.0;
        let phase = ((seed >> 3) & 0xff) as f32 / 255.0;
        let drift = (((seed >> 16) & 0xff) as f32 / 255.0 - 0.5) * 60.0;
        let speed = 240.0 + ((seed & 0xff) as f32);
        let y = -20.0 + speed * t + (t * 4.0 + phase * std::f32::consts::TAU).sin() * 8.0;
        let x = screen.left() + x_seed * screen.width() + drift * t;
        if y > screen.bottom() + 20.0 {
            continue;
        }
        let color = palette[i % palette.len()];
        let faded =
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (255.0 * fade) as u8);
        let size = 4.0 + ((seed >> 24) & 7) as f32;
        let rect = egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(size, size * 0.6));
        painter.rect_filled(rect, egui::CornerRadius::same(1), faded);
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(1),
            Stroke::new(0.5, faded),
            egui::StrokeKind::Outside,
        );
    }
}

/// Direction for [`paint_arrow`]. Up = points toward the top of the screen.
#[derive(Clone, Copy)]
enum ArrowDir {
    Up,
    Down,
    Left,
    Right,
}

/// Paint a small filled triangular arrow centered on `center`, pointing in
/// `dir`, with `size` controlling both width and height (≈ size × size).
fn paint_arrow(
    painter: &egui::Painter,
    center: egui::Pos2,
    dir: ArrowDir,
    size: f32,
    color: Color32,
) {
    let h = size * 0.5;
    let pts = match dir {
        ArrowDir::Up => vec![
            egui::pos2(center.x, center.y - h),
            egui::pos2(center.x - h, center.y + h),
            egui::pos2(center.x + h, center.y + h),
        ],
        ArrowDir::Down => vec![
            egui::pos2(center.x, center.y + h),
            egui::pos2(center.x - h, center.y - h),
            egui::pos2(center.x + h, center.y - h),
        ],
        ArrowDir::Left => vec![
            egui::pos2(center.x - h, center.y),
            egui::pos2(center.x + h, center.y - h),
            egui::pos2(center.x + h, center.y + h),
        ],
        ArrowDir::Right => vec![
            egui::pos2(center.x + h, center.y),
            egui::pos2(center.x - h, center.y - h),
            egui::pos2(center.x - h, center.y + h),
        ],
    };
    painter.add(egui::Shape::convex_polygon(
        pts,
        color,
        Stroke::new(1.0, color),
    ));
}

/// Paint the "↑↑↓↓←→←→BA" banner inside the same overlay area as the confetti.
/// Uses vector triangles for the arrows so the glyphs always render, regardless
/// of the active font's coverage.
fn paint_konami_banner(painter: &egui::Painter, screen: egui::Rect, t: f32) {
    let life = CONFETTI_DURATION_S;
    let fade = ((life - t) / life).clamp(0.0, 1.0);
    let alpha = (255.0 * fade) as u8;
    let arrow_color = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
    let label_color = Color32::from_rgba_unmultiplied(255, 220, 120, alpha);
    let bg = Color32::from_rgba_unmultiplied(20, 20, 28, (alpha as f32 * 0.8) as u8);
    let border = Color32::from_rgba_unmultiplied(255, 220, 120, (alpha as f32 * 0.9) as u8);

    let arrow_size = 18.0;
    let gap = 6.0;
    let label_w = 56.0;
    let total_w = label_w + gap + 8.0 * (arrow_size + gap) + label_w;
    let banner_h = arrow_size + 24.0;
    let center_x = screen.center().x;
    let top_y = screen.top() + 32.0;
    let rect = egui::Rect::from_center_size(
        egui::pos2(center_x, top_y + banner_h * 0.5),
        egui::vec2(total_w + 24.0, banner_h),
    );
    painter.rect_filled(rect, egui::CornerRadius::same(6), bg);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        Stroke::new(1.0, border),
        egui::StrokeKind::Outside,
    );

    let label_font = egui::FontId::proportional(16.0);
    let mut x = rect.left() + 12.0;
    let cy = rect.center().y;

    painter.text(
        egui::pos2(x, cy),
        egui::Align2::LEFT_CENTER,
        "KONAMI!",
        label_font.clone(),
        label_color,
    );
    x += label_w + gap;

    let dirs = [
        ArrowDir::Up,
        ArrowDir::Up,
        ArrowDir::Down,
        ArrowDir::Down,
        ArrowDir::Left,
        ArrowDir::Right,
        ArrowDir::Left,
        ArrowDir::Right,
    ];
    for d in dirs {
        paint_arrow(
            painter,
            egui::pos2(x + arrow_size * 0.5, cy),
            d,
            arrow_size,
            arrow_color,
        );
        x += arrow_size + gap;
    }
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        "B A",
        label_font,
        label_color,
    );
}

/// ASCII art shown on the central panel when an empty file is opened.
pub(crate) const EMPTY_FILE_ART: &str = r#"
        _---_
      /       \
     |  .   .  |
      \   ^   /
      /(. v .)\
     / / \_/ \ \
    | |  | |  | |
   /  /  | |  \  \
  /__/   |_|   \__\
"#;

pub(crate) const EMPTY_FILE_TAGLINE: &str =
    "This file is as empty as the deep sea floor. Nothing to read here.";

/// Local date check: is today in the Dec 24-26 window? Used to enable the
/// passive Christmas overlay without any explicit trigger. Returns `false`
/// when the local clock can't be read (unlikely).
pub(crate) fn is_christmas_window() -> bool {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    today.month() == 12 && (24..=26).contains(&today.day())
}

impl OctaApp {
    /// Register a click on the welcome-screen logo. When three clicks land
    /// within `WELCOME_LOGO_CLICK_WINDOW`, kicks off a `SNOWFALL_DURATION_S`
    /// snowfall animation. No-op if a snowfall is already in flight (so the
    /// user can't pile clicks to extend it).
    pub(crate) fn register_welcome_logo_click(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let in_window = self
            .welcome_logo_last_click
            .is_some_and(|t| now.saturating_duration_since(t) < WELCOME_LOGO_CLICK_WINDOW);
        self.welcome_logo_last_click = Some(now);
        if in_window {
            self.welcome_logo_click_count = self.welcome_logo_click_count.saturating_add(1);
        } else {
            self.welcome_logo_click_count = 1;
        }
        if self.welcome_logo_click_count >= WELCOME_LOGO_CLICK_TARGET
            && self.snowfall_until.is_none()
        {
            self.welcome_logo_click_count = 0;
            self.snowfall_until =
                Some(Instant::now() + Duration::from_millis((SNOWFALL_DURATION_S * 1000.0) as u64));
            self.status_message = Some(("\u{2744} Let it snow!".to_string(), Instant::now()));
            ctx.request_repaint();
        }
    }

    /// Paint the snowfall overlay if currently active. No-op otherwise.
    /// Painted in the same `Foreground` Area pattern as the confetti so it
    /// floats on top of the table view without intercepting clicks.
    pub(crate) fn render_snowfall(&mut self, ctx: &egui::Context) {
        let Some(until) = self.snowfall_until else {
            return;
        };
        if Instant::now() >= until {
            self.snowfall_until = None;
            return;
        }
        ctx.request_repaint();
        let elapsed = SNOWFALL_DURATION_S
            - until
                .saturating_duration_since(Instant::now())
                .as_secs_f32();
        let is_dark = self.theme_mode.is_dark();
        let area = egui::Area::new(egui::Id::new("octa_snowfall"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false);
        area.show(ctx, |ui| {
            let screen = ctx.content_rect();
            paint_snowfall(ui.painter(), screen, elapsed, is_dark);
        });
    }

    /// Paint the passive Christmas overlay if today is Dec 24-26.
    /// Renders a few large snowflakes near the screen corners — subtle,
    /// always on top, never blocks clicks. Independent of the snowfall
    /// easter egg (which is the click-triggered burst).
    pub(crate) fn render_christmas_overlay(&mut self, ctx: &egui::Context) {
        if !is_christmas_window() {
            return;
        }
        let is_dark = self.theme_mode.is_dark();
        let area = egui::Area::new(egui::Id::new("octa_christmas"))
            .order(egui::Order::Background)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false);
        area.show(ctx, |ui| {
            let screen = ctx.content_rect();
            paint_christmas_decorations(ui.painter(), screen, is_dark);
        });
    }
}

/// Deterministic snowfall: a dense field of particles drifting from above
/// the viewport down, with a gentle horizontal sway, plus an accumulating
/// snow drift at the bottom edge that grows over the burst's lifetime.
/// Each particle has a fixed seed so the animation is reproducible without
/// per-frame allocation.
fn paint_snowfall(painter: &egui::Painter, screen: egui::Rect, t: f32, is_dark: bool) {
    const N: usize = 260;
    let life = SNOWFALL_DURATION_S;
    let fade = ((life - t) / life).clamp(0.0, 1.0);
    let alpha = (255.0 * fade) as u8;
    let flake_color = if is_dark {
        Color32::from_rgba_unmultiplied(245, 248, 255, alpha)
    } else {
        Color32::from_rgba_unmultiplied(110, 145, 200, alpha)
    };
    let flake_outline = if is_dark {
        None
    } else {
        Some(Stroke::new(
            0.8,
            Color32::from_rgba_unmultiplied(60, 95, 150, (alpha as u16 * 180 / 255) as u8),
        ))
    };
    for i in 0..N {
        let seed = (i as u32).wrapping_mul(2654435761) ^ 0x5e1f_a771;
        let x_seed = ((seed >> 8) & 0xffff) as f32 / 65535.0;
        let phase = ((seed >> 3) & 0xff) as f32 / 255.0;
        let speed = 90.0 + ((seed & 0xff) as f32) * 0.6;
        let sway = (t * 2.0 + phase * std::f32::consts::TAU).sin() * 20.0;
        // Particles spawn at staggered start times so the sky isn't filled
        // instantly — looks like real snow gathering.
        let delay = phase * 0.8;
        let local_t = (t - delay).max(0.0);
        let y = -16.0 + speed * local_t;
        let x = screen.left() + x_seed * screen.width() + sway;
        if y > screen.bottom() + 16.0 {
            continue;
        }
        let radius = 1.6 + ((seed >> 24) & 7) as f32 * 0.35;
        let center = egui::pos2(x, y);
        painter.circle_filled(center, radius, flake_color);
        if let Some(stroke) = flake_outline {
            painter.circle_stroke(center, radius, stroke);
        }
    }
    // Drift goes last so it occludes flakes that have already passed below
    // the accumulating snow line.
    paint_snow_drift(painter, screen, t, is_dark, fade);
}

/// Paint a deterministic, irregular snow drift along the bottom edge of
/// the viewport. Height grows linearly with the burst's elapsed time so
/// the user sees the mound build up. 64 buckets across the screen give
/// natural unevenness without expensive smoothing.
fn paint_snow_drift(painter: &egui::Painter, screen: egui::Rect, t: f32, is_dark: bool, fade: f32) {
    const BUCKETS: usize = 64;
    const BASE_HEIGHT: f32 = 36.0;
    const MAX_HEIGHT: f32 = 56.0;

    let alpha_scale = fade;
    let scale_alpha = |a: u8| -> u8 { (a as f32 * alpha_scale) as u8 };

    let (fill, stroke_col, crust, sparkle) = if is_dark {
        (
            Color32::from_rgba_unmultiplied(235, 240, 250, scale_alpha(230)),
            Color32::from_rgba_unmultiplied(255, 255, 255, scale_alpha(200)),
            Color32::from_rgba_unmultiplied(240, 250, 255, scale_alpha(90)),
            Color32::from_rgba_unmultiplied(255, 255, 255, scale_alpha(230)),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(200, 220, 240, scale_alpha(230)),
            Color32::from_rgba_unmultiplied(100, 140, 190, scale_alpha(220)),
            Color32::from_rgba_unmultiplied(180, 210, 240, scale_alpha(140)),
            Color32::from_rgba_unmultiplied(245, 252, 255, scale_alpha(230)),
        )
    };

    let progress = (t / SNOWFALL_DURATION_S).clamp(0.0, 1.0);
    let bucket_w = screen.width() / BUCKETS as f32;

    // Precompute bucket heights so we can both fill the polygon and stroke
    // the top crust ribbon without doing the math twice.
    let mut top_pts: Vec<egui::Pos2> = Vec::with_capacity(BUCKETS + 1);
    for b in 0..=BUCKETS {
        let seed = (b as u32).wrapping_mul(2246822519) ^ 0xc0ff_ee42;
        let jitter = ((seed >> 8) & 0xff) as f32 / 255.0; // 0..1
        let h = (BASE_HEIGHT * progress * (0.55 + jitter * 0.9)).min(MAX_HEIGHT);
        let x = screen.left() + b as f32 * bucket_w;
        let y = screen.bottom() - h;
        top_pts.push(egui::pos2(x, y));
    }

    // Filled mound. Build the polygon manually (top edge + two bottom
    // corners). `Shape::convex_polygon` accepts non-strictly-convex shapes
    // for filling; the irregular top is fine.
    let mut poly_pts = top_pts.clone();
    poly_pts.push(egui::pos2(screen.right(), screen.bottom()));
    poly_pts.push(egui::pos2(screen.left(), screen.bottom()));
    painter.add(egui::Shape::convex_polygon(poly_pts, fill, Stroke::NONE));

    // Top crust ribbon: a thicker translucent stroke right on the curve,
    // selling the "icy crust on top of fresh snow" look.
    painter.add(egui::Shape::line(top_pts.clone(), Stroke::new(2.5, crust)));

    // Sharper outline for definition (especially important in light mode).
    painter.add(egui::Shape::line(
        top_pts.clone(),
        Stroke::new(1.0, stroke_col),
    ));

    // Sparkle dots: a handful of bright pinpricks scattered on the drift
    // surface. Cheap, deterministic, and sells the ice effect.
    for (idx, p) in top_pts.iter().enumerate().step_by(7) {
        let seed = (idx as u32).wrapping_mul(0x9E37_79B1) ^ 0xfade_b00b;
        let offset = ((seed >> 4) & 0x1f) as f32 * 0.4; // 0..12 px
        let sparkle_r = 1.0 + ((seed >> 12) & 3) as f32 * 0.3;
        let center = egui::pos2(p.x, p.y + 3.0 + offset);
        if center.y < screen.bottom() {
            painter.circle_filled(center, sparkle_r, sparkle);
        }
    }
}

/// Static decorations for the Christmas window: large snowflakes in the
/// screen corners, painted just behind everything else.
fn paint_christmas_decorations(painter: &egui::Painter, screen: egui::Rect, is_dark: bool) {
    let color = if is_dark {
        Color32::from_rgba_unmultiplied(220, 230, 245, 70)
    } else {
        Color32::from_rgba_unmultiplied(140, 170, 210, 110)
    };
    let inset = 36.0;
    let positions = [
        egui::pos2(screen.left() + inset, screen.top() + inset),
        egui::pos2(screen.right() - inset, screen.top() + inset),
        egui::pos2(screen.left() + inset, screen.bottom() - inset),
        egui::pos2(screen.right() - inset, screen.bottom() - inset),
    ];
    for p in positions {
        paint_snowflake(painter, p, 18.0, color);
    }
}

/// Paint a six-armed snowflake glyph centered on `center`. Uses thin line
/// segments so it stays crisp at any size and never overlaps the underlying
/// content visibly.
fn paint_snowflake(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    for arm in 0..6 {
        let angle = arm as f32 * std::f32::consts::TAU / 6.0;
        let (s, c) = angle.sin_cos();
        let tip = egui::pos2(center.x + c * radius, center.y + s * radius);
        painter.line_segment([center, tip], stroke);
        // Two small barbs near the tip for the classic snowflake silhouette.
        let barb_base = egui::pos2(center.x + c * radius * 0.6, center.y + s * radius * 0.6);
        let barb_angle1 = angle + std::f32::consts::FRAC_PI_4;
        let barb_angle2 = angle - std::f32::consts::FRAC_PI_4;
        let barb_len = radius * 0.35;
        painter.line_segment(
            [
                barb_base,
                egui::pos2(
                    barb_base.x + barb_angle1.cos() * barb_len,
                    barb_base.y + barb_angle1.sin() * barb_len,
                ),
            ],
            stroke,
        );
        painter.line_segment(
            [
                barb_base,
                egui::pos2(
                    barb_base.x + barb_angle2.cos() * barb_len,
                    barb_base.y + barb_angle2.sin() * barb_len,
                ),
            ],
            stroke,
        );
    }
}
