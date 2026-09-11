//! The shared "Write options" expander.
//!
//! Lives in the library rather than beside the other dialog widgets because
//! both surfaces need it: the Settings dialog (library) edits the defaults and
//! the batch convert dialog (binary) edits a per-run copy.

use eframe::egui;

/// The "Write options" expander shared by the batch convert dialog and Save As.
///
/// Every control carries a hover hint: compression codecs and row-group sizes
/// are exactly the settings nobody can guess from the label alone. The values
/// apply to the current operation; the Settings page holds the defaults these
/// are seeded from.
///
/// Three expanders inside one: Parquet, CSV/TSV, Excel. Every control here
/// belongs to exactly one format, so a flat list made a user hunt past six
/// irrelevant controls for the one their file actually uses.
///
/// `row_group_buf` is the text buffer behind the row-group size, kept by the
/// caller so a half-typed number survives a frame. Empty means "writer default".
pub fn render_write_options(
    ui: &mut egui::Ui,
    opts: &mut crate::formats::write_options::WriteOptions,
    row_group_buf: &mut String,
) {
    use crate::formats::write_options::{PARQUET_CODECS, QuoteStyle};

    // The hover lives on the group header, which is the only place the write
    // options are. A second, control-less "Write options" row used to sit in
    // the File-Specific section carrying this hint and nothing else, so it
    // read as a setting that did nothing.
    let group = egui::CollapsingHeader::new(crate::i18n::t("wo.title"))
        .id_salt("write_options")
        .show(ui, |ui| {
            // One expander per format rather than one flat list of every
            // control: a user saving a CSV has no business scrolling past
            // row-group sizes to reach the delimiter. Closed by default, so
            // the group opens on the format at hand and nothing else.
            egui::CollapsingHeader::new(crate::i18n::t("wo.parquet"))
                .id_salt("write_options_parquet_group")
                .show(ui, |ui| {
                    egui::Grid::new("write_options_parquet")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label(crate::i18n::t("wo.compression"))
                                .on_hover_text(crate::i18n::t("wo.compression_hint"));
                            egui::ComboBox::from_id_salt("wo_compression")
                                .selected_text(opts.parquet.compression.clone())
                                .show_ui(ui, |ui| {
                                    for codec in PARQUET_CODECS {
                                        ui.selectable_value(
                                            &mut opts.parquet.compression,
                                            (*codec).to_string(),
                                            *codec,
                                        );
                                    }
                                })
                                .response
                                .on_hover_text(crate::i18n::t("wo.compression_hint"));
                            ui.end_row();

                            ui.label(crate::i18n::t("wo.row_group_size"))
                                .on_hover_text(crate::i18n::t("wo.row_group_size_hint"));
                            if ui
                                .add(
                                    egui::TextEdit::singleline(row_group_buf)
                                        .desired_width(90.0)
                                        .hint_text(crate::i18n::t("wo.default")),
                                )
                                .on_hover_text(crate::i18n::t("wo.row_group_size_hint"))
                                .changed()
                            {
                                // Empty (or unparseable) means "leave it to the
                                // writer", which is what `None` says downstream.
                                opts.parquet.row_group_size = row_group_buf
                                    .trim()
                                    .replace([',', '_', '.'], "")
                                    .parse::<usize>()
                                    .ok()
                                    .filter(|n| *n > 0);
                            }
                            ui.end_row();
                        });

                    ui.checkbox(
                        &mut opts.parquet.dictionary,
                        crate::i18n::t("wo.dictionary"),
                    )
                    .on_hover_text(crate::i18n::t("wo.dictionary_hint"));
                    ui.checkbox(
                        &mut opts.parquet.statistics,
                        crate::i18n::t("wo.statistics"),
                    )
                    .on_hover_text(crate::i18n::t("wo.statistics_hint"));
                });

            egui::CollapsingHeader::new(crate::i18n::t("wo.csv"))
                .id_salt("write_options_csv_group")
                .show(ui, |ui| {
                    egui::Grid::new("write_options_csv")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label(crate::i18n::t("wo.delimiter"))
                                .on_hover_text(crate::i18n::t("wo.delimiter_hint"));
                            let mut delim = (opts.csv.delimiter as char).to_string();
                            if ui
                                .add(egui::TextEdit::singleline(&mut delim).desired_width(30.0))
                                .on_hover_text(crate::i18n::t("wo.delimiter_hint"))
                                .changed()
                                && let Some(c) = delim.chars().next()
                                && c.is_ascii()
                            {
                                opts.csv.delimiter = c as u8;
                            }
                            ui.end_row();

                            ui.label(crate::i18n::t("wo.quote_style"))
                                .on_hover_text(crate::i18n::t("wo.quote_style_hint"));
                            egui::ComboBox::from_id_salt("wo_quote_style")
                                .selected_text(crate::i18n::t(opts.csv.quote_style.i18n_key()))
                                .show_ui(ui, |ui| {
                                    for style in QuoteStyle::ALL {
                                        ui.selectable_value(
                                            &mut opts.csv.quote_style,
                                            *style,
                                            crate::i18n::t(style.i18n_key()),
                                        );
                                    }
                                })
                                .response
                                .on_hover_text(crate::i18n::t("wo.quote_style_hint"));
                            ui.end_row();
                        });

                    ui.checkbox(&mut opts.csv.crlf, crate::i18n::t("wo.crlf"))
                        .on_hover_text(crate::i18n::t("wo.crlf_hint"));
                    ui.checkbox(&mut opts.csv.write_header, crate::i18n::t("wo.header"))
                        .on_hover_text(crate::i18n::t("wo.header_hint"));
                });

            egui::CollapsingHeader::new(crate::i18n::t("wo.xlsx"))
                .id_salt("write_options_xlsx_group")
                .show(ui, |ui| {
                    ui.checkbox(
                        &mut opts.xlsx.include_formatting,
                        crate::i18n::t("wo.xlsx_include_formatting"),
                    )
                    .on_hover_text(crate::i18n::t("wo.xlsx_include_formatting_hint"));
                    ui.checkbox(
                        &mut opts.xlsx.preserve_formulas,
                        crate::i18n::t("wo.xlsx_preserve_formulas"),
                    )
                    .on_hover_text(crate::i18n::t("wo.xlsx_preserve_formulas_hint"));
                    ui.checkbox(
                        &mut opts.xlsx.document_properties,
                        crate::i18n::t("wo.xlsx_doc_properties"),
                    )
                    .on_hover_text(crate::i18n::t("wo.xlsx_doc_properties_hint"));
                    ui.checkbox(&mut opts.xlsx.as_table, crate::i18n::t("wo.xlsx_as_table"))
                        .on_hover_text(crate::i18n::t("wo.xlsx_as_table_hint"));
                });
        });
    group
        .header_response
        .on_hover_text(crate::i18n::t("settings_hint.write_options"));
}
