use crate::app::state::TabState;
use crate::ui;
use octa::data;

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Stroke};
use ui::settings::NotebookOutputLayout;
use ui::theme::ThemeMode;

/// Render the Jupyter Notebook view. Handles Ctrl+X for copying all cells.
pub fn render_notebook_view(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &TabState,
    theme_mode: ThemeMode,
    output_layout: NotebookOutputLayout,
) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let is_dark = theme_mode.is_dark();

    if tab.table.row_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Empty notebook")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    // Helper: build a LayoutJob for line number gutter (non-selectable)
    let build_line_numbers = |line_count: usize, line_num_color: Color32| {
        let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
        let gutter_width = line_count.max(1).to_string().len();
        let mut job = egui::text::LayoutJob::default();
        for i in 0..line_count.max(1) {
            let num_str = format!("{:>width$}", i + 1, width = gutter_width);
            let suffix = if i + 1 < line_count.max(1) { "\n" } else { "" };
            job.append(
                &format!("{}{}", num_str, suffix),
                0.0,
                egui::text::TextFormat {
                    font_id: mono.clone(),
                    color: line_num_color,
                    ..Default::default()
                },
            );
        }
        job
    };

    // Collect all cell text for Ctrl+C on the whole notebook
    let mut all_notebook_text = String::new();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    for row_idx in 0..tab.table.row_count() {
                        let cell_num = match tab.table.get(row_idx, 0) {
                            Some(data::CellValue::Int(n)) => Some(n),
                            _ => None,
                        };
                        let cell_type = match tab.table.get(row_idx, 1) {
                            Some(data::CellValue::String(s)) => s.clone(),
                            _ => "code".to_string(),
                        };
                        let source = match tab.table.get(row_idx, 2) {
                            Some(data::CellValue::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => String::new(),
                        };
                        let output = match tab.table.get(row_idx, 3) {
                            Some(data::CellValue::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => String::new(),
                        };

                        // Accumulate for whole-notebook copy
                        if !all_notebook_text.is_empty() {
                            all_notebook_text.push_str("\n\n");
                        }
                        all_notebook_text.push_str(&source);
                        if !output.is_empty() {
                            all_notebook_text.push('\n');
                            all_notebook_text.push_str(&output);
                        }

                        let is_code = cell_type == "code";
                        let is_markdown = cell_type == "markdown";

                        // Cell container
                        let cell_bg = if is_code {
                            if is_dark {
                                Color32::from_rgb(30, 34, 42)
                            } else {
                                Color32::from_rgb(248, 249, 250)
                            }
                        } else if is_dark {
                            Color32::from_rgb(35, 38, 45)
                        } else {
                            Color32::from_rgb(252, 252, 254)
                        };

                        let border_color = if is_code {
                            if is_dark {
                                Color32::from_rgb(60, 70, 90)
                            } else {
                                Color32::from_rgb(200, 210, 220)
                            }
                        } else {
                            colors.border_subtle
                        };

                        let line_num_color = if is_dark {
                            Color32::from_rgb(100, 110, 130)
                        } else {
                            Color32::from_rgb(150, 160, 175)
                        };

                        let text_color = if is_markdown {
                            colors.text_secondary
                        } else {
                            colors.text_primary
                        };

                        let line_count = source.lines().count();
                        let label_width = 80.0;

                        let has_output = is_code && !output.is_empty();

                        // Helper closure to render the output frame
                        let render_output =
                            |ui: &mut egui::Ui,
                             cell_num: Option<i64>,
                             output: &str,
                             border_color: Color32| {
                                let out_bg = if is_dark {
                                    Color32::from_rgb(25, 28, 35)
                                } else {
                                    Color32::from_rgb(255, 255, 255)
                                };
                                let out_frame = egui::Frame::new()
                                    .fill(out_bg)
                                    .stroke(Stroke::new(1.0, border_color))
                                    .corner_radius(4.0)
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        let out_label = if let Some(n) = cell_num {
                                            format!("Out[{}]:", n)
                                        } else {
                                            "Out[ ]:".to_string()
                                        };
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(out_label)
                                                    .font(egui::FontId::new(
                                                        12.0,
                                                        egui::FontFamily::Monospace,
                                                    ))
                                                    .color(colors.error),
                                            );
                                        });
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(output)
                                                    .font(egui::FontId::new(
                                                        13.0,
                                                        egui::FontFamily::Monospace,
                                                    ))
                                                    .color(colors.text_secondary),
                                            )
                                            .selectable(true),
                                        );
                                    });
                                let copy_output = output.to_string();
                                out_frame.response.context_menu(|ui| {
                                    if ui.button("Copy output").clicked() {
                                        ui.ctx().copy_text(copy_output.clone());
                                        ui.close_menu();
                                    }
                                });
                            };

                        // Cell label + source (always in a horizontal row)
                        ui.horizontal(|ui| {
                            // Left label area
                            ui.allocate_ui_with_layout(
                                egui::vec2(label_width, 20.0),
                                Layout::right_to_left(Align::TOP),
                                |ui| {
                                    if is_code {
                                        let label = if let Some(n) = cell_num {
                                            format!("In [{}]:", n)
                                        } else {
                                            "In [ ]:".to_string()
                                        };
                                        ui.label(
                                            RichText::new(label)
                                                .font(egui::FontId::new(
                                                    12.0,
                                                    egui::FontFamily::Monospace,
                                                ))
                                                .color(colors.accent),
                                        );
                                    }
                                },
                            );

                            // Cell content area with separate gutter + source
                            let frame_response = egui::Frame::new()
                                .fill(cell_bg)
                                .stroke(Stroke::new(1.0, border_color))
                                .corner_radius(4.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.horizontal_top(|ui| {
                                        // Line number gutter (not selectable)
                                        let gutter_job =
                                            build_line_numbers(line_count, line_num_color);
                                        ui.add(egui::Label::new(gutter_job).selectable(false));
                                        ui.add_space(8.0);
                                        // Source text (selectable -- no line numbers)
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&source)
                                                    .font(egui::FontId::new(
                                                        13.0,
                                                        egui::FontFamily::Monospace,
                                                    ))
                                                    .color(text_color),
                                            )
                                            .selectable(true),
                                        );
                                    });
                                });
                            let copy_source = source.clone();
                            let all_text = all_notebook_text.clone();
                            frame_response.response.context_menu(|ui| {
                                if ui.button("Copy cell").clicked() {
                                    ui.ctx().copy_text(copy_source.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Copy all cells").clicked() {
                                    ui.ctx().copy_text(all_text.clone());
                                    ui.close_menu();
                                }
                            });

                            // Output beside source (Beside layout)
                            if has_output && output_layout == NotebookOutputLayout::Beside {
                                render_output(ui, cell_num.copied(), &output, border_color);
                            }
                        });

                        // Output beneath source (Beneath layout)
                        if has_output && output_layout == NotebookOutputLayout::Beneath {
                            ui.horizontal(|ui| {
                                // Indent to align under the source frame
                                ui.add_space(label_width + 8.0);
                                render_output(ui, cell_num.copied(), &output, border_color);
                            });
                        }

                        // Separator between cells
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                });
            });
        });

    // Ctrl+X: cut (copy all notebook content -- notebook view is read-only)
    if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::X)) {
        ctx.copy_text(all_notebook_text);
    }
}
