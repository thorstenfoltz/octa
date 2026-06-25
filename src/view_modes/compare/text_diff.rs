//! Text-diff renderer for the Compare view: a side-by-side, line-by-line
//! diff of `tab.raw_content` (left) vs `tab.compare_right_raw` (right).
//!
//! Uses `similar` for the diff computation. Each line is annotated with one
//! of three states - Equal, Insert, Delete (Replace is split into Insert +
//! Delete by the line-diff algorithm) - and painted with a small marker
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

    // Build per-side line streams: each row is (marker, line, bg). The LEFT is
    // the current/active file, the RIGHT is what it is compared against (for a
    // git compare, the committed version). So a line that exists only on the
    // left is an addition in the current file (green `+`), and a line that
    // exists only on the right was removed from it (red `-`). Because the diff
    // is `diff_lines(left, right)`, `similar` reports left-only lines as
    // `Delete` and right-only lines as `Insert` - the opposite of the colours,
    // so we map them deliberately here. Empty placeholders keep rows aligned.
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
            // Only in the left (current) file: an addition -> green `+`.
            ChangeTag::Delete => {
                left_rows.push(("+", trimmed, Some(add_bg)));
                right_rows.push(("", String::new(), None));
            }
            // Only in the right (compared) file: a removal -> red `-`.
            ChangeTag::Insert => {
                left_rows.push(("", String::new(), None));
                right_rows.push(("-", trimmed, Some(del_bg)));
            }
        }
    }

    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);

    egui::ScrollArea::vertical()
        .id_salt("compare_text_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let total_w = ui.available_width();
            // Two equal panes with an 8px gutter between them.
            let pane_w = ((total_w - 8.0) / 2.0).max(160.0);
            // Content starts after the line-number + marker gutter.
            let content_x = 56.0;
            let content_w = (pane_w - 8.0 - content_x).max(40.0);
            let min_row_h = mono.size * 1.4;

            // Lay a line out as a wrapped galley bounded to one pane's content
            // width, so a long line breaks onto further lines instead of
            // overflowing into (and painting over) the other pane.
            let layout_line = |ui: &egui::Ui, text: &str| -> std::sync::Arc<egui::Galley> {
                let mut job = egui::text::LayoutJob::single_section(
                    text.to_owned(),
                    egui::text::TextFormat {
                        font_id: mono.clone(),
                        color: colors.text_primary,
                        ..Default::default()
                    },
                );
                job.wrap.max_width = content_w;
                ui.fonts_mut(|f| f.layout_job(job))
            };

            for (idx, (left, right)) in left_rows.iter().zip(right_rows.iter()).enumerate() {
                let lg = layout_line(ui, &left.1);
                let rg = layout_line(ui, &right.1);
                // Both panes share the taller height so corresponding lines
                // stay vertically aligned when one side wraps.
                let row_h = lg.size().y.max(rg.size().y).max(min_row_h) + 2.0;
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(pane_w * 2.0 + 8.0, row_h),
                    egui::Sense::hover(),
                );
                draw_row(
                    ui,
                    rect.left_top(),
                    pane_w,
                    content_x,
                    row_h,
                    idx + 1,
                    left,
                    lg,
                    &mono,
                    &colors,
                );
                draw_row(
                    ui,
                    rect.left_top() + egui::vec2(pane_w + 8.0, 0.0),
                    pane_w,
                    content_x,
                    row_h,
                    idx + 1,
                    right,
                    rg,
                    &mono,
                    &colors,
                );
            }
        });
}

/// Paint one line of one pane: background tint, line-number gutter, change
/// marker, and the (already wrapped) content galley.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    ui: &egui::Ui,
    origin: egui::Pos2,
    pane_w: f32,
    content_x: f32,
    row_h: f32,
    line_no: usize,
    row: &(&'static str, String, Option<Color32>),
    galley: std::sync::Arc<egui::Galley>,
    mono: &egui::FontId,
    colors: &ui::theme::ThemeColors,
) {
    let (marker, _line, bg) = row;
    let painter = ui.painter();
    let rect = egui::Rect::from_min_size(origin, egui::vec2(pane_w, row_h));
    if let Some(c) = bg {
        painter.rect_filled(rect, 0.0, *c);
    }
    painter.text(
        origin + egui::vec2(2.0, 2.0),
        egui::Align2::LEFT_TOP,
        format!("{line_no:>4}"),
        mono.clone(),
        colors.text_muted,
    );
    painter.text(
        origin + egui::vec2(40.0, 2.0),
        egui::Align2::LEFT_TOP,
        marker,
        mono.clone(),
        match *marker {
            "+" => Color32::from_rgb(60, 160, 80),
            "-" => Color32::from_rgb(200, 70, 70),
            _ => colors.text_muted,
        },
    );
    painter.galley(
        origin + egui::vec2(content_x, 2.0),
        galley,
        colors.text_primary,
    );
}

fn strip_trailing_newline(s: &str) -> String {
    s.strip_suffix('\n').unwrap_or(s).to_string()
}
