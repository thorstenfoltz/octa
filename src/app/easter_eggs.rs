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
            self.confetti_until = Some(
                Instant::now() + Duration::from_millis((CONFETTI_DURATION_S * 1000.0) as u64),
            );
            self.status_message = Some((
                "\u{1f389} ↑↑↓↓←→←→BA".to_string(),
                Instant::now(),
            ));
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
            - until.saturating_duration_since(Instant::now()).as_secs_f32();
        let area = egui::Area::new(egui::Id::new("octa_confetti"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false);
        area.show(ctx, |ui| {
            let screen = ctx.screen_rect();
            let painter = ui.painter();
            paint_confetti(painter, screen, elapsed);
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
        let faded = Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            (255.0 * fade) as u8,
        );
        let size = 4.0 + ((seed >> 24) & 7) as f32;
        let rect = egui::Rect::from_center_size(
            egui::pos2(x, y),
            egui::vec2(size, size * 0.6),
        );
        painter.rect_filled(rect, egui::CornerRadius::same(1), faded);
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(1),
            Stroke::new(0.5, faded),
            egui::StrokeKind::Outside,
        );
    }
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
