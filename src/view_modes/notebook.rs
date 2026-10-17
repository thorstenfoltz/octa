use crate::app::state::{NavDir, TabState};
use crate::ui;
use octa::data;

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Stroke};
use octa::formats::jupyter_reader::OutputPart;
use ui::settings::NotebookOutputLayout;
use ui::theme::ThemeMode;

/// Render the Jupyter Notebook view. Source cells are editable (unless
/// `readonly`); edits flow through the normal table edit overlay
/// (`tab.table.set`), so undo/redo, the modified marker, and Save all work.
/// Output cells stay read-only; pictures in them are drawn, and a click on one
/// returns its row and bytes for the binary viewer. Handles Ctrl+X for copying
/// all cells when no cell editor is focused.
pub fn render_notebook_view(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    output_layout: NotebookOutputLayout,
    readonly: bool,
) -> Option<(usize, Vec<u8>)> {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let is_dark = theme_mode.is_dark();

    if tab.table.row_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(octa::i18n::t("view.nb_empty"))
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return None;
    }

    // ponytail: every picture is decoded on the first frame (the view is not
    // virtualised); decode on first sight if a notebook with hundreds of
    // plots opens slowly.
    for (&cell, parts) in &tab.notebook_outputs {
        for (idx, part) in parts.iter().enumerate() {
            if let OutputPart::Picture { bytes, .. } = part {
                tab.notebook_textures.entry((cell, idx)).or_insert_with(|| {
                    octa::data::binary_kind::decode_image(bytes)
                        .map(|rgba| {
                            crate::app::dialogs::binary_viewer::picture_texture(
                                ctx,
                                &format!("nb_picture_{cell}_{idx}"),
                                rgba,
                            )
                        })
                        .map_err(|e| format!("{e:#}"))
                });
            }
        }
    }
    let mut clicked_picture: Option<(usize, Vec<u8>)> = None;

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

    // Highlight search (always on): count matches across every cell's source
    // and output, drive the toolbar count, and on a pending next/previous jump
    // scroll to the cell holding the current match. Matches are highlighted in
    // place; navigation is at cell granularity.
    let matcher = (!tab.search_text.is_empty()).then(|| tab.search_matcher());
    let (hl_normal, hl_active) = ui::search_highlight::highlight_colors(&colors);
    let row_match_counts: Vec<usize> = match matcher.as_ref() {
        Some(m) => (0..tab.table.row_count())
            .map(|r| {
                let src = match tab.table.get(r, 2) {
                    Some(data::CellValue::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                };
                let out = match tab.table.get(r, 3) {
                    Some(data::CellValue::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                };
                m.find_ranges(&src).len() + m.find_ranges(&out).len()
            })
            .collect(),
        None => Vec::new(),
    };
    let total_matches: usize = row_match_counts.iter().sum();
    tab.search_nav.match_count = total_matches;
    if tab.search_nav.current >= total_matches {
        tab.search_nav.current = 0;
    }
    let jump_dir = tab.search_nav.pending_jump.take();
    if let Some(dir) = jump_dir
        && total_matches > 0
    {
        tab.search_nav.current = match dir {
            NavDir::Next => (tab.search_nav.current + 1) % total_matches,
            NavDir::Prev => (tab.search_nav.current + total_matches - 1) % total_matches,
        };
    }
    // Map the current match ordinal to the cell that contains it.
    let scroll_target_row: Option<usize> = if jump_dir.is_some() && total_matches > 0 {
        let mut acc = 0usize;
        let mut found = None;
        for (r, c) in row_match_counts.iter().enumerate() {
            if tab.search_nav.current < acc + c {
                found = Some(r);
                break;
            }
            acc += c;
        }
        found
    } else {
        None
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
                            Some(data::CellValue::Int(n)) => Some(*n),
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
                        let output_hl = matcher
                            .as_ref()
                            .map(|m| m.matches(&output))
                            .unwrap_or(false);
                        let parts = cell_num.and_then(|n| tab.notebook_outputs.get(&n));
                        let textures = &tab.notebook_textures;
                        let part_matcher = matcher.as_ref();

                        // Helper closure to render the output frame. Returns the
                        // index of a picture part that was clicked.
                        let render_output = |ui: &mut egui::Ui,
                                             cell_num: Option<i64>,
                                             output: &str,
                                             border_color: Color32,
                                             highlight: bool|
                         -> Option<usize> {
                            let out_bg = if is_dark {
                                Color32::from_rgb(25, 28, 35)
                            } else {
                                Color32::from_rgb(255, 255, 255)
                            };
                            let out_frame = egui::Frame::new()
                                .fill(out_bg)
                                .stroke(Stroke::new(1.0_f32, border_color))
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
                                    let text_label =
                                        |ui: &mut egui::Ui, text: &str, highlight: bool| {
                                            let mut out_rt = RichText::new(text)
                                                .font(egui::FontId::new(
                                                    13.0,
                                                    egui::FontFamily::Monospace,
                                                ))
                                                .color(colors.text_secondary);
                                            if highlight {
                                                out_rt = out_rt.background_color(hl_normal);
                                            }
                                            ui.add(egui::Label::new(out_rt).selectable(true));
                                        };
                                    let (Some(parts), Some(cell)) = (parts, cell_num) else {
                                        text_label(ui, output, highlight);
                                        return None;
                                    };
                                    let mut clicked = None;
                                    ui.vertical(|ui| {
                                        for (idx, part) in parts.iter().enumerate() {
                                            match (part, textures.get(&(cell, idx))) {
                                                (OutputPart::Text(text), _) => text_label(
                                                    ui,
                                                    text,
                                                    part_matcher.is_some_and(|m| m.matches(text)),
                                                ),
                                                (
                                                    OutputPart::Picture { alt, .. },
                                                    Some(Ok(tex)),
                                                ) => {
                                                    let picture = egui::Image::new(
                                                        egui::load::SizedTexture::from_handle(tex),
                                                    )
                                                    .fit_to_original_size(1.0)
                                                    .max_width(ui.available_width().max(160.0))
                                                    .sense(egui::Sense::click());
                                                    // Notebooks are drawn for a white page,
                                                    // so a transparent plot gets one.
                                                    let response = egui::Frame::new()
                                                        .fill(Color32::WHITE)
                                                        .show(ui, |ui| ui.add(picture))
                                                        .inner
                                                        .on_hover_cursor(
                                                            egui::CursorIcon::PointingHand,
                                                        )
                                                        .on_hover_text(format!(
                                                            "{alt}\n{}",
                                                            octa::i18n::t("view.nb_picture_hint")
                                                        ));
                                                    if response.clicked() {
                                                        clicked = Some(idx);
                                                    }
                                                }
                                                (OutputPart::Picture { .. }, Some(Err(e))) => {
                                                    octa::ui::message::selectable_message(
                                                        ui,
                                                        colors.error,
                                                        &format!(
                                                            "{}: {e}",
                                                            octa::i18n::t("view.nb_picture_error")
                                                        ),
                                                    );
                                                }
                                                (OutputPart::Picture { .. }, None) => {}
                                            }
                                        }
                                    });
                                    clicked
                                });
                            let copy_output = output.to_string();
                            out_frame.response.context_menu(|ui| {
                                if ui.button(octa::i18n::t("view.nb_copy_output")).clicked() {
                                    ui.ctx().copy_text(copy_output.clone());
                                    ui.close();
                                }
                            });
                            out_frame.inner
                        };
                        let picture_bytes = |idx: usize| match parts.map(|p| &p[idx]) {
                            Some(OutputPart::Picture { bytes, .. }) => {
                                Some((row_idx, bytes.clone()))
                            }
                            _ => None,
                        };

                        // Beside: the source gets half the row (less its frame's
                        // margins and border, never under 200 px), or its
                        // full-width editor pushes the output off the screen.
                        let source_max_width =
                            (has_output && output_layout == NotebookOutputLayout::Beside).then(
                                || ((ui.available_width() - label_width) / 2.0 - 18.0).max(200.0),
                            );

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
                                .stroke(Stroke::new(1.0_f32, border_color))
                                .corner_radius(4.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    if let Some(w) = source_max_width {
                                        ui.set_max_width(w);
                                    }
                                    ui.horizontal_top(|ui| {
                                        // Line number gutter (not selectable)
                                        let gutter_job =
                                            build_line_numbers(line_count, line_num_color);
                                        ui.add(egui::Label::new(gutter_job).selectable(false));
                                        ui.add_space(8.0);
                                        // Editable source. Code cells keep syntect
                                        // highlighting while typing via a layouter (we
                                        // assume Python -- overwhelmingly what `.ipynb`
                                        // contains -- falling back to plain monospace if
                                        // Python isn't in the syntax set). Edits write
                                        // back through the table overlay so undo / save
                                        // / the modified marker all work.
                                        let mono =
                                            egui::FontId::new(13.0, egui::FontFamily::Monospace);
                                        let edit_id = egui::Id::new(("nb_src", row_idx));
                                        let mut buf = source.clone();
                                        let src_ranges: Vec<std::ops::Range<usize>> = matcher
                                            .as_ref()
                                            .map(|m| m.find_ranges(&source))
                                            .unwrap_or_default();
                                        let response = if is_code {
                                            let syntax = octa::ui::syntax::syntax_by_name("Python");
                                            let theme =
                                                octa::ui::syntax::theme_for_mode(theme_mode);
                                            let code_ranges = src_ranges.clone();
                                            let mut layouter = move |ui: &egui::Ui,
                                                                     text: &dyn egui::TextBuffer,
                                                                     _wrap: f32| {
                                                let font = egui::FontId::new(
                                                    13.0,
                                                    egui::FontFamily::Monospace,
                                                );
                                                let mut job = match syntax {
                                                    Some(syn) => {
                                                        octa::ui::syntax::highlight_layout_job(
                                                            text.as_str(),
                                                            syn,
                                                            theme,
                                                            font,
                                                        )
                                                    }
                                                    None => {
                                                        let mut j =
                                                            egui::text::LayoutJob::default();
                                                        j.wrap.max_width = f32::INFINITY;
                                                        j.append(
                                                            text.as_str(),
                                                            0.0,
                                                            egui::text::TextFormat {
                                                                font_id: font,
                                                                color: text_color,
                                                                ..Default::default()
                                                            },
                                                        );
                                                        j
                                                    }
                                                };
                                                ui::search_highlight::apply_highlight(
                                                    &mut job,
                                                    &code_ranges,
                                                    None,
                                                    hl_normal,
                                                    hl_active,
                                                );
                                                ui.fonts_mut(|f| f.layout_job(job))
                                            };
                                            ui.add(
                                                egui::TextEdit::multiline(&mut buf)
                                                    .id(edit_id)
                                                    .font(mono.clone())
                                                    .frame(egui::Frame::NONE)
                                                    .desired_width(f32::INFINITY)
                                                    .desired_rows(line_count.max(1))
                                                    .interactive(!readonly)
                                                    .layouter(&mut layouter),
                                            )
                                        } else {
                                            let md_ranges = src_ranges.clone();
                                            let mut md_layouter = move |ui: &egui::Ui,
                                                                        text: &dyn egui::TextBuffer,
                                                                        _wrap: f32| {
                                                let font = egui::FontId::new(
                                                    13.0,
                                                    egui::FontFamily::Monospace,
                                                );
                                                let mut job = egui::text::LayoutJob::default();
                                                job.wrap.max_width = f32::INFINITY;
                                                job.append(
                                                    text.as_str(),
                                                    0.0,
                                                    egui::text::TextFormat {
                                                        font_id: font,
                                                        color: text_color,
                                                        ..Default::default()
                                                    },
                                                );
                                                ui::search_highlight::apply_highlight(
                                                    &mut job,
                                                    &md_ranges,
                                                    None,
                                                    hl_normal,
                                                    hl_active,
                                                );
                                                ui.fonts_mut(|f| f.layout_job(job))
                                            };
                                            ui.add(
                                                egui::TextEdit::multiline(&mut buf)
                                                    .id(edit_id)
                                                    .font(mono.clone())
                                                    .frame(egui::Frame::NONE)
                                                    .desired_width(f32::INFINITY)
                                                    .desired_rows(line_count.max(1))
                                                    .interactive(!readonly)
                                                    .layouter(&mut md_layouter),
                                            )
                                        };
                                        if response.changed() && !readonly {
                                            tab.table.set(row_idx, 2, data::CellValue::String(buf));
                                        }
                                    });
                                });
                            let copy_source = source.clone();
                            let all_text = all_notebook_text.clone();
                            frame_response.response.context_menu(|ui| {
                                if ui.button(octa::i18n::t("view.nb_copy_cell")).clicked() {
                                    ui.ctx().copy_text(copy_source.clone());
                                    ui.close();
                                }
                                if ui.button(octa::i18n::t("view.nb_copy_all_cells")).clicked() {
                                    ui.ctx().copy_text(all_text.clone());
                                    ui.close();
                                }
                            });

                            // Output beside source (Beside layout)
                            if has_output
                                && output_layout == NotebookOutputLayout::Beside
                                && let Some(idx) =
                                    render_output(ui, cell_num, &output, border_color, output_hl)
                            {
                                clicked_picture = picture_bytes(idx);
                            }

                            // Highlight-search jump: scroll to the cell holding
                            // the current match.
                            if scroll_target_row == Some(row_idx) {
                                frame_response
                                    .response
                                    .scroll_to_me(Some(egui::Align::Center));
                            }
                        });

                        // Output beneath source (Beneath layout)
                        if has_output && output_layout == NotebookOutputLayout::Beneath {
                            ui.horizontal(|ui| {
                                // Indent to align under the source frame
                                ui.add_space(label_width + 8.0);
                                if let Some(idx) =
                                    render_output(ui, cell_num, &output, border_color, output_hl)
                                {
                                    clicked_picture = picture_bytes(idx);
                                }
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

    // Ctrl+X: copy all notebook content -- but only when no source-cell editor
    // is focused, so Ctrl+X inside a focused cell cuts that cell's text instead.
    let editor_focused = ctx.memory(|m| m.focused().is_some());
    if !editor_focused && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::X)) {
        ctx.copy_text(all_notebook_text);
    }
    clicked_picture
}

#[cfg(test)]
#[path = "notebook_tests.rs"]
mod tests;
