//! The Save As questions, asked after the OS file picker: which rows (only
//! while a search or filter hides some), whether to open the new file, and for
//! a `.csv` how to write it. Set up by `OctaApp::begin_save_as`; Save hands
//! the answers to `finish_save_as`. Each section shows only when it applies,
//! so with both questions switched off in Settings a Save As to Parquet never
//! opens this at all.
//!
//! The CSV part is written for someone who has never heard of a delimiter:
//! every choice says in plain words who expects it, and a preview shows the
//! first lines exactly as they will be written.

use eframe::egui;
use egui::RichText;

use octa::i18n::t;

use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::{CsvChoice, OctaApp, SaveAsPrompt};

/// Rows shown in the CSV preview, below the header.
const PREVIEW_ROWS: usize = 3;

/// The separators on offer: the byte, its label, and the plain-words note
/// shown beside it (and as its tooltip).
const SEPARATORS: &[(u8, &str, &str)] = &[
    (
        b',',
        "dialog.saveas_sep_comma",
        "dialog.saveas_sep_comma_hint",
    ),
    (
        b';',
        "dialog.saveas_sep_semicolon",
        "dialog.saveas_sep_semicolon_hint",
    ),
    (b'\t', "dialog.saveas_sep_tab", "dialog.saveas_sep_tab_hint"),
    (
        b'|',
        "dialog.saveas_sep_pipe",
        "dialog.saveas_sep_pipe_hint",
    ),
];

pub(crate) fn render_save_as_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut prompt) = app.pending_save_as.take() else {
        return;
    };
    // The tab was closed while the question was open: nothing left to save.
    if prompt.tab_idx >= app.tabs.len() {
        return;
    }
    let mut save = false;
    let mut cancel = false;
    let preview = prompt.csv.map(|choice| preview_text(app, &prompt, choice));

    // The standard dialog chrome. The prompt itself is rebuilt for every
    // Save As, so the size mode lives in egui's memory like the other
    // save-time prompts; the title bar's close button means Cancel.
    let dialog_id = egui::Id::new("octa_save_as_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let file_name = prompt
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let default = if prompt.csv.is_some() {
        egui::vec2(520.0, 600.0)
    } else {
        egui::vec2(440.0, 260.0)
    };
    let center = center_on_first_show(ctx, default);
    let window = egui::Window::new("octa_save_as")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(default.x)
            .default_height(default.y)
            .min_width(360.0)
            .min_height(200.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("save_as_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(RichText::new(t("dialog.saveas_title")).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            cancel = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }

        egui::Panel::bottom("save_as_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui
                        .button(t("common.save"))
                        .on_hover_text(t("dialog.saveas_save_hint"))
                        .clicked()
                    {
                        save = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // The file the answers are for; a long path is cut, the
                    // whole of it is on hover.
                    ui.add(
                        egui::Label::new(RichText::new(&file_name).monospace().weak()).truncate(),
                    )
                    .on_hover_text(prompt.path.display().to_string());
                    ui.add_space(4.0);
                    let mut first = true;
                    let mut section = |ui: &mut egui::Ui| {
                        if !first {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);
                        }
                        first = false;
                    };
                    if prompt.ask_rows {
                        section(ui);
                        rows_section(ui, &mut prompt);
                    }
                    if let Some(choice) = prompt.csv.as_mut() {
                        section(ui);
                        csv_section(ui, choice, preview.as_deref());
                    }
                    if prompt.ask_open {
                        section(ui);
                        ui.checkbox(&mut prompt.open, t("dialog.saveas_open"))
                            .on_hover_text(t("dialog.saveas_open_hint"));
                    }
                    if prompt.ask_rows || prompt.ask_open {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(t("dialog.saveas_settings_note"))
                                .small()
                                .weak(),
                        );
                    }
                });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    // A closed dialog opens at its normal size next time.
    let keep = if save || cancel {
        DialogSize::Normal
    } else {
        size
    };
    ctx.data_mut(|d| d.insert_temp(size_key, keep));

    if save {
        let filtered = prompt.filtered && prompt.visible < prompt.total;
        app.finish_save_as(
            prompt.tab_idx,
            prompt.path,
            filtered,
            prompt.open,
            prompt.csv,
        );
    } else if !cancel {
        app.pending_save_as = Some(prompt);
    }
}

fn rows_section(ui: &mut egui::Ui, prompt: &mut SaveAsPrompt) {
    ui.label(RichText::new(t("dialog.saveas_rows")).strong());
    ui.radio_value(
        &mut prompt.filtered,
        false,
        t("dialog.saveas_rows_all").replace("{n}", &prompt.total.to_string()),
    )
    .on_hover_text(t("dialog.saveas_rows_all_hint"));
    ui.radio_value(
        &mut prompt.filtered,
        true,
        t("dialog.saveas_rows_visible").replace("{n}", &prompt.visible.to_string()),
    )
    .on_hover_text(t("dialog.saveas_rows_visible_hint"));
}

fn csv_section(ui: &mut egui::Ui, choice: &mut CsvChoice, preview: Option<&str>) {
    ui.label(RichText::new(t("dialog.saveas_sep")).strong());
    ui.label(RichText::new(t("dialog.saveas_sep_intro")).small().weak());
    ui.add_space(2.0);
    // A grid, so the plain-words notes start in one column whatever the
    // length of the separator names in this language.
    octa::ui::control_row::control_grid(ui, "save_as_separators", |ui| {
        for &(delimiter, label, note) in SEPARATORS {
            ui.radio_value(&mut choice.delimiter, delimiter, t(label))
                .on_hover_text(t(note));
            ui.label(RichText::new(t(note)).small().weak());
            ui.end_row();
        }
    });

    ui.add_space(8.0);
    ui.label(RichText::new(t("dialog.saveas_decimal")).strong());
    octa::ui::control_row::control_line(ui, |ui| {
        ui.radio_value(
            &mut choice.decimal_comma,
            false,
            t("dialog.saveas_decimal_point"),
        )
        .on_hover_text(t("dialog.saveas_decimal_hint"));
        ui.radio_value(
            &mut choice.decimal_comma,
            true,
            t("dialog.saveas_decimal_comma"),
        )
        .on_hover_text(t("dialog.saveas_decimal_hint"));
    });
    if choice.decimal_comma && choice.delimiter == b',' {
        ui.label(
            RichText::new(t("dialog.saveas_quoted_note"))
                .small()
                .color(ui.visuals().warn_fg_color),
        );
    }

    ui.add_space(4.0);
    ui.checkbox(&mut choice.bom, t("dialog.saveas_bom"))
        .on_hover_text(t("dialog.saveas_bom_hint"));

    if let Some(text) = preview {
        ui.add_space(8.0);
        ui.label(RichText::new(t("dialog.saveas_preview")).strong())
            .on_hover_text(t("dialog.saveas_preview_hint"));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).monospace())
                    .wrap_mode(egui::TextWrapMode::Truncate),
            )
            .on_hover_text(t("dialog.saveas_preview_hint"));
        });
    }
    ui.add_space(4.0);
    ui.label(RichText::new(t("dialog.saveas_tip")).small().weak());
}

/// The first lines of the file as it will be written: the rows the save will
/// actually write (only the visible ones when that is the choice), with the
/// Settings quoting and header switches and the dialog's picks.
fn preview_text(app: &OctaApp, prompt: &SaveAsPrompt, choice: CsvChoice) -> String {
    let tab = &app.tabs[prompt.tab_idx];
    let rows: Vec<usize> = if prompt.filtered && prompt.visible < prompt.total {
        tab.filtered_rows
            .iter()
            .take(PREVIEW_ROWS)
            .copied()
            .collect()
    } else {
        (0..prompt.total.min(PREVIEW_ROWS)).collect()
    };
    let head = tab.table.clone_with_rows(&rows);
    let opts = octa::formats::write_options::CsvOptions {
        delimiter: choice.delimiter,
        decimal_comma: choice.decimal_comma,
        bom: choice.bom,
        ..app.settings.write_options.csv.clone()
    };
    // A tab character has no glyph in the UI font; show it as a gap.
    octa::formats::csv_reader::preview_delimited(&head, &opts, PREVIEW_ROWS).replace('\t', "    ")
}
