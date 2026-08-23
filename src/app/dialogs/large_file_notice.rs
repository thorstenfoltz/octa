//! The forced choice before a very large file opens: read it from disk with
//! everything editable turned off, or load it the usual way and see the first
//! N rows.
//!
//! Both answers are reasonable, which is why this asks rather than deciding.
//! It also states what large-file mode cannot do **before** the file opens,
//! because discovering that editing is refused after a two-minute load is the
//! worst version of the same information.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use egui::RichText;

use octa::i18n::t;
use octa::ui::status_bar::human_size;

use super::super::state::OctaApp;

/// The pending question about one file.
pub(crate) struct LargeFileNotice {
    pub(crate) path: PathBuf,
    pub(crate) size_bytes: u64,
    /// This format cannot be scanned in place, so opening it in large-file
    /// mode means converting it to a temporary Parquet file first.
    pub(crate) needs_conversion: bool,
    pub(crate) suppress_future: bool,
}

/// A conversion running on a worker thread, with its cancel flag.
pub(crate) struct LargeConvertJob {
    pub(crate) name: String,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) slot: Arc<std::sync::Mutex<Option<Result<PathBuf, String>>>>,
}

pub(crate) fn render_large_file_notice(app: &mut OctaApp, ctx: &egui::Context) {
    render_convert_progress(app, ctx);

    let Some(notice) = app.pending_large_file_notice.as_ref() else {
        return;
    };
    let path = notice.path.clone();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let needs_conversion = notice.needs_conversion;
    // Round-tripped through the notice rather than re-derived from settings
    // each frame: re-deriving overwrites the click in the frame it happens,
    // and the checkbox flickers.
    let mut suppress_future = notice.suppress_future;
    let mut choice: Option<bool> = None; // Some(true) = large mode
    let mut close = false;

    egui::Window::new(t("largefile.title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(460.0);
            ui.label(
                t("largefile.body")
                    .replace("{name}", &name)
                    .replace("{size}", &human_size(notice.size_bytes))
                    .replace(
                        "{cap}",
                        &octa::ui::status_bar::format_number(octa::formats::initial_load_rows()),
                    ),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(t("largefile.works")).size(11.0));
            ui.label(
                RichText::new(t("largefile.missing"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(if needs_conversion {
                    t("largefile.convert")
                } else {
                    t("largefile.direct")
                })
                .size(11.0),
            );
            ui.add_space(8.0);
            ui.checkbox(&mut suppress_future, t("largefile.suppress"))
                .on_hover_text(t("largefile.suppress_hint"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(t("largefile.open_large"))
                    .on_hover_text(t("largefile.open_large_hint"))
                    .clicked()
                {
                    choice = Some(true);
                }
                if ui
                    .button(t("largefile.open_normal"))
                    .on_hover_text(t("largefile.open_normal_hint"))
                    .clicked()
                {
                    choice = Some(false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if let Some(n) = app.pending_large_file_notice.as_mut() {
        n.suppress_future = suppress_future;
    }

    let Some(large) = choice else {
        if close {
            app.pending_large_file_notice = None;
        }
        return;
    };
    app.pending_large_file_notice = None;
    if suppress_future {
        app.settings.show_large_file_notice = false;
        app.settings.save();
    }
    if large {
        app.open_large_file(path);
    } else {
        // Bypass the size check for exactly this path, or load_file would ask
        // the same question again.
        app.large_check_bypass = Some(path.clone());
        app.load_file(path);
    }
}

/// The conversion's own little window: a spinner and a Cancel, drained here so
/// the finished path opens without the caller polling.
fn render_convert_progress(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(job) = app.large_convert_job.as_ref() else {
        return;
    };
    let name = job.name.clone();
    let mut cancel = false;

    if let Some(res) = job.slot.lock().ok().and_then(|mut g| g.take()) {
        app.large_convert_job = None;
        match res {
            Ok(tmp) => app.finish_large_open(tmp, Some(name)),
            Err(e) => app.status_message = Some((e, std::time::Instant::now())),
        }
        return;
    }

    egui::Window::new(t("largefile.title"))
        .id(egui::Id::new("octa_large_convert"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t("largefile.converting").replace("{name}", &name));
            });
            ui.add_space(8.0);
            if ui.button(t("common.cancel")).clicked() {
                cancel = true;
            }
        });

    if cancel && let Some(job) = app.large_convert_job.as_ref() {
        job.cancel.store(true, Ordering::Relaxed);
    }
}
