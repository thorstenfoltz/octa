//! Detect-PII dialog (Analyse -> Detect PII...).
//!
//! Read-only report of columns that look like personal data (email, phone,
//! IBAN, credit card, SSN), produced by the pure engine
//! [`octa::data::pii::scan_pii`]. A "Send to Anonymise" button seeds the
//! existing Anonymise dialog with one Hash rule per finding
//! (`octa::data::pii::suggested_anon_rules` describes the same defaults).

use eframe::egui;
use egui::RichText;

use octa::data::pii::{ColumnPii, PiiKind, scan_pii};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{AnonRuleDraft, AnonymizeState, OctaApp, PiiState};

/// Rows sampled per column when scanning for PII.
const PII_SAMPLE_ROWS: usize = 1000;

impl OctaApp {
    /// Scan the active table for PII and open the report dialog.
    pub(crate) fn open_pii_dialog(&mut self) {
        self.tabs[self.active_tab].table.apply_edits();
        let findings = scan_pii(&self.tabs[self.active_tab].table, PII_SAMPLE_ROWS);
        self.pii_dialog = Some(PiiState {
            findings,
            size: DialogSize::default(),
        });
    }
}

fn kind_label(kind: PiiKind) -> String {
    let key = match kind {
        PiiKind::Email => "pii_kind.email",
        PiiKind::Phone => "pii_kind.phone",
        PiiKind::Ip => "pii_kind.ip",
        PiiKind::Iban => "pii_kind.iban",
        PiiKind::CreditCard => "pii_kind.credit_card",
        PiiKind::Ssn => "pii_kind.ssn",
        PiiKind::Name => "pii_kind.name",
        PiiKind::Gender => "pii_kind.gender",
        PiiKind::Country => "pii_kind.country",
        PiiKind::BirthDate => "pii_kind.birth_date",
        PiiKind::PostalCode => "pii_kind.postal_code",
        PiiKind::Address => "pii_kind.address",
    };
    octa::i18n::t(key)
}

/// Short explanation of what drove the confidence for one finding.
fn basis_label(f: &octa::data::pii::ColumnPii) -> String {
    let pct = format!("{:.0}%", f.value_match * 100.0);
    if f.by_name && f.value_match > 0.0 {
        octa::i18n::t("pii.basis_both").replace("{pct}", &pct)
    } else if f.by_name {
        octa::i18n::t("pii.basis_name")
    } else {
        octa::i18n::t("pii.basis_values").replace("{pct}", &pct)
    }
}

pub(crate) fn render_pii_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pii_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let mut close = false;
    let mut send_to_anonymise = false;
    let mut st = app.pii_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_pii_dialog");
    let window = egui::Window::new("octa_pii")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(340.0)
            .min_width(340.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("pii_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("pii.title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("pii_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if !st.findings.is_empty()
                        && ui.button(octa::i18n::t("pii.send_to_anon")).clicked()
                    {
                        send_to_anonymise = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("pii.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if st.findings.is_empty() {
                ui.label(
                    RichText::new(octa::i18n::t("pii.none_found"))
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }

            ui.label(
                RichText::new(octa::i18n::t("pii.desc"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            // Plain-language legend, placed ABOVE the scroll area so the scroll
            // area stays the last (filling) element - a label after a
            // `[false, false]` scroll area makes the window grow every frame.
            ui.label(
                RichText::new(octa::i18n::t("pii.confidence_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("pii_grid")
                        .num_columns(4)
                        .striped(true)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new(octa::i18n::t("pii.col_column")).strong());
                            ui.label(RichText::new(octa::i18n::t("pii.col_kind")).strong());
                            ui.label(RichText::new(octa::i18n::t("pii.col_confidence")).strong())
                                .on_hover_text(octa::i18n::t("pii.confidence_tip"));
                            ui.label(RichText::new(octa::i18n::t("pii.col_basis")).strong())
                                .on_hover_text(octa::i18n::t("pii.basis_tip"));
                            ui.end_row();

                            for f in &st.findings {
                                let name = col_names
                                    .get(f.column)
                                    .cloned()
                                    .unwrap_or_else(|| format!("col {}", f.column));
                                ui.label(name);
                                ui.label(kind_label(f.kind));
                                ui.label(format!("{:.0}%", f.confidence * 100.0))
                                    .on_hover_text(octa::i18n::t("pii.confidence_tip"));
                                ui.label(
                                    RichText::new(basis_label(f))
                                        .color(ui.visuals().weak_text_color()),
                                )
                                .on_hover_text(octa::i18n::t("pii.basis_tip"));
                                ui.end_row();
                            }
                        });
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if send_to_anonymise {
        seed_anonymise_from_pii(app, &st.findings);
        return; // PII dialog dropped; Anonymise dialog now open.
    }
    if !close {
        app.pii_dialog = Some(st);
    }
}

/// Open the Anonymise dialog pre-seeded with one (default Hash) rule per PII
/// finding. Mirrors `octa::data::pii::suggested_anon_rules`, expressed as the
/// dialog's editable `AnonRuleDraft`s so the user can adjust before applying.
fn seed_anonymise_from_pii(app: &mut OctaApp, findings: &[ColumnPii]) {
    let rules: Vec<AnonRuleDraft> = findings
        .iter()
        .map(|f| {
            let mut draft = AnonRuleDraft::default(); // default kind = Hash
            draft.columns.insert(f.column);
            draft
        })
        .collect();
    let mut state = AnonymizeState::default();
    if !rules.is_empty() {
        state.rules = rules;
    }
    app.anonymize_dialog = Some(state);
}
