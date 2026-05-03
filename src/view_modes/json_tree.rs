use crate::app::state::TabState;
use crate::ui;
use octa::data::json_util;

use eframe::egui;
use egui::{Color32, RichText};
use ui::theme::ThemeMode;

/// Approximate height of one tree row at the default 13pt monospace font.
/// Used as the constant row height for `ScrollArea::show_rows` virtualization.
/// If the actual row layout exceeds this, egui clips per-row but the column
/// scroll bar will be slightly off — close enough for the use case.
const JSON_ROW_HEIGHT: f32 = 18.0;

/// One renderable row in the flattened JSON tree.
struct JsonRow<'a> {
    path: String,
    depth: usize,
    key: Option<String>,
    is_index: bool,
    is_last: bool,
    kind: JsonRowKind<'a>,
}

enum JsonRowKind<'a> {
    /// Opening line of an object/array (with arrow).
    Open {
        is_object: bool,
        count: usize,
        is_expanded: bool,
    },
    /// Closing brace/bracket of an object/array.
    Close { is_object: bool },
    /// Leaf value (string/number/bool/null).
    Leaf { value: &'a serde_json::Value },
}

/// Render the interactive JSON tree view (Firefox-style collapsible tree).
pub fn render_json_tree_view(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode) {
    if tab.json_value.is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("JSON tree view is not available")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let file_max_depth = tab.json_file_max_depth;

    if tab.json_expand_depth > file_max_depth {
        tab.json_expand_depth = file_max_depth;
        tab.json_expand_depth_str = tab.json_expand_depth.to_string();
    }

    let mut apply_depth: Option<usize> = None;
    let mut expand_all = false;
    let mut collapse_all = false;

    ui.horizontal(|ui| {
        if ui.button("Expand All").clicked() {
            expand_all = true;
        }
        if ui.button("Collapse All").clicked() {
            collapse_all = true;
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
            tab.json_expand_depth = tab.json_expand_depth.min(file_max_depth);
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();
        }
        ui.label(format!("/ {file_max_depth}"));
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Apply").clicked() || enter_pressed {
            apply_depth = Some(tab.json_expand_depth);
        }
    });
    ui.add_space(4.0);

    if expand_all {
        if let Some(ref v) = tab.json_value {
            tab.json_tree_expanded = json_util::collect_json_paths(v, None);
        }
    } else if collapse_all {
        tab.json_tree_expanded.clear();
    } else if let Some(d) = apply_depth {
        if let Some(ref v) = tab.json_value {
            tab.json_tree_expanded = json_util::collect_json_paths(v, Some(d));
        }
    }

    let remaining_rect = ui.available_rect_before_wrap();
    let bg_response = ui.interact(
        remaining_rect,
        ui.id().with("json_tree_ctx"),
        egui::Sense::click(),
    );

    let value_ref = tab.json_value.as_ref().expect("checked above");
    let mut rows: Vec<JsonRow<'_>> = Vec::new();
    flatten(
        value_ref,
        "",
        None,
        false,
        0,
        true,
        &tab.json_tree_expanded,
        &mut rows,
    );

    let mut toggle_path: Option<String> = None;
    let mut edit_request: Option<(String, String)> = None;
    let mut edit_commit = false;
    let mut edit_cancel = false;

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show_rows(ui, JSON_ROW_HEIGHT, rows.len(), |ui, range| {
            ui.add_space(8.0);
            for i in range {
                let row = &rows[i];
                let comma = if row.is_last { "" } else { "," };
                ui.horizontal(|ui| {
                    ui.add_space(16.0 + row.depth as f32 * 20.0);
                    match &row.kind {
                        JsonRowKind::Open {
                            is_object,
                            count,
                            is_expanded,
                        } => {
                            let arrow = if *is_expanded { "\u{25BC}" } else { "\u{25B6}" };
                            if ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(arrow).font(mono()).color(colors.text_muted),
                                    )
                                    .selectable(false)
                                    .sense(egui::Sense::click()),
                                )
                                .clicked()
                            {
                                toggle_path = Some(row.path.clone());
                            }
                            render_key(ui, row.key.as_deref(), row.is_index, &colors);
                            if *is_expanded {
                                let opener = if *is_object { "{" } else { "[" };
                                ui.label(
                                    RichText::new(opener)
                                        .font(mono())
                                        .color(colors.text_primary),
                                );
                            } else {
                                let summary = if *is_object {
                                    format!("{{...}} ({count} keys){comma}")
                                } else {
                                    format!("[...] ({count} items){comma}")
                                };
                                ui.label(
                                    RichText::new(summary).font(mono()).color(colors.text_muted),
                                );
                            }
                        }
                        JsonRowKind::Close { is_object } => {
                            let closer = if *is_object { "}" } else { "]" };
                            ui.label(
                                RichText::new(format!("{closer}{comma}"))
                                    .font(mono())
                                    .color(colors.text_primary),
                            );
                        }
                        JsonRowKind::Leaf { value } => {
                            ui.add_space(18.0);
                            render_key(ui, row.key.as_deref(), row.is_index, &colors);
                            let is_editing = tab.json_edit_path.as_deref() == Some(&row.path);
                            if is_editing {
                                if tab.json_edit_width.is_none() {
                                    let display = leaf_display(value, comma);
                                    let measured = ui.fonts(|f| {
                                        f.layout_no_wrap(display, mono(), colors.text_primary)
                                            .size()
                                            .x
                                    });
                                    tab.json_edit_width = Some(measured.max(200.0) + 16.0);
                                }
                                let width = tab.json_edit_width.unwrap_or(200.0);
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut tab.json_edit_buffer)
                                        .font(mono())
                                        .desired_width(width)
                                        .min_size(egui::vec2(width, 0.0)),
                                );
                                if !response.has_focus() && !response.gained_focus() {
                                    response.request_focus();
                                }
                                ui.label(
                                    RichText::new(comma).font(mono()).color(colors.text_muted),
                                );
                            } else {
                                let display = leaf_display(value, comma);
                                let color = json_value_color(value, &colors);
                                let response = ui.add(
                                    egui::Label::new(
                                        RichText::new(display).font(mono()).color(color),
                                    )
                                    .selectable(true)
                                    .sense(egui::Sense::click()),
                                );
                                if response.double_clicked() {
                                    edit_request = Some((row.path.clone(), leaf_edit_text(value)));
                                }
                            }
                        }
                    }
                });
            }
        });

    if tab.json_edit_path.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            edit_cancel = true;
        } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            edit_commit = true;
        }
    }

    if let Some(p) = toggle_path {
        if !tab.json_tree_expanded.remove(&p) {
            tab.json_tree_expanded.insert(p);
        }
    }
    if let Some((path, buf)) = edit_request {
        tab.json_edit_path = Some(path);
        tab.json_edit_buffer = buf;
        tab.json_edit_width = None;
    }
    if edit_commit {
        if let Some(ref edit_path) = tab.json_edit_path.clone() {
            let new_value = json_util::parse_json_edit(&tab.json_edit_buffer);
            if let Some(ref mut root) = tab.json_value {
                if json_util::set_json_value_at_path(root, edit_path, new_value).is_ok() {
                    tab.raw_content = Some(serde_json::to_string_pretty(root).unwrap_or_default());
                    tab.raw_content_modified = true;
                }
            }
        }
        tab.json_edit_path = None;
        tab.json_edit_buffer.clear();
        tab.json_edit_width = None;
    } else if edit_cancel {
        tab.json_edit_path = None;
        tab.json_edit_buffer.clear();
        tab.json_edit_width = None;
    }

    bg_response.context_menu(|ui| {
        if ui.button("Copy JSON").clicked() {
            let s = tab
                .raw_content
                .clone()
                .unwrap_or_else(|| match tab.json_value.as_ref() {
                    Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
                    None => String::new(),
                });
            ui.ctx().copy_text(s);
            ui.close_menu();
        }
    });

    if tab.json_edit_path.is_none()
        && ui.input(|i| {
            i.modifiers.command && (i.key_pressed(egui::Key::C) || i.key_pressed(egui::Key::X))
        })
    {
        let s = tab
            .raw_content
            .clone()
            .unwrap_or_else(|| match tab.json_value.as_ref() {
                Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
                None => String::new(),
            });
        ui.ctx().copy_text(s);
    }
}

fn mono() -> egui::FontId {
    egui::FontId::new(13.0, egui::FontFamily::Monospace)
}

fn render_key(
    ui: &mut egui::Ui,
    key: Option<&str>,
    is_index: bool,
    colors: &ui::theme::ThemeColors,
) {
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
            egui::Label::new(RichText::new(label).font(mono()).color(key_color)).selectable(true),
        );
        ui.add_space(4.0);
    }
}

fn leaf_display(value: &serde_json::Value, comma: &str) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\"{comma}"),
        serde_json::Value::Number(n) => format!("{n}{comma}"),
        serde_json::Value::Bool(b) => format!("{b}{comma}"),
        serde_json::Value::Null => format!("null{comma}"),
        _ => String::new(),
    }
}

fn leaf_edit_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => String::new(),
    }
}

fn json_value_color(value: &serde_json::Value, colors: &ui::theme::ThemeColors) -> Color32 {
    match value {
        serde_json::Value::String(_) => Color32::from_rgb(10, 140, 70),
        serde_json::Value::Number(_) => Color32::from_rgb(30, 100, 200),
        serde_json::Value::Bool(_) => Color32::from_rgb(180, 80, 180),
        serde_json::Value::Null => colors.text_muted,
        _ => colors.text_primary,
    }
}

/// DFS pre-order walk of the JSON value, emitting one [`JsonRow`] per visible
/// line. Honors the `expanded` set — collapsed subtrees produce a single row
/// summary and skip their descendants.
#[allow(clippy::too_many_arguments)]
fn flatten<'a>(
    value: &'a serde_json::Value,
    path: &str,
    key: Option<&str>,
    is_index: bool,
    depth: usize,
    is_last: bool,
    expanded: &std::collections::HashSet<String>,
    out: &mut Vec<JsonRow<'a>>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let is_expanded = expanded.contains(path);
            out.push(JsonRow {
                path: path.to_string(),
                depth,
                key: key.map(str::to_string),
                is_index,
                is_last,
                kind: JsonRowKind::Open {
                    is_object: true,
                    count: map.len(),
                    is_expanded,
                },
            });
            if is_expanded {
                let n = map.len();
                for (i, (k, v)) in map.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    flatten(
                        v,
                        &child_path,
                        Some(k),
                        false,
                        depth + 1,
                        i + 1 == n,
                        expanded,
                        out,
                    );
                }
                out.push(JsonRow {
                    path: path.to_string(),
                    depth,
                    key: None,
                    is_index: false,
                    is_last,
                    kind: JsonRowKind::Close { is_object: true },
                });
            }
        }
        serde_json::Value::Array(arr) => {
            let is_expanded = expanded.contains(path);
            out.push(JsonRow {
                path: path.to_string(),
                depth,
                key: key.map(str::to_string),
                is_index,
                is_last,
                kind: JsonRowKind::Open {
                    is_object: false,
                    count: arr.len(),
                    is_expanded,
                },
            });
            if is_expanded {
                let n = arr.len();
                for (i, v) in arr.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{path}[{i}]")
                    };
                    let key_owned = i.to_string();
                    flatten(
                        v,
                        &child_path,
                        Some(&key_owned),
                        true,
                        depth + 1,
                        i + 1 == n,
                        expanded,
                        out,
                    );
                }
                out.push(JsonRow {
                    path: path.to_string(),
                    depth,
                    key: None,
                    is_index: false,
                    is_last,
                    kind: JsonRowKind::Close { is_object: false },
                });
            }
        }
        _ => {
            out.push(JsonRow {
                path: path.to_string(),
                depth,
                key: key.map(str::to_string),
                is_index,
                is_last,
                kind: JsonRowKind::Leaf { value },
            });
        }
    }
}
