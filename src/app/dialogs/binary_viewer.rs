//! The binary cell viewer: what the bytes are, what is inside them, and two
//! ways out (Save to file, Open in default app). Detection and readers are in
//! `octa::data::binary_kind`; this file only draws.

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::CellValue;
use octa::data::binary_kind::{self, BinaryKind};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, dialog_header, draw_result_message, remember_dialog_rect, size_dialog_window,
};
use octa::ui::status_bar::human_size;

use crate::app::state::{BinaryViewerState, GunzipView, OctaApp};

/// The hex dump shows this much; the rest is a click on Save away.
const HEX_MAX: usize = 64 * 1024;
/// Text shows this much: one egui label holds it, laid out every frame.
const TEXT_MAX: usize = 256 * 1024;

impl OctaApp {
    /// Open the viewer on cell (`row`, `col`) of the active tab.
    pub(crate) fn open_binary_viewer(&mut self, row: usize, col: usize) {
        let tab = &self.tabs[self.active_tab];
        let Some(CellValue::Binary(bytes)) = tab.table.get(row, col) else {
            self.status_message = Some((t("binview.not_binary"), std::time::Instant::now()));
            return;
        };
        let bytes = bytes.clone();
        let column = tab
            .table
            .columns
            .get(col)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        self.open_binary_viewer_on(bytes, column, row);
    }

    /// Open the viewer on `bytes`; `column` and `row` name it (title, file name).
    pub(crate) fn open_binary_viewer_on(&mut self, bytes: Vec<u8>, column: String, row: usize) {
        let kind = binary_kind::detect(&bytes);
        let pdf = Arc::new(Mutex::new(None));
        if kind == BinaryKind::Pdf {
            let slot = pdf.clone();
            let b = bytes.clone();
            std::thread::spawn(move || {
                let r = binary_kind::pdf_text(&b).map_err(|e| format!("{e:#}"));
                if let Ok(mut g) = slot.lock() {
                    *g = Some(r);
                }
            });
        }
        // Settings > Performance is the only limit: unlimited means unlimited.
        let cap = self.settings.max_decompressed();
        let gunzip = Arc::new(Mutex::new(None));
        if kind == BinaryKind::Gzip {
            let slot = gunzip.clone();
            let b = bytes.clone();
            std::thread::spawn(move || {
                let r = gunzip_view(&b, cap).map_err(|e| format!("{e:#}"));
                if let Ok(mut g) = slot.lock() {
                    *g = Some(r);
                }
            });
        }
        self.binary_viewer = Some(BinaryViewerState {
            size: DialogSize::Normal,
            column,
            row,
            zip: (kind == BinaryKind::Zip).then(|| {
                octa::formats::archive_reader::list_zip_entries_from(std::io::Cursor::new(
                    bytes.as_slice(),
                ))
                .map_err(|e| format!("{e:#}"))
            }),
            gunzip,
            gunzip_done: None,
            json: (kind == BinaryKind::Json)
                .then(|| octa::data::json_util::pretty_print(&String::from_utf8_lossy(&bytes))),
            bytes,
            kind,
            texture: None,
            image_size: None,
            image_error: None,
            pdf,
            pdf_done: None,
            hex: None,
            message: None,
        });
    }
}

/// Unpack `bytes` up to `cap` and keep only what the window shows, so a
/// gigabyte of output is not held while the dialog is open.
fn gunzip_view(bytes: &[u8], cap: u64) -> std::io::Result<GunzipView> {
    let (inner, cut) = binary_kind::gunzip_capped(bytes, cap)?;
    let body = match binary_kind::as_text(&inner) {
        Some(text) => Ok(text_head(text).to_owned()),
        None => Err((
            binary_kind::detect(&inner).i18n_key(),
            binary_kind::hex_dump(&inner, HEX_MAX),
        )),
    };
    Ok(GunzipView {
        size: inner.len(),
        cut,
        body,
    })
}

/// Upload a decoded picture. The GPU refuses a texture wider than its limit
/// (egui_glow panics), so a huge picture is shrunk for the screen only.
pub(crate) fn picture_texture(
    ctx: &egui::Context,
    name: &str,
    rgba: image::RgbaImage,
) -> egui::TextureHandle {
    let (w, h) = rgba.dimensions();
    let max = ctx.input(|i| i.max_texture_side) as u32;
    let (tw, th) = fit_to_side(w, h, max);
    let rgba = if (tw, th) == (w, h) {
        rgba
    } else {
        image::imageops::resize(&rgba, tw, th, image::imageops::FilterType::Triangle)
    };
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    )
}

/// `w` x `h` shrunk, keeping the aspect ratio, until neither side is over
/// `max`. Something already small enough comes back as it is.
fn fit_to_side(w: u32, h: u32, max: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max || max == 0 {
        return (w, h);
    }
    let scale = |v: u32| ((u64::from(v) * u64::from(max) / u64::from(long)) as u32).max(1);
    (scale(w), scale(h))
}

/// A file name for the cell: column, row and the kind's extension.
fn file_name(st: &BinaryViewerState) -> String {
    format!(
        "{}_row{}.{}",
        octa::sql::sanitize_sql_name(&st.column),
        st.row + 1,
        st.kind.extension()
    )
}

pub(crate) fn render_binary_viewer(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.binary_viewer.take() else {
        return;
    };
    let mut close = false;
    let mut save = false;
    let mut open = false;

    // The picture is decoded once, on the first frame.
    if st.kind.is_image() && st.texture.is_none() && st.image_error.is_none() {
        match binary_kind::decode_image(&st.bytes) {
            Ok(rgba) => {
                let (w, h) = rgba.dimensions();
                st.image_size = Some([w, h]);
                st.texture = Some(picture_texture(ctx, "octa_binary_cell", rgba));
            }
            Err(e) => st.image_error = Some(format!("{e:#}")),
        }
    }
    if st.kind == BinaryKind::Pdf && st.pdf_done.is_none() {
        st.pdf_done = st.pdf.lock().ok().and_then(|mut g| g.take());
    }
    if st.kind == BinaryKind::Gzip && st.gunzip_done.is_none() {
        st.gunzip_done = st.gunzip.lock().ok().and_then(|mut g| g.take());
    }
    let waiting = match st.kind {
        BinaryKind::Pdf => st.pdf_done.is_none(),
        BinaryKind::Gzip => st.gunzip_done.is_none(),
        _ => false,
    };
    if waiting {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    let dialog_id = egui::Id::new("octa_binary_viewer");
    let window = egui::Window::new("octa_binary_viewer_window")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, st.size, window, |w| {
        w.resizable(true)
            .default_width(640.0)
            .default_height(480.0)
            .min_width(360.0)
            .min_height(200.0)
    });
    let mut size = st.size;
    let inner = window.show(ctx, |ui| {
        close |= dialog_header(
            ui,
            "binview_header",
            RichText::new(format!("{} - {}", t("binview.title"), st.column)),
            &mut size,
        );
        if size == DialogSize::Minimized {
            return;
        }
        egui::Panel::bottom("binview_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                if let Some((ok, msg)) = &st.message {
                    draw_result_message(ui, *ok, msg);
                }
                octa::ui::control_row::control_line(ui, |ui| {
                    save = ui
                        .button(t("binview.save"))
                        .on_hover_text(t("binview.save_hint"))
                        .clicked();
                    open = ui
                        .button(t("binview.open_app"))
                        .on_hover_text(t("binview.open_app_hint"))
                        .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(
                    t("binview.holds")
                        .replace("{kind}", &t(st.kind.i18n_key()))
                        .replace("{size}", &human_size(st.bytes.len() as u64)),
                )
                .weak(),
            );
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| draw_body(ui, &mut st));
        });
    });
    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if save
        && let Some(path) = rfd::FileDialog::new()
            .set_file_name(file_name(&st))
            .save_file()
    {
        st.message = Some(match std::fs::write(&path, &st.bytes) {
            Ok(()) => (
                true,
                t("binview.saved").replace("{path}", &path.display().to_string()),
            ),
            Err(e) => (false, format!("{e:#}")),
        });
    }
    if open {
        st.message = Some(match open_in_default_app(&mut app.binary_temp_dir, &st) {
            Ok(()) => (true, t("binview.opened")),
            Err(e) => (false, e),
        });
    }
    if !close {
        app.binary_viewer = Some(st);
    }
}

/// Write a copy into Octa's temp folder (deleted on exit) and hand it to the
/// system. `open_with_system` runs `xdg-open` / `open` / `start`, which take a
/// path as readily as a URL.
fn open_in_default_app(
    dir: &mut Option<tempfile::TempDir>,
    st: &BinaryViewerState,
) -> Result<(), String> {
    if dir.is_none() {
        *dir = Some(
            tempfile::Builder::new()
                .prefix("octa-cells-")
                .tempdir()
                .map_err(|e| format!("{e:#}"))?,
        );
    }
    let path = dir
        .as_ref()
        .map(|d| d.path().join(file_name(st)))
        .ok_or_else(|| "no temporary folder".to_string())?;
    std::fs::write(&path, &st.bytes).map_err(|e| format!("{e:#}"))?;
    octa::auth::oauth_browser::open_with_system(&path.to_string_lossy())
        .map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
#[path = "binary_viewer_tests.rs"]
mod tests;

/// The contents, by kind. Anything that cannot be shown as itself falls back
/// to the hex dump, with the reason above it.
fn draw_body(ui: &mut egui::Ui, st: &mut BinaryViewerState) {
    match st.kind {
        k if k.is_image() => match (&st.texture, st.image_size) {
            (Some(texture), Some([w, h])) => {
                ui.label(
                    t("binview.pixels")
                        .replace("{w}", &w.to_string())
                        .replace("{h}", &h.to_string()),
                );
                ui.add(egui::Image::from_texture(texture).shrink_to_fit());
            }
            _ => {
                let error = st.image_error.clone().unwrap_or_default();
                ui.label(t("binview.image_unreadable").replace("{error}", &error));
                draw_hex(ui, &st.bytes, &mut st.hex);
            }
        },
        BinaryKind::Pdf => match &st.pdf_done {
            None => {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.spinner();
                    ui.label(t("binview.reading_pdf"));
                });
            }
            Some(Ok((pages, text))) => {
                ui.label(t("binview.pages").replace("{n}", &pages.to_string()));
                ui.separator();
                draw_text(ui, text);
            }
            Some(Err(e)) => {
                ui.label(e);
                draw_hex(ui, &st.bytes, &mut st.hex);
            }
        },
        BinaryKind::Zip => match &st.zip {
            Some(Ok(entries)) => {
                ui.label(t("binview.zip_files").replace("{n}", &entries.len().to_string()));
                egui::Grid::new("binview_zip")
                    .striped(true)
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.strong(t("binview.zip_name"));
                        ui.strong(t("binview.zip_size"));
                        ui.strong(t("binview.zip_date"));
                        ui.end_row();
                        for e in entries {
                            ui.label(&e.path);
                            ui.label(human_size(e.size_bytes.max(0) as u64));
                            ui.label(e.mtime.map(|d| d.to_string()).unwrap_or_default());
                            ui.end_row();
                        }
                    });
            }
            Some(Err(e)) => {
                ui.label(e);
                draw_hex(ui, &st.bytes, &mut st.hex);
            }
            None => draw_hex(ui, &st.bytes, &mut st.hex),
        },
        BinaryKind::Gzip => match &st.gunzip_done {
            None => {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.spinner();
                    ui.label(t("binview.unpacking"));
                });
            }
            Some(Ok(view)) => {
                ui.label(t("binview.unpacks_to").replace("{size}", &human_size(view.size as u64)));
                if view.cut {
                    ui.label(
                        RichText::new(
                            t("binview.unpack_capped")
                                .replace("{size}", &human_size(view.size as u64)),
                        )
                        .weak(),
                    );
                }
                ui.separator();
                match &view.body {
                    Ok(head) => {
                        if head.len() < view.size {
                            draw_cut_note(ui, head.len(), view.size);
                        }
                        draw_mono(ui, head);
                    }
                    Err((key, hex)) => {
                        ui.label(t(key));
                        if view.size > HEX_MAX {
                            draw_cut_note(ui, HEX_MAX, view.size);
                        }
                        draw_mono(ui, hex);
                    }
                }
            }
            Some(Err(e)) => {
                ui.label(e);
                draw_hex(ui, &st.bytes, &mut st.hex);
            }
        },
        BinaryKind::Json => draw_text(ui, st.json.as_deref().unwrap_or_default()),
        _ => draw_hex(ui, &st.bytes, &mut st.hex),
    }
}

/// The first `TEXT_MAX` bytes of `text`, cut on a character boundary.
fn text_head(text: &str) -> &str {
    &text[..text.floor_char_boundary(TEXT_MAX)]
}

/// Text, its first `TEXT_MAX` bytes when longer, with a note saying so.
fn draw_text(ui: &mut egui::Ui, text: &str) {
    let head = text_head(text);
    if head.len() < text.len() {
        draw_cut_note(ui, head.len(), text.len());
    }
    draw_mono(ui, head);
}

fn draw_cut_note(ui: &mut egui::Ui, shown: usize, size: usize) {
    ui.label(
        RichText::new(
            t("binview.hex_first")
                .replace("{shown}", &human_size(shown as u64))
                .replace("{size}", &human_size(size as u64)),
        )
        .weak(),
    );
}

fn draw_mono(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(RichText::new(text).monospace())
            .selectable(true)
            .extend(),
    );
}

fn draw_hex(ui: &mut egui::Ui, bytes: &[u8], cache: &mut Option<String>) {
    if bytes.len() > HEX_MAX {
        draw_cut_note(ui, HEX_MAX, bytes.len());
    }
    draw_mono(
        ui,
        cache.get_or_insert_with(|| binary_kind::hex_dump(bytes, HEX_MAX)),
    );
}
