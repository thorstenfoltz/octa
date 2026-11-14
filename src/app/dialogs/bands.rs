//! "Number bands" dialog (Columns menu): numbers into labelled bands, or
//! labelled bands back into numbers, as a new column right after the source.
//! The work is `octa::data::bands`, shared with `--bands` and the MCP tool
//! `number_bands`. One undo step. A database tab bands the loaded rows.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::bands::{self, Back, BandBy, BandSpec};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;
use super::widgets::col_combo;

/// The three ways to cut, in the order the combo lists them.
const BY_KEYS: [(&str, &str); 3] = [
    ("bands.by_width", "bands.by_width_hint"),
    ("bands.by_count", "bands.by_count_hint"),
    ("bands.by_edges", "bands.by_edges_hint"),
];

pub(crate) struct BandsState {
    pub(crate) col: Option<usize>,
    /// Bands back to numbers instead of numbers to bands.
    pub(crate) back: bool,
    /// Index into [`BY_KEYS`].
    pub(crate) by: usize,
    pub(crate) count_buf: String,
    pub(crate) edges_buf: String,
    pub(crate) give: Back,
    pub(crate) error: Option<String>,
    size: DialogSize,
}

impl Default for BandsState {
    fn default() -> Self {
        Self {
            col: None,
            back: false,
            by: 0,
            count_buf: "5".into(),
            edges_buf: String::new(),
            give: Back::Middle,
            error: None,
            size: DialogSize::default(),
        }
    }
}

impl BandsState {
    /// What the form asks for, or why it cannot be run yet.
    fn spec(&self) -> Result<BandSpec, String> {
        if self.back {
            return Ok(BandSpec::Back(self.give));
        }
        let count = || {
            self.count_buf
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=bands::MAX_BANDS).contains(n))
                .ok_or_else(|| t("bands.bad_count"))
        };
        Ok(BandSpec::ToBands(match self.by {
            0 => BandBy::Width(count()?),
            1 => BandBy::Count(count()?),
            _ => {
                let edges = bands::parse_edges(&self.edges_buf)
                    .map_err(|e| t("bands.bad_edge").replace("{text}", &e))?;
                if edges.is_empty() {
                    return Err(t("bands.no_edges"));
                }
                BandBy::Edges(edges)
            }
        }))
    }
}

impl OctaApp {
    /// Columns > Number bands... and the shortcut. Starts on the selected
    /// column.
    pub(crate) fn open_bands(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.table.col_count() == 0 || self.is_readonly() {
            return;
        }
        let col = tab.table_state.selected_cell.map(|(_, c)| c);
        self.bands_dialog = Some(BandsState {
            col,
            ..BandsState::default()
        });
    }
}

pub(crate) fn render_bands_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.bands_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let col_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    st.col = st.col.filter(|&c| c < col_names.len());
    let spec = st.spec();
    let why_not: Option<String> = if st.col.is_none() {
        Some(t("bands.apply_why"))
    } else {
        spec.as_ref().err().cloned()
    };

    let mut close = false;
    let mut apply = false;
    let dialog_id = egui::Id::new("octa_bands_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 320.0));
    let window = egui::Window::new("octa_bands")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(320.0)
            .min_width(340.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "bands_header",
            RichText::new(t("bands.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("bands_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b = ui.add_enabled(why_not.is_none(), egui::Button::new(t("bands.apply")));
                    if b.clicked() {
                        apply = true;
                    }
                    match &why_not {
                        None => {
                            b.on_hover_text(t("bands.apply_hint"));
                        }
                        Some(w) => {
                            b.on_disabled_hover_text(disabled_tip(&t("bands.apply_hint"), w));
                        }
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("bands.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            control_row(ui, |ui| {
                ui.radio_value(&mut st.back, false, t("bands.to_bands"))
                    .on_hover_text(t("bands.to_bands_hint"));
                ui.radio_value(&mut st.back, true, t("bands.to_numbers"))
                    .on_hover_text(t("bands.to_numbers_hint"));
            });
            ui.add_space(6.0);
            control_grid(ui, "bands_grid", |ui| {
                ui.label(t("bands.column"))
                    .on_hover_text(t("bands.column_hint"));
                col_combo(ui, "bands_col", &mut st.col, &col_names)
                    .on_hover_text(t("bands.column_hint"));
                ui.end_row();

                if st.back {
                    ui.label(t("bands.give"))
                        .on_hover_text(t("bands.give_hint"));
                    egui::ComboBox::from_id_salt("bands_give")
                        .selected_text(t(st.give.label_key()))
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            for b in Back::ALL {
                                ui.selectable_value(&mut st.give, b, t(b.label_key()));
                            }
                        })
                        .response
                        .on_hover_text(t("bands.give_hint"));
                    ui.end_row();
                } else {
                    ui.label(t("bands.by")).on_hover_text(t("bands.by_hint"));
                    egui::ComboBox::from_id_salt("bands_by")
                        .selected_text(t(BY_KEYS[st.by].0))
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            for (i, (label, hint)) in BY_KEYS.iter().enumerate() {
                                ui.selectable_value(&mut st.by, i, t(label))
                                    .on_hover_text(t(hint));
                            }
                        })
                        .response
                        .on_hover_text(t(BY_KEYS[st.by].1));
                    ui.end_row();

                    if st.by == 2 {
                        ui.label(t("bands.edges"))
                            .on_hover_text(t("bands.edges_hint"));
                        control_text_edit(
                            ui,
                            200.0,
                            egui::TextEdit::singleline(&mut st.edges_buf).hint_text("18, 35, 65"),
                        )
                        .on_hover_text(t("bands.edges_hint"));
                    } else {
                        ui.label(t("bands.count"))
                            .on_hover_text(t("bands.count_hint"));
                        control_text_edit(ui, 80.0, egui::TextEdit::singleline(&mut st.count_buf))
                            .on_hover_text(t("bands.count_hint"));
                    }
                    ui.end_row();
                }
            });
            if st.col.is_some()
                && let Some(why) = &why_not
            {
                octa::ui::message::selectable_message(ui, ui.visuals().warn_fg_color, why);
            }
            if let Some(e) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if apply
        && !app.is_readonly()
        && let (Some(col), Ok(spec)) = (st.col, spec)
    {
        let tab = &mut app.tabs[app.active_tab];
        let start = tab.table.undo_stack.len();
        match bands::add_band_column(&mut tab.table, col, &spec) {
            Ok((name, unread)) => {
                tab.table.coalesce_undo_since(start);
                tab.filter_dirty = true;
                tab.table_state.widths_initialized = false;
                let mut msg = t("bands.done").replace("{name}", &name);
                if unread > 0 {
                    msg = format!(
                        "{msg} {}",
                        t("bands.unread").replace("{n}", &unread.to_string())
                    );
                }
                if tab.source_has_more() {
                    msg = format!("{msg} {}", t("cohort.loaded_note"));
                }
                app.status_message = Some((msg, Instant::now()));
                return;
            }
            Err(e) => st.error = Some(t(e.key())),
        }
    }
    if !close {
        app.bands_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "bands_tests.rs"]
mod tests;
