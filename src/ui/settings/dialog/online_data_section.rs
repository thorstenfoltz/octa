//! Online data: the five reference downloads, each off until switched on,
//! with its refresh interval, Download now, size, Delete and credit line.
//! Downloads run through the app's `Jobs` handle (`reference_jobs`).

use std::collections::HashMap;
use std::path::Path;

use egui::RichText;

use super::super::*;
use crate::i18n::t;
use crate::reference::{self, Dataset, IntervalUnit, SourceSettings};
use crate::ui::control_row::{control_line, control_text_edit};
use crate::ui::menu_entry::disabled_tip;

/// The section's typed-but-not-yet-valid interval texts; reset on open.
#[derive(Default)]
pub(crate) struct OnlineForm {
    every_buf: HashMap<Dataset, String>,
}

impl OnlineForm {
    /// Take a typed interval: a whole number from 1 up, else the old one stays.
    pub(crate) fn take_every(
        &mut self,
        d: Dataset,
        text: &str,
        s: &mut SourceSettings,
    ) -> Result<(), ()> {
        self.every_buf.insert(d, text.to_string());
        match text.trim().parse::<u32>() {
            Ok(n) if n >= 1 => {
                s.every = n;
                Ok(())
            }
            _ => Err(()),
        }
    }
}

fn human(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{} KB", bytes.div_ceil(1000))
    }
}

impl SettingsDialog {
    pub(super) fn online_data_section_body(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(t("online.intro")).weak().size(11.0));
        ui.add_space(6.0);
        let dir = reference::store::dir().ok();
        for d in Dataset::ALL {
            ui.push_id(d.id(), |ui| self.online_block(ui, d, dir.as_deref()));
            ui.add_space(10.0);
        }
    }

    fn online_block(&mut self, ui: &mut egui::Ui, d: Dataset, dir: Option<&Path>) {
        ui.label(RichText::new(t(&d.label_key())).strong());
        let jobs = self.reference_jobs.clone();
        let running = jobs.as_ref().is_some_and(|j| j.running(d));
        {
            let s = self.draft.reference.get_mut(d);
            ui.checkbox(&mut s.enabled, t("online.enable"))
                .on_hover_text(t(&d.hint_key()));
        }
        let on = self.draft.reference.get(d).enabled;
        let off_why = t("greyed.online_off");
        let mut bad = false;
        control_line(ui, |ui| {
            ui.label(t("online.every"))
                .on_hover_text(t("online.every_hint"));
            let current = self.draft.reference.get(d).every.to_string();
            let mut text = self.online.every_buf.get(&d).cloned().unwrap_or(current);
            let edit = ui
                .add_enabled_ui(on, |ui| {
                    control_text_edit(ui, 50.0, egui::TextEdit::singleline(&mut text))
                })
                .inner
                .on_hover_text(t("online.every_hint"))
                .on_disabled_hover_text(disabled_tip(&t("online.every_hint"), &off_why));
            if edit.changed() {
                let s = self.draft.reference.get_mut(d);
                bad = self.online.take_every(d, &text, s).is_err();
            } else {
                bad = self
                    .online
                    .every_buf
                    .get(&d)
                    .is_some_and(|b| b.trim().parse::<u32>().map_or(true, |n| n < 1));
            }
            let unit = &mut self.draft.reference.get_mut(d).unit;
            let label = |u: IntervalUnit| {
                t(if u == IntervalUnit::Days {
                    "online.unit_days"
                } else {
                    "online.unit_months"
                })
            };
            ui.add_enabled_ui(on, |ui| {
                egui::ComboBox::from_id_salt("unit")
                    .selected_text(label(*unit))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(unit, IntervalUnit::Days, label(IntervalUnit::Days));
                        ui.selectable_value(
                            unit,
                            IntervalUnit::Months,
                            label(IntervalUnit::Months),
                        );
                    })
                    .response
                    .on_hover_text(t("online.unit_hint"))
                    .on_disabled_hover_text(disabled_tip(&t("online.unit_hint"), &off_why));
            });
        });
        if bad {
            crate::ui::message::selectable_message(
                ui,
                ui.visuals().warn_fg_color,
                &t("online.every_bad"),
            );
        }
        let size = dir.map_or(0, |dir| reference::stored_size(d, dir));
        let status = match dir.and_then(|dir| reference::last_downloaded(d, dir)) {
            Some(when) => t("online.last")
                .replace("{when}", &when)
                .replace("{size}", &human(size)),
            None => t("online.never"),
        };
        ui.label(RichText::new(status).weak());
        control_line(ui, |ui| {
            let why = if !on {
                Some(off_why.clone())
            } else if running {
                Some(t("greyed.online_running"))
            } else {
                None
            };
            let b = ui.add_enabled(
                why.is_none() && jobs.is_some(),
                egui::Button::new(t("online.download_now")),
            );
            let b = match &why {
                None => b.on_hover_text(t("online.download_now_hint")),
                Some(w) => {
                    b.on_disabled_hover_text(disabled_tip(&t("online.download_now_hint"), w))
                }
            };
            if b.clicked()
                && let Some(j) = &jobs
            {
                j.start(d, Vec::new());
            }
            if running {
                ui.spinner();
            }
            let del_why = if size == 0 {
                Some(t("greyed.online_nothing"))
            } else if running {
                Some(t("greyed.online_running"))
            } else {
                None
            };
            let del = ui.add_enabled(
                del_why.is_none(),
                egui::Button::new(t("online.delete").replace("{size}", &human(size))),
            );
            let del = match &del_why {
                None => del.on_hover_text(t("online.delete_hint")),
                Some(w) => del.on_disabled_hover_text(disabled_tip(&t("online.delete_hint"), w)),
            };
            if del.clicked()
                && let Some(dir) = dir
                && let Err(e) = reference::delete(d, dir)
            {
                self.online_error = Some(format!("{e:#}"));
            }
        });
        ui.hyperlink_to(RichText::new(d.credit()).weak().size(11.0), d.credit_url())
            .on_hover_text(t("online.credit_hint"));
        if let Some(e) = jobs.as_ref().and_then(|j| j.last_error(d)) {
            crate::ui::message::selectable_message(ui, ui.visuals().error_fg_color, &e);
        }
    }
}
