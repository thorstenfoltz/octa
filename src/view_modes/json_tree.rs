use crate::app::state::TabState;
use crate::ui;
use octa::data::json_util;

use eframe::egui;
use egui::{Color32, RichText};
use ui::theme::ThemeMode;

/// Which TabState field the tree view is bound to, plus how to serialize
/// edits back into `raw_content`. Lets `render_json_tree_view` and
/// `render_yaml_tree_view` share one implementation.
#[derive(Clone, Copy)]
enum TreeKind {
    Json,
    Yaml,
}

impl TreeKind {
    fn value(self, tab: &TabState) -> Option<&serde_json::Value> {
        match self {
            TreeKind::Json => tab.json_value.as_ref(),
            TreeKind::Yaml => tab.yaml_value.as_ref(),
        }
    }
    fn value_mut(self, tab: &mut TabState) -> Option<&mut serde_json::Value> {
        match self {
            TreeKind::Json => tab.json_value.as_mut(),
            TreeKind::Yaml => tab.yaml_value.as_mut(),
        }
    }
    fn serialize_pretty(self, value: &serde_json::Value) -> Option<String> {
        match self {
            TreeKind::Json => serde_json::to_string_pretty(value).ok(),
            TreeKind::Yaml => serde_yaml_ng::to_string(value).ok(),
        }
    }
    fn copy_label(self) -> &'static str {
        match self {
            TreeKind::Json => "Copy JSON",
            TreeKind::Yaml => "Copy YAML",
        }
    }
    fn unavailable_label(self) -> &'static str {
        match self {
            TreeKind::Json => "JSON tree view is not available",
            TreeKind::Yaml => "YAML tree view is not available",
        }
    }
}

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
    render_value_tree(ui, tab, theme_mode, TreeKind::Json);
}

/// Render the interactive YAML tree view. Shares the JSON tree's renderer —
/// YAML is parsed once at load time, converted to a `serde_json::Value`, and
/// stored on `TabState.yaml_value`. Edits are serialized back as YAML when
/// the user commits a leaf change.
pub fn render_yaml_tree_view(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode) {
    render_value_tree(ui, tab, theme_mode, TreeKind::Yaml);
}

fn render_value_tree(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode, kind: TreeKind) {
    if kind.value(tab).is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(kind.unavailable_label())
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    // Overriding the local selection background prevents the
    // accent-on-accent collapse the user reported: drag-selected text in
    // tree labels was nearly invisible because keys are rendered in
    // `colors.accent` and the theme's `selection.bg_fill` is the same
    // accent at low opacity.
    let select_bg = if theme_mode.is_dark() {
        Color32::from_rgba_unmultiplied(80, 80, 80, 200)
    } else {
        Color32::from_rgba_unmultiplied(40, 40, 40, 80)
    };
    ui.style_mut().visuals.selection.bg_fill = select_bg;

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
        if response.changed()
            && let Ok(n) = tab.json_expand_depth_str.parse::<usize>()
        {
            tab.json_expand_depth = n.min(file_max_depth);
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
        if let Some(v) = kind.value(tab) {
            tab.json_tree_expanded = json_util::collect_json_paths(v, None);
        }
    } else if collapse_all {
        tab.json_tree_expanded.clear();
    } else if let Some(d) = apply_depth
        && let Some(v) = kind.value(tab)
    {
        tab.json_tree_expanded = json_util::collect_json_paths(v, Some(d));
    }

    let remaining_rect = ui.available_rect_before_wrap();
    let bg_response = ui.interact(
        remaining_rect,
        ui.id().with("json_tree_ctx"),
        egui::Sense::click(),
    );

    // Bind directly to the matching tab field rather than going through
    // `kind.value(tab)` so the borrow targets a specific field, leaving the
    // closure below free to mutably borrow `tab.json_edit_*` and `tab.json_tree_expanded`.
    let value_ref: &serde_json::Value = match kind {
        TreeKind::Json => tab.json_value.as_ref().expect("checked above"),
        TreeKind::Yaml => tab.yaml_value.as_ref().expect("checked above"),
    };
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
    let mut key_edit_request: Option<(String, String)> = None;
    let mut add_key_request: Option<String> = None;
    let mut edit_commit = false;
    let mut edit_cancel = false;
    let mut key_edit_commit = false;
    let mut key_edit_cancel = false;
    let mut add_key_commit = false;
    let mut add_key_cancel = false;

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
                            if let Some(req) = render_key_or_edit(
                                ui,
                                row.key.as_deref(),
                                row.is_index,
                                &row.path,
                                tab.tree_key_edit_path.as_deref(),
                                &mut tab.tree_key_edit_buffer,
                                &colors,
                            ) {
                                key_edit_request = Some(req);
                            }
                            if *is_expanded {
                                let opener = if *is_object { "{" } else { "[" };
                                ui.label(
                                    RichText::new(opener)
                                        .font(mono())
                                        .color(colors.text_primary),
                                );
                                if *is_object
                                    && ui.small_button("+").on_hover_text("Add key").clicked()
                                {
                                    add_key_request = Some(row.path.clone());
                                }
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
                            // Inline new-key prompt rendered immediately
                            // beneath the open object's "{". Lets the user
                            // type the new key name; Enter commits, Esc cancels.
                            if *is_expanded
                                && *is_object
                                && tab.tree_add_key_path.as_deref() == Some(&row.path)
                            {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("new key:")
                                        .font(mono())
                                        .color(colors.text_muted),
                                );
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut tab.tree_add_key_buffer)
                                        .font(mono())
                                        .desired_width(140.0),
                                );
                                if !resp.has_focus() && !resp.gained_focus() {
                                    resp.request_focus();
                                }
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
                            if let Some(req) = render_key_or_edit(
                                ui,
                                row.key.as_deref(),
                                row.is_index,
                                &row.path,
                                tab.tree_key_edit_path.as_deref(),
                                &mut tab.tree_key_edit_buffer,
                                &colors,
                            ) {
                                key_edit_request = Some(req);
                            }
                            let is_editing = tab.json_edit_path.as_deref() == Some(&row.path);
                            if is_editing {
                                if tab.json_edit_width.is_none() {
                                    let display = leaf_display(value, comma);
                                    let measured = ui.fonts_mut(|f| {
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
    if tab.tree_key_edit_path.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            key_edit_cancel = true;
        } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            key_edit_commit = true;
        }
    }
    if tab.tree_add_key_path.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            add_key_cancel = true;
        } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            add_key_commit = true;
        }
    }

    if let Some(p) = toggle_path
        && !tab.json_tree_expanded.remove(&p)
    {
        tab.json_tree_expanded.insert(p);
    }
    if let Some((path, buf)) = edit_request {
        tab.json_edit_path = Some(path);
        tab.json_edit_buffer = buf;
        tab.json_edit_width = None;
    }
    if let Some((path, buf)) = key_edit_request {
        tab.tree_key_edit_path = Some(path);
        tab.tree_key_edit_buffer = buf;
    }
    if let Some(parent_path) = add_key_request {
        tab.tree_add_key_path = Some(parent_path);
        tab.tree_add_key_buffer.clear();
    }
    if edit_commit {
        if let Some(ref edit_path) = tab.json_edit_path.clone() {
            let new_value = json_util::parse_json_edit(&tab.json_edit_buffer);
            let mutated = match kind.value_mut(tab) {
                Some(root) => json_util::set_json_value_at_path(root, edit_path, new_value).is_ok(),
                None => false,
            };
            if mutated
                && let Some(serialized) = kind.value(tab).and_then(|v| kind.serialize_pretty(v))
            {
                tab.raw_content = Some(serialized);
                tab.raw_content_modified = true;
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

    if key_edit_commit {
        if let Some(row_path) = tab.tree_key_edit_path.clone() {
            let new_key = tab.tree_key_edit_buffer.trim().to_string();
            if let Some((parent_path, old_key)) = split_key_path(&row_path) {
                let mutated = match kind.value_mut(tab) {
                    Some(root) => {
                        json_util::rename_object_key_at_path(root, &parent_path, &old_key, &new_key)
                            .is_ok()
                    }
                    None => false,
                };
                if mutated
                    && let Some(serialized) = kind.value(tab).and_then(|v| kind.serialize_pretty(v))
                {
                    tab.raw_content = Some(serialized);
                    tab.raw_content_modified = true;
                }
            }
        }
        tab.tree_key_edit_path = None;
        tab.tree_key_edit_buffer.clear();
    } else if key_edit_cancel {
        tab.tree_key_edit_path = None;
        tab.tree_key_edit_buffer.clear();
    }

    if add_key_commit {
        if let Some(parent_path) = tab.tree_add_key_path.clone() {
            let new_key = tab.tree_add_key_buffer.trim().to_string();
            if !new_key.is_empty() {
                let mutated = match kind.value_mut(tab) {
                    Some(root) => json_util::add_object_key_at_path(
                        root,
                        &parent_path,
                        &new_key,
                        serde_json::Value::String(String::new()),
                    )
                    .is_ok(),
                    None => false,
                };
                if mutated
                    && let Some(serialized) = kind.value(tab).and_then(|v| kind.serialize_pretty(v))
                {
                    tab.raw_content = Some(serialized);
                    tab.raw_content_modified = true;
                }
            }
        }
        tab.tree_add_key_path = None;
        tab.tree_add_key_buffer.clear();
    } else if add_key_cancel {
        tab.tree_add_key_path = None;
        tab.tree_add_key_buffer.clear();
    }

    let copy_label = kind.copy_label();
    bg_response.context_menu(|ui| {
        if ui.button(copy_label).clicked() {
            let s = tab.raw_content.clone().unwrap_or_else(|| {
                kind.value(tab)
                    .and_then(|v| kind.serialize_pretty(v))
                    .unwrap_or_default()
            });
            ui.ctx().copy_text(s);
            ui.close();
        }
    });

    if tab.json_edit_path.is_none()
        && ui.input(|i| {
            i.modifiers.command && (i.key_pressed(egui::Key::C) || i.key_pressed(egui::Key::X))
        })
    {
        let s = tab.raw_content.clone().unwrap_or_else(|| {
            kind.value(tab)
                .and_then(|v| kind.serialize_pretty(v))
                .unwrap_or_default()
        });
        ui.ctx().copy_text(s);
    }
}

fn mono() -> egui::FontId {
    egui::FontId::new(13.0, egui::FontFamily::Monospace)
}

/// Decompose a row path into (parent_path, key). Returns `None` for array
/// elements (paths ending in `]`) since those are positional, not named.
/// Examples:
///   "users[0].name"  -> Some(("users[0]", "name"))
///   "name"           -> Some(("", "name"))
///   "users[0]"       -> None
fn split_key_path(path: &str) -> Option<(String, String)> {
    if path.ends_with(']') {
        return None;
    }
    if let Some(idx) = path.rfind('.') {
        let parent = &path[..idx];
        let key = &path[idx + 1..];
        Some((parent.to_string(), key.to_string()))
    } else if let Some(idx) = path.rfind(']') {
        // path of the form "arr[2].key" handled above; here we'd have
        // something like "key" — fall through to the no-parent branch.
        let parent = &path[..idx + 1];
        let key = &path[idx + 1..];
        if key.is_empty() {
            None
        } else {
            Some((parent.to_string(), key.to_string()))
        }
    } else {
        Some((String::new(), path.to_string()))
    }
}

/// Render a row's key. Returns `Some((row_path, initial_buffer))` when the
/// user double-clicked an editable (non-index) key, signalling the caller to
/// enter rename mode. Array indices are non-editable — JSON/YAML arrays
/// don't have user-facing labels so `[0]` etc. are display-only.
fn render_key_or_edit(
    ui: &mut egui::Ui,
    key: Option<&str>,
    is_index: bool,
    row_path: &str,
    edit_path: Option<&str>,
    edit_buffer: &mut String,
    colors: &ui::theme::ThemeColors,
) -> Option<(String, String)> {
    let k = key?;

    let editing = !is_index && edit_path == Some(row_path);
    if editing {
        // Inline TextEdit for the renamed key. Width tracks the buffer so
        // typing a longer name doesn't get visually clipped.
        let measured = ui
            .fonts_mut(|f| {
                f.layout_no_wrap(edit_buffer.clone(), mono(), colors.text_primary)
                    .size()
                    .x
            })
            .max(60.0)
            + 16.0;
        let response = ui.add(
            egui::TextEdit::singleline(edit_buffer)
                .font(mono())
                .desired_width(measured),
        );
        if !response.has_focus() && !response.gained_focus() {
            response.request_focus();
        }
        ui.label(RichText::new(":").font(mono()).color(colors.accent));
        ui.add_space(4.0);
        return None;
    }

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
    let resp = ui.add(
        egui::Label::new(RichText::new(label).font(mono()).color(key_color))
            .selectable(true)
            .sense(egui::Sense::click()),
    );
    let mut request = None;
    if resp.double_clicked() && !is_index {
        request = Some((row_path.to_string(), k.to_string()));
    }
    ui.add_space(4.0);
    request
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
