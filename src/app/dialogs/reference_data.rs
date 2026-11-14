//! Data -> Reference data... and the Online data error window.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::reference::currency::{CurrencySpec, Mark, Rates};
use octa::data::reference::inflation::{Indices, InflationSpec};
use octa::data::reference::names;
use octa::data::reference::places::{self, KeyKind, Output, PlacesSpec};
use octa::data::reference::{Computed, Pick, Rounding, Several, insert_columns};
use octa::i18n::t;
use octa::reference::{Dataset, ReferenceSettings, Request, lookup};
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;
use super::widgets::col_combo;

/// Failed downloads, selectable and copyable, until dismissed.
pub(crate) fn render_reference_errors(app: &mut OctaApp, ctx: &egui::Context) {
    if app.reference_errors.is_empty() {
        return;
    }
    let dialog_id = egui::Id::new("octa_reference_errors");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let (mut close, mut settings) = (false, false);
    let center = center_on_first_show(ctx, egui::vec2(480.0, 220.0));
    let window = egui::Window::new("octa_reference_errors")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(220.0)
            .min_width(320.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "reference_errors_header",
            RichText::new(t("online.errors_title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }
        egui::Panel::bottom("reference_errors_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    if ui
                        .button(t("online.open_settings"))
                        .on_hover_text(t("online.open_settings_hint"))
                        .clicked()
                    {
                        settings = true;
                    }
                    if ui
                        .button(t("common.close"))
                        .on_hover_text(t("online.close_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("online.errors_intro"));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in &app.reference_errors {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| d.insert_temp(size_key, size));
    if settings {
        app.settings_dialog.open(&app.settings);
        app.settings_dialog.focus_online_section = true;
        close = true;
    }
    if close {
        app.reference_errors.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Currency,
    Inflation,
    Places,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RoundChoice {
    #[default]
    Usual,
    To,
    None,
}

type Slot = Arc<Mutex<Option<Result<(Computed, Vec<String>), String>>>>;

pub(crate) struct ReferenceState {
    pub(crate) mode: Mode,
    pub(crate) amount: Option<usize>,
    pub(crate) from_col: bool,
    pub(crate) from: Option<usize>,
    pub(crate) from_code: String,
    pub(crate) to_code: String,
    pub(crate) date: Option<usize>,
    /// In today's money: the date comes from a column, or is one month.
    pub(crate) date_col: bool,
    pub(crate) date_year: i32,
    pub(crate) date_month: u32,
    pub(crate) latest: bool,
    pub(crate) country_col: bool,
    pub(crate) country: Option<usize>,
    pub(crate) country_code: String,
    /// `None`: the newest month every country has.
    pub(crate) target: Option<chrono::NaiveDate>,
    pub(crate) key: Option<usize>,
    pub(crate) kind: KeyKind,
    pub(crate) outputs: Vec<Output>,
    pub(crate) several: Several,
    pub(crate) round: RoundChoice,
    pub(crate) decimals_buf: String,
    pub(crate) mark: Mark,
    /// `(code, name with code)`, sorted by name.
    currencies: Vec<(String, String)>,
    areas: Vec<(String, String)>,
    place_countries: Vec<(String, String)>,
    /// German municipality codes are stored, so Kreisschlüssel can be
    /// offered. Without them the entries are left out (the user, 2026-11-14).
    kreis_stored: bool,
    months: Vec<chrono::NaiveDate>,
    running: Option<(Slot, usize, usize)>,
    pub(crate) error: Option<String>,
    size: DialogSize,
}

impl Default for ReferenceState {
    fn default() -> Self {
        Self {
            mode: Mode::Currency,
            amount: None,
            from_col: true,
            from: None,
            from_code: "EUR".into(),
            to_code: "EUR".into(),
            date: None,
            date_col: true,
            date_year: chrono::Datelike::year(&chrono::Local::now()),
            date_month: 1,
            latest: false,
            country_col: true,
            country: None,
            country_code: "DE".into(),
            target: None,
            key: None,
            kind: KeyKind::Postcode,
            outputs: Output::ALL.to_vec(),
            several: Several::All,
            round: RoundChoice::Usual,
            decimals_buf: "2".into(),
            mark: Mark::Code,
            currencies: Vec::new(),
            areas: Vec::new(),
            place_countries: Vec::new(),
            kreis_stored: false,
            months: Vec::new(),
            running: None,
            error: None,
            size: DialogSize::default(),
        }
    }
}

/// Why a mode cannot run, or `None`.
pub(crate) fn mode_why(mode: Mode, r: &ReferenceSettings) -> Option<String> {
    let off = |d: Dataset| {
        (!r.get(d).enabled).then(|| t("greyed.refdata_off").replace("{name}", &t(&d.label_key())))
    };
    match mode {
        Mode::Currency => off(Dataset::ExchangeRates),
        Mode::Inflation => off(Dataset::Inflation),
        Mode::Places => KeyKind::ALL
            .iter()
            .all(|k| k.sources().iter().all(|d| !r.get(*d).enabled))
            .then(|| t("greyed.refdata_places_off")),
    }
}

impl ReferenceState {
    pub(crate) fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if mode == Mode::Inflation && self.round == RoundChoice::Usual {
            self.round = RoundChoice::To;
        }
    }

    pub(crate) fn rounding(&self) -> Result<Rounding, String> {
        match self.round {
            RoundChoice::Usual if self.mode == Mode::Currency => Ok(Rounding::Usual),
            RoundChoice::Usual => Ok(Rounding::Decimals(2)),
            RoundChoice::None => Ok(Rounding::None),
            RoundChoice::To => self
                .decimals_buf
                .trim()
                .parse::<u32>()
                .map(Rounding::Decimals)
                .map_err(|_| t("refdata.bad_decimals")),
        }
    }

    /// Reload the code and month lists from what is stored.
    fn reload_lists(&mut self) {
        let Ok(dir) = octa::reference::store::dir() else {
            return;
        };
        self.kreis_stored = [
            octa::reference::sources::openplz::FILE,
            octa::reference::sources::eurostat::MUNICIPALITIES,
        ]
        .iter()
        .any(|f| dir.join(f).exists());
        if !self.kreis_stored && self.kind == KeyKind::Kreis {
            self.kind = KeyKind::Postcode;
        }
        let codes = Rates::load(&dir).map(|r| r.codes()).unwrap_or_default();
        self.currencies = names::labelled(&codes, names::currency_label);
        if let Ok(idx) = Indices::load(&dir) {
            self.areas = names::labelled(&idx.areas(), names::country_label);
            self.months = idx.months();
        }
        let all: Vec<String> = octa::reference::sources::geonames::COUNTRIES
            .iter()
            .map(|c| c.to_string())
            .collect();
        self.place_countries = names::labelled(&all, names::country_label);
    }

    /// Whether the dialog lists this output at all.
    pub(crate) fn offers(&self, o: Output) -> bool {
        self.kreis_stored || !matches!(o, Output::Kreis | Output::KreisName)
    }

    /// The years the fixed date offers, newest first: those the index has,
    /// from 1950 while nothing is downloaded yet.
    fn years(&self) -> Vec<i32> {
        use chrono::Datelike;
        let now = chrono::Local::now().year();
        let first = self.months.last().map_or(1950, |m| m.year());
        (first..=now).rev().collect()
    }

    /// The date `In today's money` reads: a column, or `YYYY-MM`.
    pub(crate) fn inflation_date(&self) -> Option<Pick> {
        if self.date_col {
            self.date.map(Pick::Column)
        } else {
            Some(Pick::Fixed(format!(
                "{}-{:02}",
                self.date_year, self.date_month
            )))
        }
    }
}

fn run_job(
    table: DataTable,
    req: Request,
    r: ReferenceSettings,
) -> Result<(Computed, Vec<String>), String> {
    lookup(&table, &req, &r).map_err(|e| format!("{e:#}"))
}

impl OctaApp {
    /// Data > Reference data... and the shortcut. Starts on the selected column.
    pub(crate) fn open_reference_data(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.table.col_count() == 0 || self.is_readonly() {
            return;
        }
        let col = tab.table_state.selected_cell.map(|(_, c)| c);
        let mut st = ReferenceState {
            amount: col,
            key: col,
            ..ReferenceState::default()
        };
        if let Some(first) = [Mode::Currency, Mode::Inflation, Mode::Places]
            .into_iter()
            .find(|m| mode_why(*m, &self.settings.reference).is_none())
        {
            st.set_mode(first);
        }
        st.reload_lists();
        self.reference_dialog = Some(st);
    }
}

/// A column or one fixed value, as two radios and the matching control.
/// `(use the column?, the column, the fixed value)`.
type Either<'a> = (&'a mut bool, &'a mut Option<usize>, &'a mut String);

/// A picker of codes that shows what each one is called.
fn code_combo(
    ui: &mut egui::Ui,
    id: &str,
    sel: &mut String,
    choices: &[(String, String)],
    hint: &str,
) {
    let shown = choices
        .iter()
        .find(|(c, _)| c == sel)
        .map_or_else(|| sel.clone(), |(_, l)| l.clone());
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(240.0)
        .show_ui(ui, |ui| {
            for (c, l) in choices {
                ui.selectable_value(sel, c.clone(), l);
            }
        })
        .response
        .on_hover_text(t(hint));
}

fn column_or_fixed(
    ui: &mut egui::Ui,
    id: &str,
    (use_col, col, fixed): Either,
    choices: &[(String, String)],
    names: &[String],
    hint: &str,
) {
    control_row(ui, |ui| {
        ui.radio_value(use_col, true, t("refdata.use_column"))
            .on_hover_text(t("refdata.use_column_hint"));
        ui.radio_value(use_col, false, t("refdata.use_fixed"))
            .on_hover_text(t("refdata.use_fixed_hint"));
        if *use_col {
            col_combo(ui, &format!("{id}_col"), col, names).on_hover_text(t(hint));
        } else {
            code_combo(ui, &format!("{id}_fixed"), fixed, choices, hint);
        }
    });
}

pub(crate) fn render_reference_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.reference_dialog.take() else {
        return;
    };
    // A finished job lands here first.
    let landed = st.running.as_ref().and_then(|(slot, undo_len, rows)| {
        let result = slot.lock().unwrap_or_else(|p| p.into_inner()).take()?;
        Some((result, *undo_len, *rows))
    });
    if let Some((result, undo_len, rows)) = landed {
        st.running = None;
        match result {
            Ok((computed, warnings)) => {
                app.reference_errors.extend(warnings);
                let tab = &mut app.tabs[app.active_tab];
                if app.readonly_mode
                    || tab.table.undo_stack.len() != undo_len
                    || tab.table.row_count() != rows
                {
                    st.error = Some(t("refdata.changed"));
                } else {
                    let start = tab.table.undo_stack.len();
                    let names = insert_columns(&mut tab.table, &computed);
                    tab.table.coalesce_undo_since(start);
                    tab.filter_dirty = true;
                    tab.table_state.widths_initialized = false;
                    let mut msg = t("refdata.done")
                        .replace("{names}", &names.join(", "))
                        .replace("{found}", &computed.found.to_string())
                        .replace("{total}", &(computed.found + computed.missed).to_string());
                    if tab.source_has_more() {
                        msg = format!("{msg} {}", t("cohort.loaded_note"));
                    }
                    app.status_message = Some((msg, Instant::now()));
                    return;
                }
            }
            Err(e) => st.error = Some(e),
        }
    }
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    let r = app.settings.reference.clone();
    for c in [
        &mut st.amount,
        &mut st.from,
        &mut st.date,
        &mut st.country,
        &mut st.key,
    ] {
        *c = c.filter(|&i| i < names.len());
    }
    let mode_off = mode_why(st.mode, &r);
    let rounding = st.rounding();
    let why_not: Option<String> = mode_off
        .clone()
        .or_else(|| match st.mode {
            Mode::Currency
                if st.amount.is_none()
                    || (st.from_col && st.from.is_none())
                    || (!st.latest && st.date.is_none()) =>
            {
                Some(t("refdata.apply_why"))
            }
            Mode::Inflation
                if st.amount.is_none()
                    || st.inflation_date().is_none()
                    || (st.country_col && st.country.is_none()) =>
            {
                Some(t("refdata.apply_why"))
            }
            Mode::Places
                if st.key.is_none()
                    || st
                        .outputs
                        .iter()
                        .all(|o| !st.offers(*o) || !places::available(&r, *o)) =>
            {
                Some(t("refdata.apply_why"))
            }
            _ => rounding.clone().err(),
        })
        .or_else(|| st.running.is_some().then(|| t("refdata.running")));

    let (mut close, mut apply, mut open_settings) = (false, false, false);
    let dialog_id = egui::Id::new("octa_reference_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;
    let center = center_on_first_show(ctx, egui::vec2(560.0, 520.0));
    let window = egui::Window::new("octa_reference")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(520.0)
            .min_width(380.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "reference_header",
            RichText::new(t("refdata.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }
        egui::Panel::bottom("reference_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b =
                        ui.add_enabled(why_not.is_none(), egui::Button::new(t("refdata.apply")));
                    if b.clicked() {
                        apply = true;
                    }
                    match &why_not {
                        None => {
                            b.on_hover_text(t("refdata.apply_hint"));
                        }
                        Some(w) => {
                            b.on_disabled_hover_text(disabled_tip(&t("refdata.apply_hint"), w));
                        }
                    }
                    if st.running.is_some() {
                        ui.spinner();
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("refdata.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .button(t("online.open_settings"))
                        .on_hover_text(t("online.open_settings_hint"))
                        .clicked()
                    {
                        open_settings = true;
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                control_row(ui, |ui| {
                    for (m, key) in [
                        (Mode::Currency, "refdata.mode_currency"),
                        (Mode::Inflation, "refdata.mode_inflation"),
                        (Mode::Places, "refdata.mode_places"),
                    ] {
                        let why = mode_why(m, &r);
                        let resp = ui.add_enabled(
                            why.is_none(),
                            egui::RadioButton::new(st.mode == m, t(key)),
                        );
                        let hint = t(&format!("{key}_hint"));
                        let resp = match &why {
                            None => resp.on_hover_text(hint),
                            Some(w) => resp.on_disabled_hover_text(disabled_tip(&hint, w)),
                        };
                        if resp.clicked() {
                            st.set_mode(m);
                        }
                    }
                });
                ui.add_space(6.0);
                match st.mode {
                    Mode::Currency => control_grid(ui, "reference_currency", |ui| {
                        ui.label(t("refdata.amount"))
                            .on_hover_text(t("refdata.amount_hint"));
                        col_combo(ui, "ref_amount", &mut st.amount, &names)
                            .on_hover_text(t("refdata.amount_hint"));
                        ui.end_row();
                        ui.label(t("refdata.from"))
                            .on_hover_text(t("refdata.from_hint"));
                        let codes = st.currencies.clone();
                        column_or_fixed(
                            ui,
                            "ref_from",
                            (&mut st.from_col, &mut st.from, &mut st.from_code),
                            &codes,
                            &names,
                            "refdata.from_hint",
                        );
                        ui.end_row();
                        ui.label(t("refdata.to"))
                            .on_hover_text(t("refdata.to_hint"));
                        code_combo(ui, "ref_to", &mut st.to_code, &codes, "refdata.to_hint");
                        ui.end_row();
                        ui.label(t("refdata.mark"))
                            .on_hover_text(t("refdata.mark_hint"));
                        control_row(ui, |ui| {
                            ui.radio_value(&mut st.mark, Mark::Code, t("refdata.mark_code"))
                                .on_hover_text(t("refdata.mark_code_hint"));
                            ui.radio_value(&mut st.mark, Mark::Symbol, t("refdata.mark_symbol"))
                                .on_hover_text(t("refdata.mark_symbol_hint"));
                        });
                        ui.end_row();
                        ui.label(t("refdata.date"))
                            .on_hover_text(t("refdata.date_hint"));
                        control_row(ui, |ui| {
                            ui.checkbox(&mut st.latest, t("refdata.latest"))
                                .on_hover_text(t("refdata.latest_hint"));
                            if !st.latest {
                                col_combo(ui, "ref_date", &mut st.date, &names)
                                    .on_hover_text(t("refdata.date_hint"));
                            }
                        });
                        ui.end_row();
                    }),
                    Mode::Inflation => control_grid(ui, "reference_inflation", |ui| {
                        ui.label(t("refdata.amount"))
                            .on_hover_text(t("refdata.amount_hint"));
                        col_combo(ui, "ref_amount", &mut st.amount, &names)
                            .on_hover_text(t("refdata.amount_hint"));
                        ui.end_row();
                        ui.label(t("refdata.date"))
                            .on_hover_text(t("refdata.inflation_date_hint"));
                        let years = st.years();
                        control_row(ui, |ui| {
                            ui.radio_value(&mut st.date_col, true, t("refdata.use_column"))
                                .on_hover_text(t("refdata.use_column_hint"));
                            ui.radio_value(&mut st.date_col, false, t("refdata.use_fixed"))
                                .on_hover_text(t("refdata.use_fixed_hint"));
                            if st.date_col {
                                col_combo(ui, "ref_date", &mut st.date, &names)
                                    .on_hover_text(t("refdata.inflation_date_hint"));
                            } else {
                                egui::ComboBox::from_id_salt("ref_date_year")
                                    .selected_text(st.date_year.to_string())
                                    .width(70.0)
                                    .show_ui(ui, |ui| {
                                        for y in years {
                                            ui.selectable_value(
                                                &mut st.date_year,
                                                y,
                                                y.to_string(),
                                            );
                                        }
                                    })
                                    .response
                                    .on_hover_text(t("refdata.inflation_date_hint"));
                                egui::ComboBox::from_id_salt("ref_date_month")
                                    .selected_text(format!("{:02}", st.date_month))
                                    .width(50.0)
                                    .show_ui(ui, |ui| {
                                        for m in 1..=12 {
                                            ui.selectable_value(
                                                &mut st.date_month,
                                                m,
                                                format!("{m:02}"),
                                            );
                                        }
                                    })
                                    .response
                                    .on_hover_text(t("refdata.inflation_date_hint"));
                            }
                        });
                        ui.end_row();
                        ui.label(t("refdata.country"))
                            .on_hover_text(t("refdata.country_hint"));
                        let areas = st.areas.clone();
                        column_or_fixed(
                            ui,
                            "ref_country",
                            (&mut st.country_col, &mut st.country, &mut st.country_code),
                            &areas,
                            &names,
                            "refdata.country_hint",
                        );
                        ui.end_row();
                        ui.label(t("refdata.target"))
                            .on_hover_text(t("refdata.target_hint"));
                        let shown = st.target.map_or_else(
                            || t("refdata.target_default"),
                            |m| m.format("%Y-%m").to_string(),
                        );
                        egui::ComboBox::from_id_salt("ref_target")
                            .selected_text(shown)
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut st.target,
                                    None,
                                    t("refdata.target_default"),
                                );
                                for m in &st.months.clone() {
                                    ui.selectable_value(
                                        &mut st.target,
                                        Some(*m),
                                        m.format("%Y-%m").to_string(),
                                    );
                                }
                            })
                            .response
                            .on_hover_text(t("refdata.target_hint"));
                        ui.end_row();
                    }),
                    Mode::Places => {
                        control_grid(ui, "reference_places", |ui| {
                            ui.label(t("refdata.key"))
                                .on_hover_text(t("refdata.key_hint"));
                            col_combo(ui, "ref_key", &mut st.key, &names)
                                .on_hover_text(t("refdata.key_hint"));
                            ui.end_row();
                            ui.label(t("refdata.key_kind"))
                                .on_hover_text(t("refdata.key_kind_hint"));
                            egui::ComboBox::from_id_salt("ref_kind")
                                .selected_text(t(&st.kind.label_key()))
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for k in KeyKind::ALL {
                                        if k == KeyKind::Kreis && !st.kreis_stored {
                                            continue;
                                        }
                                        let off = k.sources().iter().all(|d| !r.get(*d).enabled);
                                        ui.add_enabled_ui(!off, |ui| {
                                            ui.selectable_value(&mut st.kind, k, t(&k.label_key()))
                                                .on_hover_text(t(&format!(
                                                    "{}_hint",
                                                    k.label_key()
                                                )))
                                                .on_disabled_hover_text(t(
                                                    "greyed.refdata_places_off",
                                                ));
                                        });
                                    }
                                })
                                .response
                                .on_hover_text(t("refdata.key_kind_hint"));
                            ui.end_row();
                            if matches!(st.kind, KeyKind::Postcode | KeyKind::Municipality) {
                                ui.label(t("refdata.country"))
                                    .on_hover_text(t("refdata.places_country_hint"));
                                let all = st.place_countries.clone();
                                column_or_fixed(
                                    ui,
                                    "ref_pcountry",
                                    (&mut st.country_col, &mut st.country, &mut st.country_code),
                                    &all,
                                    &names,
                                    "refdata.places_country_hint",
                                );
                                ui.end_row();
                            }
                            ui.label(t("refdata.several"))
                                .on_hover_text(t("refdata.several_hint"));
                            control_row(ui, |ui| {
                                ui.radio_value(
                                    &mut st.several,
                                    Several::All,
                                    t("refdata.several_all"),
                                )
                                .on_hover_text(t("refdata.several_all_hint"));
                                ui.radio_value(
                                    &mut st.several,
                                    Several::First,
                                    t("refdata.several_first"),
                                )
                                .on_hover_text(t("refdata.several_first_hint"));
                            });
                            ui.end_row();
                        });
                        ui.label(t("refdata.outputs"))
                            .on_hover_text(t("refdata.outputs_hint"));
                        control_row(ui, |ui| {
                            if ui
                                .button(t("column_format.select_all"))
                                .on_hover_text(t("relmap.select_all_hint"))
                                .clicked()
                            {
                                st.outputs = Output::ALL.to_vec();
                            }
                            if ui
                                .button(t("column_format.select_none"))
                                .on_hover_text(t("relmap.select_none_hint"))
                                .clicked()
                            {
                                st.outputs.clear();
                            }
                        });
                        control_row(ui, |ui| {
                            for o in Output::ALL {
                                if !st.offers(o) {
                                    continue;
                                }
                                let mut on = st.outputs.contains(&o);
                                let hint = t(&format!("{}_hint", o.label_key()));
                                let resp = ui.add_enabled(
                                    places::available(&r, o),
                                    egui::Checkbox::new(&mut on, t(&o.label_key())),
                                );
                                let resp = if places::available(&r, o) {
                                    resp.on_hover_text(hint)
                                } else {
                                    let need = o
                                        .sources()
                                        .iter()
                                        .map(|d| t(&d.label_key()))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    resp.on_disabled_hover_text(disabled_tip(
                                        &hint,
                                        &t("greyed.refdata_off").replace("{name}", &need),
                                    ))
                                };
                                if resp.changed() {
                                    if on {
                                        st.outputs.push(o);
                                    } else {
                                        st.outputs.retain(|x| *x != o);
                                    }
                                }
                            }
                        });
                    }
                }
                if st.mode != Mode::Places {
                    ui.add_space(6.0);
                    control_row(ui, |ui| {
                        ui.label(t("refdata.rounding"))
                            .on_hover_text(t("refdata.rounding_hint"));
                        if st.mode == Mode::Currency {
                            ui.radio_value(
                                &mut st.round,
                                RoundChoice::Usual,
                                t("refdata.round_usual"),
                            )
                            .on_hover_text(t("refdata.round_usual_hint"));
                        }
                        ui.radio_value(&mut st.round, RoundChoice::To, t("refdata.round_to"))
                            .on_hover_text(t("refdata.round_to_hint"));
                        ui.add_enabled_ui(st.round == RoundChoice::To, |ui| {
                            control_text_edit(
                                ui,
                                40.0,
                                egui::TextEdit::singleline(&mut st.decimals_buf),
                            )
                            .on_hover_text(t("refdata.round_to_hint"));
                        });
                        ui.label(t("refdata.decimals"));
                        ui.radio_value(&mut st.round, RoundChoice::None, t("refdata.round_none"))
                            .on_hover_text(t("refdata.round_none_hint"));
                    });
                }
                ui.add_space(6.0);
                let credits: Vec<Dataset> = match st.mode {
                    Mode::Currency => vec![Dataset::ExchangeRates],
                    Mode::Inflation => vec![Dataset::Inflation],
                    Mode::Places => vec![
                        Dataset::Postcodes,
                        Dataset::EuRegions,
                        Dataset::DePostcodeMunicipality,
                    ],
                };
                for d in credits {
                    ui.hyperlink_to(RichText::new(d.credit()).weak().size(11.0), d.credit_url())
                        .on_hover_text(t("refdata.credit_hint"));
                }
                if let Some(why) = &why_not
                    && st.running.is_none()
                {
                    octa::ui::message::selectable_message(ui, ui.visuals().warn_fg_color, why);
                }
                if let Some(e) = &st.error {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if open_settings {
        app.settings_dialog.open(&app.settings);
        app.settings_dialog.focus_online_section = true;
    }
    if apply && !app.is_readonly() && st.running.is_none() {
        let pick = |use_col: bool, col: Option<usize>, code: &str| {
            if use_col {
                col.map(Pick::Column)
            } else {
                Some(Pick::Fixed(code.to_string()))
            }
        };
        let job = match st.mode {
            Mode::Currency => st
                .amount
                .zip(pick(st.from_col, st.from, &st.from_code))
                .map(|(amount, from)| {
                    Request::Currency(CurrencySpec {
                        amount,
                        from,
                        to: st.to_code.clone(),
                        date: if st.latest { None } else { st.date },
                        rounding: rounding.clone().unwrap_or(Rounding::Usual),
                        mark: st.mark,
                    })
                }),
            Mode::Inflation => st
                .amount
                .zip(st.inflation_date())
                .zip(pick(st.country_col, st.country, &st.country_code))
                .map(|((amount, date), country)| {
                    Request::Inflation(InflationSpec {
                        amount,
                        date,
                        country,
                        target: st.target,
                        rounding: rounding.clone().unwrap_or(Rounding::Decimals(2)),
                    })
                }),
            Mode::Places => st.key.map(|key| {
                Request::Places(PlacesSpec {
                    key,
                    kind: st.kind,
                    country: pick(st.country_col, st.country, &st.country_code)
                        .unwrap_or(Pick::Fixed(st.country_code.clone())),
                    outputs: Output::ALL
                        .into_iter()
                        .filter(|o| {
                            st.outputs.contains(o) && st.offers(*o) && places::available(&r, *o)
                        })
                        .collect(),
                    several: st.several,
                })
            }),
        };
        if let Some(job) = job {
            let tab = &app.tabs[app.active_tab];
            // ponytail: the worker reads a full copy of the table; a huge tab
            // doubles its memory for the run. Hand over only the needed
            // columns if that ever bites.
            let mut snapshot = tab.table.clone();
            snapshot.apply_edits();
            let slot: Slot = Arc::default();
            st.running = Some((
                slot.clone(),
                tab.table.undo_stack.len(),
                tab.table.row_count(),
            ));
            st.error = None;
            let task = app.long_tasks.start(t("refdata.running"));
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _task = task;
                let result = run_job(snapshot, job, r);
                *slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(result);
                repaint.request_repaint();
            });
        }
    }
    if !close {
        app.reference_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "reference_data_tests.rs"]
mod tests;
