use crate::app::state::TabState;
use crate::ui;
use octa::data::json_util;

use eframe::egui;
use egui::{Color32, RichText};
use ui::theme::ThemeMode;

/// Render the interactive JSON tree view (Firefox-style collapsible tree).
pub fn render_json_tree_view(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode) {
    let Some(ref value) = tab.json_value else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("JSON tree view is not available")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    };

    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let value = value.clone();
    let file_max_depth = json_util::max_json_depth(&value);

    // Clamp expand_depth to this file's max
    if tab.json_expand_depth > file_max_depth {
        tab.json_expand_depth = file_max_depth;
        tab.json_expand_depth_str = tab.json_expand_depth.to_string();
    }

    // --- Expand/Collapse toolbar ---
    ui.horizontal(|ui| {
        if ui.button("Expand All").clicked() {
            tab.json_tree_expanded = json_util::collect_json_paths(&value, None);
        }
        if ui.button("Collapse All").clicked() {
            tab.json_tree_expanded.clear();
        }
        ui.separator();
        ui.label("Depth:");
        let response = ui.add(
            egui::TextEdit::singleline(&mut tab.json_expand_depth_str)
                .desired_width(30.0)
                .horizontal_align(egui::Align::Center),
        );
        if response.changed() {
            if let Ok(n) = tab.json_expand_depth_str.parse::<usize>() {
                tab.json_expand_depth = n.min(file_max_depth);
            }
        }
        if response.lost_focus() {
            // On blur, clamp and re-sync the string
            tab.json_expand_depth = tab.json_expand_depth.min(file_max_depth);
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();
        }
        ui.label(format!("/ {file_max_depth}"));
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Apply").clicked() || enter_pressed {
            tab.json_tree_expanded =
                json_util::collect_json_paths(&value, Some(tab.json_expand_depth));
        }
    });
    ui.add_space(4.0);

    let json_text = tab
        .raw_content
        .clone()
        .unwrap_or_else(|| serde_json::to_string_pretty(&value).unwrap_or_default());

    // Background interact covering only the remaining area (below the toolbar).
    // Placed BEFORE the scroll area so tree nodes drawn later take click priority.
    let remaining_rect = ui.available_rect_before_wrap();
    let bg_response = ui.interact(
        remaining_rect,
        ui.id().with("json_tree_ctx"),
        egui::Sense::click(),
    );

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    render_json_node(
                        ui,
                        &value,
                        "",
                        None,
                        false,
                        0,
                        &colors,
                        &mut tab.json_tree_expanded,
                        true,
                        &mut tab.json_edit_path,
                        &mut tab.json_edit_buffer,
                        &mut tab.json_edit_width,
                    );
                });
            });
        });

    // Apply pending edit if confirmed
    if let Some(ref edit_path) = tab.json_edit_path.clone() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            tab.json_edit_path = None;
            tab.json_edit_buffer.clear();
            tab.json_edit_width = None;
        } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let new_value = json_util::parse_json_edit(&tab.json_edit_buffer);
            if let Some(ref mut root) = tab.json_value {
                if json_util::set_json_value_at_path(root, edit_path, new_value).is_ok() {
                    tab.raw_content = Some(serde_json::to_string_pretty(root).unwrap_or_default());
                    tab.raw_content_modified = true;
                }
            }
            tab.json_edit_path = None;
            tab.json_edit_buffer.clear();
            tab.json_edit_width = None;
        }
    }

    // Right-click context menu on the background
    bg_response.context_menu(|ui| {
        if ui.button("Copy JSON").clicked() {
            ui.ctx().copy_text(json_text.clone());
            ui.close_menu();
        }
    });

    // Ctrl+C / Ctrl+X: copy full JSON (only when not editing)
    if tab.json_edit_path.is_none()
        && ui.input(|i| {
            i.modifiers.command && (i.key_pressed(egui::Key::C) || i.key_pressed(egui::Key::X))
        })
    {
        ui.ctx().copy_text(json_text);
    }
}

/// Colors for different JSON value types.
fn json_value_color(value: &serde_json::Value, colors: &ui::theme::ThemeColors) -> Color32 {
    match value {
        serde_json::Value::String(_) => Color32::from_rgb(10, 140, 70),
        serde_json::Value::Number(_) => Color32::from_rgb(30, 100, 200),
        serde_json::Value::Bool(_) => Color32::from_rgb(180, 80, 180),
        serde_json::Value::Null => colors.text_muted,
        _ => colors.text_primary,
    }
}

/// Extract the raw display text for a leaf value (without quotes for strings).
fn leaf_edit_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => String::new(),
    }
}

/// Render a single JSON node recursively.
#[allow(clippy::too_many_arguments)]
fn render_json_node(
    ui: &mut egui::Ui,
    value: &serde_json::Value,
    path: &str,
    key: Option<&str>,
    is_index: bool,
    depth: usize,
    colors: &ui::theme::ThemeColors,
    expanded: &mut std::collections::HashSet<String>,
    is_last: bool,
    edit_path: &mut Option<String>,
    edit_buffer: &mut String,
    edit_width: &mut Option<f32>,
) {
    let indent = depth as f32 * 20.0;
    let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let comma = if is_last { "" } else { "," };

    // Render the key label: "key": for object keys, index: for array items
    let show_key = |ui: &mut egui::Ui| {
        if let Some(k) = key {
            let label = if is_index {
                format!("{k}:")
            } else {
                format!("\"{k}\":")
            };
            let key_color = if is_index {
                colors.text_muted
            } else {
                colors.accent
            };
            ui.add(
                egui::Label::new(RichText::new(label).font(mono.clone()).color(key_color))
                    .selectable(true),
            );
            ui.add_space(4.0);
        }
    };

    match value {
        serde_json::Value::Object(map) => {
            let is_expanded = expanded.contains(path);
            let count = map.len();

            ui.horizontal(|ui| {
                ui.add_space(indent);
                let arrow = if is_expanded { "\u{25BC}" } else { "\u{25B6}" };
                if ui
                    .add(
                        egui::Label::new(
                            RichText::new(arrow)
                                .font(mono.clone())
                                .color(colors.text_muted),
                        )
                        .selectable(false)
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    if is_expanded {
                        expanded.remove(path);
                    } else {
                        expanded.insert(path.to_string());
                    }
                }
                show_key(ui);
                if is_expanded {
                    ui.label(
                        RichText::new("{")
                            .font(mono.clone())
                            .color(colors.text_primary),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("{{...}} ({count} keys){comma}"))
                            .font(mono.clone())
                            .color(colors.text_muted),
                    );
                }
            });

            if is_expanded {
                let entries: Vec<_> = map.iter().collect();
                for (i, (k, v)) in entries.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        k.to_string()
                    } else {
                        format!("{path}.{k}")
                    };
                    render_json_node(
                        ui,
                        v,
                        &child_path,
                        Some(k),
                        false,
                        depth + 1,
                        colors,
                        expanded,
                        i == entries.len() - 1,
                        edit_path,
                        edit_buffer,
                        edit_width,
                    );
                }
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.label(
                        RichText::new(format!("}}{comma}"))
                            .font(mono.clone())
                            .color(colors.text_primary),
                    );
                });
            }
        }
        serde_json::Value::Array(arr) => {
            let is_expanded = expanded.contains(path);
            let count = arr.len();

            ui.horizontal(|ui| {
                ui.add_space(indent);
                let arrow = if is_expanded { "\u{25BC}" } else { "\u{25B6}" };
                if ui
                    .add(
                        egui::Label::new(
                            RichText::new(arrow)
                                .font(mono.clone())
                                .color(colors.text_muted),
                        )
                        .selectable(false)
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    if is_expanded {
                        expanded.remove(path);
                    } else {
                        expanded.insert(path.to_string());
                    }
                }
                show_key(ui);
                if is_expanded {
                    ui.label(
                        RichText::new("[")
                            .font(mono.clone())
                            .color(colors.text_primary),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("[...] ({count} items){comma}"))
                            .font(mono.clone())
                            .color(colors.text_muted),
                    );
                }
            });

            if is_expanded {
                for (i, v) in arr.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{path}[{i}]")
                    };
                    render_json_node(
                        ui,
                        v,
                        &child_path,
                        Some(&i.to_string()),
                        true,
                        depth + 1,
                        colors,
                        expanded,
                        i == arr.len() - 1,
                        edit_path,
                        edit_buffer,
                        edit_width,
                    );
                }
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.label(
                        RichText::new(format!("]{comma}"))
                            .font(mono.clone())
                            .color(colors.text_primary),
                    );
                });
            }
        }
        // Leaf values (string, number, bool, null)
        _ => {
            let is_editing = edit_path.as_deref() == Some(path);
            ui.horizontal(|ui| {
                ui.add_space(indent);
                ui.add_space(18.0); // Align with content after arrows
                show_key(ui);
                if is_editing {
                    if edit_width.is_none() {
                        let display = match value {
                            serde_json::Value::String(s) => format!("\"{s}\"{comma}"),
                            serde_json::Value::Number(n) => format!("{n}{comma}"),
                            serde_json::Value::Bool(b) => format!("{b}{comma}"),
                            serde_json::Value::Null => format!("null{comma}"),
                            _ => unreachable!(),
                        };
                        let measured = ui.fonts(|f| {
                            f.layout_no_wrap(display, mono.clone(), colors.text_primary)
                                .size()
                                .x
                        });
                        *edit_width = Some(measured.max(200.0) + 16.0);
                    }
                    let width = edit_width.unwrap_or(200.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(edit_buffer)
                            .font(mono.clone())
                            .desired_width(width)
                            .min_size(egui::vec2(width, 0.0)),
                    );
                    // Auto-focus on first frame
                    if !response.has_focus() && !response.gained_focus() {
                        response.request_focus();
                    }
                    ui.label(
                        RichText::new(comma)
                            .font(mono.clone())
                            .color(colors.text_muted),
                    );
                } else {
                    let display = match value {
                        serde_json::Value::String(s) => format!("\"{s}\"{comma}"),
                        serde_json::Value::Number(n) => format!("{n}{comma}"),
                        serde_json::Value::Bool(b) => format!("{b}{comma}"),
                        serde_json::Value::Null => format!("null{comma}"),
                        _ => unreachable!(),
                    };
                    let color = json_value_color(value, colors);
                    let response = ui.add(
                        egui::Label::new(RichText::new(display).font(mono.clone()).color(color))
                            .selectable(true)
                            .sense(egui::Sense::click()),
                    );
                    if response.double_clicked() {
                        *edit_path = Some(path.to_string());
                        *edit_buffer = leaf_edit_text(value);
                        *edit_width = None;
                    }
                }
            });
        }
    }
}
