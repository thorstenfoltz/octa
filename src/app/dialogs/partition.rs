//! Split-into-files dialog (Data -> Split into files...).
//!
//! The user optionally picks a column, a row limit and a size limit, an output
//! directory and a format override. Apply runs `data::partition::write_split`
//! on a worker thread (the one write path the CLI and MCP tool use too), with
//! progress and Cancel; `drain_partition` reports the result.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{DialogSize, dialog_header, remember_dialog_rect, size_dialog_window};

use crate::app::state::{OctaApp, PartitionState};

pub(crate) fn render_partition_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.partition_dialog.is_none() {
        return;
    }

    // Need an active tab with at least one column.
    if app.tabs.is_empty() || app.tabs[app.active_tab].table.col_count() == 0 {
        app.partition_dialog = None;
        return;
    }

    // A paged very large file holds one page only; a split would write just it.
    if app.tabs[app.active_tab].large.is_some() {
        app.status_message = Some((
            octa::i18n::t("partition.large_refused"),
            std::time::Instant::now(),
        ));
        app.partition_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut load_rest = false;
    let mut st = app.partition_dialog.take().unwrap();

    // Clamp column index in case the table changed while the dialog was open.
    let col_count = app.tabs[app.active_tab].table.col_count();
    if st.col.is_some_and(|c| c >= col_count) {
        st.col = Some(0);
    }
    let running = st.running.load(Ordering::Acquire);

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_partition_dialog");
    let window = egui::Window::new("octa_partition")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(380.0)
            .min_width(320.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "partition_header",
            RichText::new(octa::i18n::t("partition.title")),
            &mut size,
        ) {
            close = true;
        }

        if minimized {
            return;
        }

        egui::Panel::bottom("partition_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                let can_apply = st.out_dir.is_some() && st.spec(st.layout).is_ok();
                octa::ui::control_row::control_line(ui, |ui| {
                    if running {
                        ui.label(octa::i18n::t("partition.running").replace(
                            "{n}",
                            &(st.progress.load(Ordering::Relaxed) + 1).to_string(),
                        ));
                        if ui
                            .button(octa::i18n::t("partition.cancel_run"))
                            .on_hover_text(octa::i18n::t("partition.cancel_run_hint"))
                            .clicked()
                        {
                            st.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        if ui
                            .add_enabled(
                                can_apply,
                                egui::Button::new(octa::i18n::t("partition.apply")),
                            )
                            .on_disabled_hover_text(octa::i18n::t("partition.apply_disabled_hint"))
                            .clicked()
                        {
                            apply = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("partition.cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(!running, |ui| {
                let tab = &app.tabs[app.active_tab];
                let col_names: Vec<String> =
                    tab.table.columns.iter().map(|c| c.name.clone()).collect();

                // --- Column picker ---
                ui.label(RichText::new(octa::i18n::t("partition.column_label")).strong());
                let selected_name = st
                    .col
                    .and_then(|c| col_names.get(c).cloned())
                    .unwrap_or_else(|| octa::i18n::t("partition.column_none"));
                egui::ComboBox::from_id_salt(egui::Id::new("partition_col"))
                    .selected_text(selected_name.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut st.col,
                            None,
                            octa::i18n::t("partition.column_none"),
                        );
                        for (i, name) in col_names.iter().enumerate() {
                            ui.selectable_value(&mut st.col, Some(i), name.as_str());
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("partition.column_hint"));

                ui.add_space(8.0);

                // --- Limits ---
                octa::ui::control_row::control_grid(ui, "partition_limits", |ui| {
                    ui.label(octa::i18n::t("partition.max_rows_label"));
                    ui.add(
                        egui::TextEdit::singleline(&mut st.max_rows_buf)
                            .desired_width(120.0)
                            .hint_text("10000"),
                    )
                    .on_hover_text(octa::i18n::t("partition.max_rows_hint"));
                    ui.end_row();
                    ui.label(octa::i18n::t("partition.max_mb_label"));
                    ui.add(
                        egui::TextEdit::singleline(&mut st.max_mb_buf)
                            .desired_width(120.0)
                            .hint_text("25"),
                    )
                    .on_hover_text(octa::i18n::t("partition.max_mb_hint"));
                    ui.end_row();
                });

                ui.add_space(8.0);

                // --- Folder picker ---
                ui.label(RichText::new(octa::i18n::t("partition.folder_label")).strong());
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui
                        .button(octa::i18n::t("partition.choose_folder"))
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        st.out_dir = Some(path);
                    }
                    let dir_label = st
                        .out_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| octa::i18n::t("partition.no_folder"));
                    ui.label(dir_label);
                });

                ui.add_space(8.0);

                // --- Layout ---
                // Each option carries its own tooltip: "Flat" and "Hive" mean
                // nothing without knowing that only one of them can be reopened
                // as a table.
                ui.label(RichText::new(octa::i18n::t("partition.layout_label")).strong())
                    .on_hover_text(octa::i18n::t("partition.layout_hint"));
                // One list of shapes rather than two independent switches, and a
                // live preview underneath: the surest description of a naming
                // scheme is the names themselves, on this user's own values.
                let ext = preview_ext(&tab.table, &st.format);
                ui.add_enabled_ui(st.col.is_some(), |ui| {
                    for layout in octa::data::partition::PartitionLayout::ALL {
                        ui.radio_value(&mut st.layout, *layout, octa::i18n::t(layout.i18n_key()))
                            .on_hover_text(octa::i18n::t(layout.hint_key()))
                            .on_disabled_hover_text(octa::i18n::t(
                                "partition.layout_disabled_hint",
                            ));
                    }
                });

                ui.add_space(6.0);
                let examples = preview_lines(&tab.table, &st, &ext);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_width(360.0);
                    ui.label(
                        RichText::new(octa::i18n::t("partition.preview_label"))
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    if examples.is_empty() {
                        ui.label(
                            RichText::new(octa::i18n::t("partition.preview_empty"))
                                .size(11.0)
                                .italics()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else {
                        let root = st
                            .out_dir
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| octa::i18n::t("partition.preview_root"));
                        for line in &examples {
                            ui.label(
                                RichText::new(format!("{root}/{line}"))
                                    .monospace()
                                    .size(11.0),
                            );
                        }
                        if st.col.is_some() && examples.len() < tab.table.row_count() {
                            ui.label(
                                RichText::new(octa::i18n::t("partition.preview_more"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                });

                ui.add_space(8.0);

                // --- Format override ---
                ui.label(RichText::new(octa::i18n::t("partition.format_label")).strong());
                ui.text_edit_singleline(&mut st.format)
                    .on_hover_text(octa::i18n::t("partition.format_hint"));

                // --- Partial data ---
                if let Some((loaded, total)) = tab.partial_note() {
                    ui.add_space(6.0);
                    let text = match total {
                        Some(total) => octa::i18n::t("partition.partial_note")
                            .replace("{loaded}", &octa::ui::status_bar::format_number(loaded))
                            .replace("{total}", &octa::ui::status_bar::format_number(total)),
                        None => octa::i18n::t("partition.partial_note_unknown")
                            .replace("{loaded}", &octa::ui::status_bar::format_number(loaded)),
                    };
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.label(RichText::new(text).color(ui.visuals().warn_fg_color));
                        if ui
                            .button(octa::i18n::t("partition.load_rest"))
                            .on_hover_text(octa::i18n::t("partition.load_rest_hint"))
                            .clicked()
                        {
                            load_rest = true;
                        }
                    });
                }

                // --- Inline error ---
                if let Some(err) = &st.error {
                    ui.add_space(6.0);
                    octa::ui::message::selectable_message_sized(
                        ui,
                        ui.visuals().error_fg_color,
                        err,
                        Some(11.0),
                    );
                }
            });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if apply {
        st.error = start_split(app, &st, ctx).err();
    }
    if load_rest {
        app.open_load_all(ctx);
    }
    if close && running {
        // Keep the state so the worker's result still has a home.
        st.cancel.store(true, Ordering::Relaxed);
        st.close_when_done = true;
        close = false;
    }
    if !close {
        app.partition_dialog = Some(st);
    }
}

/// Start the split on a worker thread. Returns the localised reason when it
/// cannot start.
fn start_split(app: &mut OctaApp, st: &PartitionState, ctx: &egui::Context) -> Result<(), String> {
    let out_dir = st
        .out_dir
        .clone()
        .ok_or_else(|| octa::i18n::t("partition.need_folder"))?;
    let spec = st.spec(st.layout)?;
    let mut snap = app.tabs[app.active_tab].table.clone();
    snap.apply_edits();
    let ext = if !st.format.is_empty() {
        st.format.trim_start_matches('.').to_string()
    } else {
        snap.source_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).extension())
            .and_then(|e| e.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| octa::i18n::t("partition.need_format"))?
    };
    // Check the writer exists before spawning; the worker resolves its own.
    let check = std::path::PathBuf::from(format!("_check_.{ext}"));
    if !octa::formats::FormatRegistry::new()
        .reader_for_path(&check)
        .is_some_and(|r| r.supports_write())
    {
        return Err(format!("No writer available for extension \".{ext}\""));
    }
    let stem = snap
        .source_path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());

    st.cancel.store(false, Ordering::Relaxed);
    st.progress.store(0, Ordering::Relaxed);
    st.running.store(true, Ordering::Release);
    let (running, cancel, progress, result) = (
        Arc::clone(&st.running),
        Arc::clone(&st.cancel),
        Arc::clone(&st.progress),
        Arc::clone(&st.result),
    );
    let ctx = ctx.clone();
    let task = app.long_tasks.start(octa::i18n::t("partition.title"));
    std::thread::spawn(move || {
        let _task = task;
        let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
        let outcome = (|| {
            let registry = octa::formats::FormatRegistry::new();
            let writer = registry
                .reader_for_path(&std::path::PathBuf::from(format!("_check_.{ext}")))
                .ok_or_else(|| anyhow::anyhow!("No writer available for extension \".{ext}\""))?;
            octa::data::partition::write_split(
                &snap,
                &spec,
                &out_dir,
                &ext,
                &stem,
                octa::data::partition::SplitIo {
                    write: &mut |t, p| writer.write_file(p, t),
                    cancel: &cancel,
                    progress: &mut |n, _| {
                        progress.store(n, Ordering::Relaxed);
                        ctx.request_repaint();
                    },
                },
            )
        })()
        .map(|o| (o, out_dir))
        .map_err(|e| format!("{e:#}"));
        if let Ok(mut slot) = result.lock() {
            *slot = Some(outcome);
        }
        ctx.request_repaint();
    });
    Ok(())
}

impl OctaApp {
    /// Close the Split dialog once its worker finished, with a status line.
    pub(crate) fn drain_partition(&mut self) {
        let Some(st) = &mut self.partition_dialog else {
            return;
        };
        let done = st.result.lock().ok().and_then(|mut s| s.take());
        let Some(done) = done else {
            return;
        };
        match done {
            Ok((outcome, dir)) => {
                let key = if outcome.cancelled {
                    "partition.cancelled"
                } else {
                    "partition.done"
                };
                self.status_message = Some((
                    octa::i18n::t(key)
                        .replace("{n}", &outcome.parts.len().to_string())
                        .replace("{dir}", &dir.display().to_string()),
                    std::time::Instant::now(),
                ));
                self.partition_dialog = None;
            }
            Err(e) => {
                if st.close_when_done {
                    self.status_message = Some((e, std::time::Instant::now()));
                    self.partition_dialog = None;
                } else {
                    st.error = Some(e);
                }
            }
        }
    }
}

/// Distinct values of `col`, in row order, stopping at `want`.
///
/// ponytail: bounded scan rather than a cached result. It runs per frame to
/// build a tooltip, so it reads at most `SCAN_ROWS` rows and stops as soon as
/// it has enough - cheap enough to need no invalidation logic, and the values
/// are real ones from the table either way.
fn sample_values(table: &octa::data::DataTable, col: usize, want: usize) -> Vec<String> {
    const SCAN_ROWS: usize = 2_000;
    let mut seen: Vec<String> = Vec::with_capacity(want);
    for row in 0..table.row_count().min(SCAN_ROWS) {
        let v = table
            .get(row, col)
            .map(|c| c.to_string())
            .unwrap_or_default();
        if v.is_empty() || seen.contains(&v) {
            continue;
        }
        seen.push(v);
        if seen.len() == want {
            break;
        }
    }
    seen
}

/// What the split would name its first files, via the writer's own naming.
fn preview_lines(table: &octa::data::DataTable, st: &PartitionState, ext: &str) -> Vec<String> {
    // An incomplete form still previews: fall back to "no limit" naming.
    let spec = st
        .spec(st.layout)
        .unwrap_or_else(|_| octa::data::partition::SplitSpec {
            column: st.col,
            layout: st.layout,
            ..Default::default()
        });
    let col_name = st
        .col
        .and_then(|c| table.columns.get(c))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let values = st
        .col
        .map(|c| sample_values(table, c, 3))
        .unwrap_or_default();
    let stem = table
        .source_path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());
    octa::data::partition::preview_paths(&spec, &col_name, &values, &stem, ext)
}

/// The extension the outputs would get: the override if set, else the source
/// file's own. Mirrors `start_split`, which does the same thing before
/// writing anything.
fn preview_ext(table: &octa::data::DataTable, format_override: &str) -> String {
    if !format_override.is_empty() {
        return format_override.trim_start_matches('.').to_string();
    }
    table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "csv".to_string())
}

#[cfg(test)]
#[path = "partition_tests.rs"]
mod tests;
