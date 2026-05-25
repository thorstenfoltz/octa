//! Text-diff renderer for the Compare view: a side-by-side, line-by-line
//! diff of `tab.raw_content` (left) vs `tab.compare_right_raw` (right).
//!
//! Uses `similar` for the diff computation. Each line is annotated with one
//! of three states — Equal, Insert, Delete (Replace is split into Insert +
//! Delete by the line-diff algorithm) — and painted with a small marker
//! column and a background tint so the visual reads like a `git diff`.

use eframe::egui;
use egui::Color32;
use similar::{ChangeTag, TextDiff};

use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

/// Render a side-by-side text diff into the available space. The two columns
/// share a vertical scroll so left and right line up while reading.
pub fn render(
    ui: &mut egui::Ui,
    tab: &TabState,
    theme_mode: ThemeMode,
    _syntax_highlight_max_bytes: usize,
) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);

    let left_text = tab.raw_content.as_deref().unwrap_or("");
    let right_text = tab.compare_right_raw.as_deref().unwrap_or("");

    // similar's `TextDiff` over lines is O(n²) in worst-case but ships with
    // a `timeout` knob. We cap the work at 500ms so a pathological pair
    // doesn't hang the UI thread.
    let diff = TextDiff::configure()
        .timeout(std::time::Duration::from_millis(500))
        .diff_lines(left_text, right_text);

    let add_bg = if theme_mode.is_dark() {
        Color32::from_rgb(20, 60, 32)
    } else {
        Color32::from_rgb(220, 248, 220)
    };
    let del_bg = if theme_mode.is_dark() {
        Color32::from_rgb(75, 28, 32)
    } else {
        Color32::from_rgb(255, 220, 220)
    };

    // Build per-side line streams: each row is (marker, line, bg). Equal
    // lines appear on both sides; insertions only on the right, deletions
    // only on the left. Empty placeholders keep the rows aligned visually.
    let mut left_rows: Vec<(&'static str, String, Option<Color32>)> = Vec::new();
    let mut right_rows: Vec<(&'static str, String, Option<Color32>)> = Vec::new();

    for change in diff.iter_all_changes() {
        let line = change.to_string();
        let trimmed = strip_trailing_newline(&line);
        match change.tag() {
            ChangeTag::Equal => {
                left_rows.push((" ", trimmed.clone(), None));
                right_rows.push((" ", trimmed, None));
            }
            ChangeTag::Delete => {
                left_rows.push(("-", trimmed, Some(del_bg)));
                right_rows.push(("", String::new(), None));
            }
            ChangeTag::Insert => {
                left_rows.push(("", String::new(), None));
                right_rows.push(("+", trimmed, Some(add_bg)));
            }
        }
    }

    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);

    egui::ScrollArea::both()
        .id_salt("compare_text_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let pane_width = (ui.available_width() - 8.0).max(200.0) / 2.0;
                draw_pane(ui, &left_rows, &mono, &colors, pane_width);
                ui.add_space(8.0);
                draw_pane(ui, &right_rows, &mono, &colors, pane_width);
            });
        });
}

fn draw_pane(
    ui: &mut egui::Ui,
    rows: &[(&'static str, String, Option<Color32>)],
    mono: &egui::FontId,
    colors: &ui::theme::ThemeColors,
    width: f32,
) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, colors.border_subtle))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.vertical(|ui| {
                for (idx, (marker, line, bg)) in rows.iter().enumerate() {
                    let row_rect_height = mono.size * 1.4;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(width - 8.0, row_rect_height),
                        egui::Sense::hover(),
                    );
                    if let Some(c) = bg {
                        ui.painter().rect_filled(rect, 0.0, *c);
                    }
                    // Line number gutter (1-based, left-aligned narrow).
                    let line_no = format!("{:>4}", idx + 1);
                    ui.painter().text(
                        rect.left_top() + egui::vec2(2.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        line_no,
                        mono.clone(),
                        colors.text_muted,
                    );
                    ui.painter().text(
                        rect.left_top() + egui::vec2(40.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        marker,
                        mono.clone(),
                        match *marker {
                            "+" => Color32::from_rgb(60, 160, 80),
                            "-" => Color32::from_rgb(200, 70, 70),
                            _ => colors.text_muted,
                        },
                    );
                    // The actual line content. Drawn as a single Galley so a
                    // very long line just gets clipped at the pane edge.
                    ui.painter().text(
                        rect.left_top() + egui::vec2(60.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        line,
                        mono.clone(),
                        colors.text_primary,
                    );
                }
            });
        });
}

fn strip_trailing_newline(s: &str) -> String {
    s.strip_suffix('\n').unwrap_or(s).to_string()
}
