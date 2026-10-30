//! "Export to PDF" dialog. Prints what the active tab is showing - the grid,
//! the Summary tab, the Quality report, any result tab - as a paginated PDF,
//! and a Markdown file shown as text as the rendered document. Driven by
//! `OctaApp.pdf_export_dialog`; the rendering lives in
//! `octa::data::pdf_export` and `octa::data::pdf_markdown`.

use eframe::egui;
use egui::RichText;

use octa::data::pdf_export::{PageSize, PdfOptions, PdfTable, page_count, table_to_pdf};
use octa::data::pdf_markdown::{markdown_page_count, markdown_to_pdf};
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

/// What the export is looking at: the rows and columns on screen, in display
/// order, plus the frozen count translated into that column list.
struct ViewSlice {
    rows: Vec<usize>,
    cols: Vec<usize>,
    frozen: usize,
}

impl OctaApp {
    /// The active tab's visible rows and columns.
    fn pdf_view_slice(&self) -> ViewSlice {
        let tab = &self.tabs[self.active_tab];
        let frozen_cols = tab.table_state.frozen_cols;
        let cols: Vec<usize> = (0..tab.table.col_count())
            .filter(|c| !tab.hidden_columns.contains(c))
            .collect();
        ViewSlice {
            // A frozen column that is also hidden is not on the page, so the
            // repeated band is counted in the *visible* list, not the table's.
            frozen: cols.iter().filter(|&&c| c < frozen_cols).count(),
            rows: tab.filtered_rows.clone(),
            cols,
        }
    }

    /// One line saying what the reader is looking at. English, like the rest
    /// of the document chrome and like the HTML report.
    fn pdf_subtitle(&self, slice: &ViewSlice) -> String {
        let tab = &self.tabs[self.active_tab];
        let total_rows = tab.table.row_count();
        let mut parts = Vec::new();
        if slice.rows.len() == total_rows {
            parts.push(format!("{total_rows} rows"));
        } else {
            parts.push(format!(
                "{} of {total_rows} rows (filtered)",
                slice.rows.len()
            ));
        }
        if slice.cols.len() == tab.table.col_count() {
            parts.push(format!("{} columns", slice.cols.len()));
        } else {
            parts.push(format!(
                "{} of {} columns",
                slice.cols.len(),
                tab.table.col_count()
            ));
        }
        if !tab.search_text.trim().is_empty() {
            parts.push(format!("search: {}", tab.search_text.trim()));
        }
        if !tab.column_filters.is_empty() {
            parts.push(format!("{} column filters", tab.column_filters.len()));
        }
        if !tab.predicate_filters.is_empty() {
            parts.push(format!("{} row filters", tab.predicate_filters.len()));
        }
        if tab.mark_filter_active {
            parts.push("marked rows only".to_string());
        }
        parts.join(", ")
    }

    /// The Markdown text to print as a document, when the active tab is a
    /// Markdown file shown as text. Its table is one row per source line, so
    /// printing that would put the raw syntax on paper. The Table view still
    /// prints the table: that is what the user is looking at there.
    fn pdf_markdown_doc(&self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        (tab.table.format_name.as_deref() == Some("Markdown")
            && tab.view_mode != octa::data::ViewMode::Table)
            .then(|| tab.raw_content.clone())
            .flatten()
    }

    /// Title line: the file name if there is one, else the tab's label.
    fn pdf_title(&self) -> String {
        let tab = &self.tabs[self.active_tab];
        tab.table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| tab.title_display())
    }
}

pub(crate) fn render_pdf_export_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pdf_export_dialog.is_none() {
        return;
    }
    let mut state = app.pdf_export_dialog.take().unwrap();
    let mut close = false;
    let mut export = false;

    let dialog_id = egui::Id::new("octa_pdf_export_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let slice = app.pdf_view_slice();
    let doc = app.pdf_markdown_doc();
    let title = app.pdf_title();
    let subtitle = (state.include_filter && doc.is_none()).then(|| app.pdf_subtitle(&slice));
    let opts = PdfOptions {
        page: state.page,
        landscape: state.landscape,
        title: title.clone(),
        subtitle,
    };

    // Sizing the columns samples cells and a document has to be laid out,
    // so only count when the page or what is printed changed.
    let content_key = match &doc {
        Some(text) => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            h.finish() as usize
        }
        None => slice.rows.len(),
    };
    if state.counted.map(|(p, l, r, _)| (p, l, r))
        != Some((state.page, state.landscape, content_key))
    {
        let count = match &doc {
            Some(text) => markdown_page_count(text, &opts),
            None => page_count(
                &PdfTable {
                    table: &app.tabs[app.active_tab].table,
                    rows: &slice.rows,
                    cols: &slice.cols,
                    frozen: slice.frozen,
                    rules: &app.tabs[app.active_tab].conditional_format_rules,
                },
                &opts,
            ),
        };
        state.counted = Some((state.page, state.landscape, content_key, count));
    }
    let pages = state.counted.map(|(_, _, _, n)| n).unwrap_or(0);

    let center = center_on_first_show(ctx, egui::vec2(380.0, 320.0));
    let window = egui::Window::new("octa_pdf_export")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(380.0).default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "pdf_export_header",
            RichText::new(octa::i18n::t("pdf.title")),
            &mut size,
        ) {
            close = true;
        }

        if minimized {
            return;
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t(if doc.is_some() {
                    "pdf.hint_markdown"
                } else {
                    "pdf.hint"
                }))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);

            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("pdf.page_size"));
                egui::ComboBox::from_id_salt("pdf_page_size")
                    .selected_text(state.page.label())
                    .show_ui(ui, |ui| {
                        for &p in PageSize::ALL {
                            ui.selectable_value(&mut state.page, p, p.label());
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("pdf.page_size_hint"));
            });

            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(octa::i18n::t("pdf.orientation"));
                ui.selectable_value(&mut state.landscape, false, octa::i18n::t("pdf.portrait"))
                    .on_hover_text(octa::i18n::t("pdf.orientation_hint"));
                ui.selectable_value(&mut state.landscape, true, octa::i18n::t("pdf.landscape"))
                    .on_hover_text(octa::i18n::t("pdf.orientation_hint"));
            });

            // A document has no filter or row counts to describe.
            if doc.is_none() {
                ui.checkbox(
                    &mut state.include_filter,
                    octa::i18n::t("pdf.include_filter"),
                )
                .on_hover_text(octa::i18n::t("pdf.include_filter_hint"));
            }

            ui.add_space(6.0);
            ui.label(
                RichText::new(octa::i18n::t("pdf.pages").replace("{n}", &pages.to_string()))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(8.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .button(octa::i18n::t("pdf.export"))
                    .on_hover_text(octa::i18n::t("pdf.export_hint"))
                    .clicked()
                {
                    export = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || export {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if export {
        let stem = std::path::Path::new(&title)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "table".to_string());
        let picked = rfd::FileDialog::new()
            .set_title(octa::i18n::t("pdf.title"))
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{stem}.pdf"))
            .save_file();
        if let Some(path) = picked {
            let tab = &app.tabs[app.active_tab];
            let pdf = match &doc {
                Some(text) => markdown_to_pdf(text, &opts),
                None => table_to_pdf(
                    &PdfTable {
                        table: &tab.table,
                        rows: &slice.rows,
                        cols: &slice.cols,
                        frozen: slice.frozen,
                        rules: &tab.conditional_format_rules,
                    },
                    &opts,
                ),
            };
            let result =
                pdf.and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
            let message = match result {
                Ok(()) => {
                    octa::i18n::t("pdf.written").replace("{path}", &path.display().to_string())
                }
                Err(e) => octa::i18n::t("pdf.failed").replace("{error}", &e),
            };
            app.status_message = Some((message, std::time::Instant::now()));
        }
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.pdf_export_dialog = Some(state);
    }
}
