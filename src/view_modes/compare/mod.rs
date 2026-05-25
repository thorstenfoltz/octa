//! Compare view: side-by-side comparison of two files. Two sub-modes that
//! the user toggles in the view's toolbar:
//!
//! - **TextDiff** — line-by-line git-style diff of raw content (`similar` crate).
//! - **RowHashDiff** — hash user-picked columns per row, set-difference rows
//!   between left and right (`blake3` crate). Order doesn't matter; only the
//!   column content. Cross-format because hashing sees only cell text.
//!
//! The left side is the active tab; the right side lives on
//! `TabState::compare_right_*` and is populated by the
//! "View → Compare with…" menu entry.

use eframe::egui;
use egui::RichText;

use octa::data::CompareMode;

use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

pub mod hash;
pub mod row_diff;
pub mod text_diff;

/// User actions emitted by the Compare view in one frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompareAction {
    /// Clear the right-side and exit Compare back to Table view.
    pub close: bool,
}

/// Render the Compare view. Returns user actions for the app shell to dispatch.
pub fn render_compare_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    syntax_highlight_max_bytes: usize,
) -> CompareAction {
    let mut action = CompareAction::default();
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);

    // Dismissable error banner. Shown when the right-side file failed to
    // load via the standard FormatRegistry path.
    if let Some(err) = tab.compare_error.clone() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("⚠ {err}"))
                    .color(colors.warning)
                    .size(12.0),
            );
            if ui.small_button("✕").clicked() {
                tab.compare_error = None;
            }
        });
        ui.add_space(4.0);
    }

    // Toolbar: which file is on which side + mode toggle + close.
    ui.horizontal(|ui| {
        let left_full = tab
            .table
            .source_path
            .clone()
            .unwrap_or_else(|| "Untitled".to_string());
        let right_full = tab
            .compare_right_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(no file)".to_string());
        let left_label = short_filename(&left_full);
        let right_label = short_filename(&right_full);
        ui.label(RichText::new("Left:").strong())
            .on_hover_text(&left_full);
        ui.label(RichText::new(left_label).color(colors.text_secondary))
            .on_hover_text(&left_full);
        ui.add_space(12.0);
        ui.label(RichText::new("Right:").strong())
            .on_hover_text(&right_full);
        ui.label(RichText::new(right_label).color(colors.text_secondary))
            .on_hover_text(&right_full);
        ui.add_space(16.0);
        // Mode toggle. Each radio commits the new mode immediately —
        // there's no Apply step, the renderers are cheap enough.
        for mode in [CompareMode::TextDiff, CompareMode::RowHashDiff] {
            if ui
                .radio_value(&mut tab.compare_mode, mode, mode.label())
                .clicked()
            {
                // Selecting a new mode is the moment to clear stale
                // per-mode UI state (e.g. column-pick highlights), but
                // both modes are stateless beyond the columns the user
                // picked, so nothing to do here yet.
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close compare").clicked() {
                action.close = true;
            }
        });
    });
    ui.separator();

    // Refuse to render when nothing was loaded for the right side. Should
    // be unreachable if the menu entry validated before setting view_mode.
    if tab.compare_right_path.is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Pick a right-side file via View → Compare with…")
                    .color(colors.text_muted),
            );
        });
        return action;
    }

    match tab.compare_mode {
        CompareMode::TextDiff => {
            text_diff::render(ui, tab, theme_mode, syntax_highlight_max_bytes);
        }
        CompareMode::RowHashDiff => {
            row_diff::render(ui, tab, theme_mode);
        }
    }

    action
}

/// Trim a path to the bare filename for compact toolbar labels.
fn short_filename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}
