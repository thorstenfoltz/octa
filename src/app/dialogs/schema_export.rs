//! Schema Export dialog. Opens from **View → Export schema as…** and
//! renders the active tab's column list in one of the targets defined
//! in `octa::data::schema_export`. The user can switch target inside
//! the dialog without reopening, then **Copy** the result to the
//! clipboard or **Save as…** to disk.
//!
//! The rendering itself is delegated to the pure library functions —
//! this file only orchestrates the UI.

use std::io::Write;

use eframe::egui;
use egui::RichText;

use octa::data::schema_export::SchemaTarget;
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::{OctaApp, SchemaExportState};

pub(crate) fn render_schema_export_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.schema_export.as_ref() else {
        return;
    };
    let target = state.target;
    let mut size = state.size;

    // Capture the active tab's columns + a sensible default identifier
    // before opening the UI closure so we don't have to keep borrowing
    // `app` inside it.
    let (columns, table_name) = {
        let tab = &app.tabs[app.active_tab];
        let cols = tab.table.columns.clone();
        let name = tab
            .table
            .source_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "data".to_string());
        (cols, name)
    };

    let rendered = target.export(&columns, &table_name);

    let mut new_target = target;
    let mut close_requested = false;
    let mut copy_payload: Option<String> = None;
    let mut save_payload: Option<(String, &'static str)> = None;

    let mut window = egui::Window::new("Schema Export")
        .title_bar(false)
        .collapsible(false);
    window = match size {
        DialogSize::Maximized => window.fixed_rect(ctx.content_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => window
            .resizable(true)
            .default_width(720.0)
            .default_height(520.0)
            .min_width(420.0)
            .min_height(260.0),
    };
    let minimized = size == DialogSize::Minimized;

    window.show(ctx, |ui| {
        egui::Panel::top("schema_export_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Schema Export — {}", target.label()))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close_requested = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("schema_export_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                    if ui.button("Copy to clipboard").clicked() {
                        copy_payload = Some(rendered.clone());
                    }
                    if ui.button("Save as…").clicked() {
                        save_payload = Some((rendered.clone(), target.extension()));
                    }
                    ui.label(
                        RichText::new(format!("{} columns from {}", columns.len(), table_name))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Target:");
                    for &t in SchemaTarget::ALL {
                        if ui.selectable_label(t == target, t.label()).clicked() {
                            new_target = t;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut rendered.as_str())
                                .desired_width(f32::INFINITY)
                                .desired_rows(20)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                    });
            });
    });

    if let Some(payload) = copy_payload {
        ctx.copy_text(payload);
        app.status_message = Some((
            format!("Copied {} schema to clipboard", target.label()),
            std::time::Instant::now(),
        ));
    }

    if let Some((payload, ext)) = save_payload {
        save_to_disk(app, &payload, ext, &table_name, target.label());
    }

    // Apply target switch / window-size change after the closure so the
    // mutable borrows of `app` don't collide with the read-only one used
    // above for `columns` + `table_name`.
    if let Some(state) = app.schema_export.as_mut() {
        state.target = new_target;
        state.size = size;
    }

    if close_requested {
        app.schema_export = None;
    }
}

fn save_to_disk(
    app: &mut OctaApp,
    payload: &str,
    ext: &'static str,
    table_name: &str,
    target_label: &str,
) {
    let default_name = format!("{}_schema.{}", table_name, ext);
    let path = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter(target_label, &[ext])
        .save_file();
    let Some(path) = path else {
        return;
    };
    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            app.status_message = Some((
                format!("Schema export: create {}: {}", path.display(), e),
                std::time::Instant::now(),
            ));
            return;
        }
    };
    let mut writer = std::io::BufWriter::new(file);
    if let Err(e) = writer.write_all(payload.as_bytes()) {
        app.status_message = Some((
            format!("Schema export: write {}: {}", path.display(), e),
            std::time::Instant::now(),
        ));
        return;
    }
    app.status_message = Some((
        format!("Saved {} schema to {}", target_label, path.display()),
        std::time::Instant::now(),
    ));
}

/// Helper used by `toolbar_handler` + `shortcuts_dispatch` to open the
/// dialog. Always defaults to Postgres DDL; the chip row inside the
/// dialog (alphabetical, driven by `SchemaTarget::ALL`) lets the user
/// switch without reopening. Adding a "last picked target persists"
/// hook would mean another `AppSettings` field; we don't yet have a
/// reason to do it.
pub(crate) fn open(app: &mut OctaApp) {
    if app.tabs[app.active_tab].table.col_count() == 0 {
        app.status_message = Some((
            "Schema export: active tab has no columns".to_string(),
            std::time::Instant::now(),
        ));
        return;
    }
    app.schema_export = Some(SchemaExportState {
        target: SchemaTarget::PostgresSqlDdl,
        size: DialogSize::default(),
    });
}
