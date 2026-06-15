//! Anonymise-columns dialog (Edit -> Anonymise columns...). Pick per-rule
//! columns + a strategy + one shared salt, then choose where the result goes
//! (replace in place, add as new columns, or a new tab). The scrambling is the
//! pure [`octa::data::transform::anonymize_table`]; this file is the picker +
//! dispatch only. A whole apply is one undo step (`coalesce_undo_since`).

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::transform::{
    AnonRule, AnonSource, AnonSpec, AnonStrategy, FakeKind, HashAlgo, KeepEnd, RedactToken,
    anonymize_table,
};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{
    AnonRuleDraft, AnonStrategyKind, AnonymizeOutput, AnonymizeState, OctaApp, TabState,
};

pub(crate) fn render_anonymize_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.anonymize_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let mut close = false;
    let mut apply = false;
    let mut st = app.anonymize_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_anonymize_dialog");
    let window = egui::Window::new("octa_anonymize")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(760.0)
            .default_height(480.0)
            .min_width(600.0)
            .min_height(280.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("anon_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("anonymize.title"))
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

        egui::Panel::bottom("anon_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("anonymize.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("anonymize.desc"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(6.0);
                    rule_list(ui, &mut st, &col_names);
                    ui.add_space(6.0);
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(RichText::new(octa::i18n::t("anonymize.salt")).strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut st.salt)
                                .desired_width(220.0)
                                .hint_text(octa::i18n::t("anonymize.salt_hint")),
                        )
                        .on_hover_text(octa::i18n::t("anonymize.salt_tip"));
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(octa::i18n::t("anonymize.output")).strong());
                    ui.radio_value(
                        &mut st.output,
                        AnonymizeOutput::InPlace,
                        octa::i18n::t("anonymize.out_in_place"),
                    );
                    ui.radio_value(
                        &mut st.output,
                        AnonymizeOutput::NewColumns,
                        octa::i18n::t("anonymize.out_new_columns"),
                    );
                    ui.radio_value(
                        &mut st.output,
                        AnonymizeOutput::NewTab,
                        octa::i18n::t("anonymize.out_new_tab"),
                    );

                    if let Some(err) = &st.error {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(err)
                                .color(ui.visuals().error_fg_color)
                                .size(11.0),
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
        match apply_anonymize(app, &st) {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.anonymize_dialog = Some(st);
    }
}

/// The per-rule rows (columns + strategy + that strategy's parameters).
fn rule_list(ui: &mut egui::Ui, st: &mut AnonymizeState, cols: &[String]) {
    let mut remove_idx: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("anon_rules")
        .auto_shrink([false, true])
        .max_height(260.0)
        .show(ui, |ui| {
            for (i, rule) in st.rules.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    // Multi-column picker.
                    let label = if rule.columns.is_empty() {
                        octa::i18n::t("anonymize.pick_column")
                    } else {
                        rule.columns
                            .iter()
                            .filter_map(|c| cols.get(*c).cloned())
                            .collect::<Vec<_>>()
                            .join(" + ")
                    };
                    egui::ComboBox::from_id_salt(("anon_cols", i))
                        .selected_text(label)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (c, name) in cols.iter().enumerate() {
                                let mut on = rule.columns.contains(&c);
                                if ui.checkbox(&mut on, name).changed() {
                                    if on {
                                        rule.columns.insert(c);
                                    } else {
                                        rule.columns.remove(&c);
                                    }
                                }
                            }
                        })
                        .response
                        .on_hover_text(octa::i18n::t("anonymize.columns_hint"));

                    // Strategy.
                    egui::ComboBox::from_id_salt(("anon_strat", i))
                        .selected_text(rule.kind.label_t())
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for &k in AnonStrategyKind::ALL {
                                if ui.selectable_label(rule.kind == k, k.label_t()).clicked() {
                                    rule.kind = k;
                                }
                            }
                        })
                        .response
                        .on_hover_text(octa::i18n::t("anonymize.strategy_hint"));

                    strategy_params(ui, i, rule);

                    // New-column name for a multi-source hash.
                    if rule.kind == AnonStrategyKind::Hash && rule.columns.len() >= 2 {
                        ui.label(octa::i18n::t("anonymize.new_column_name"));
                        ui.add(
                            egui::TextEdit::singleline(&mut rule.new_column)
                                .desired_width(120.0)
                                .hint_text("person_id"),
                        );
                    }

                    if ui
                        .small_button("\u{2716}")
                        .on_hover_text(octa::i18n::t("dialog.cnf_remove"))
                        .clicked()
                    {
                        remove_idx = Some(i);
                    }
                });
            }
        });

    if ui.button(octa::i18n::t("anonymize.add_rule")).clicked() {
        st.rules.push(AnonRuleDraft::default());
    }
    if let Some(i) = remove_idx
        && i < st.rules.len()
    {
        st.rules.remove(i);
    }
}

/// The parameter controls that depend on the chosen strategy.
fn strategy_params(ui: &mut egui::Ui, i: usize, rule: &mut AnonRuleDraft) {
    match rule.kind {
        AnonStrategyKind::Hash => {
            ui.label(octa::i18n::t("anonymize.hash_algo"));
            egui::ComboBox::from_id_salt(("anon_algo", i))
                .selected_text(match rule.hash_algo {
                    HashAlgo::Sha256 => octa::i18n::t("hash_algo.sha256"),
                    HashAlgo::Blake3 => octa::i18n::t("hash_algo.blake3"),
                })
                .width(100.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut rule.hash_algo,
                        HashAlgo::Sha256,
                        octa::i18n::t("hash_algo.sha256"),
                    );
                    ui.selectable_value(
                        &mut rule.hash_algo,
                        HashAlgo::Blake3,
                        octa::i18n::t("hash_algo.blake3"),
                    );
                });
            ui.checkbox(&mut rule.hash_full, octa::i18n::t("anonymize.hash_full"))
                .on_hover_text(octa::i18n::t("anonymize.hash_full_hint"));
            if !rule.hash_full {
                ui.label(octa::i18n::t("anonymize.hash_length"));
                ui.add(egui::TextEdit::singleline(&mut rule.hash_length).desired_width(40.0))
                    .on_hover_text(octa::i18n::t("anonymize.hash_length_hint"));
            }
        }
        AnonStrategyKind::PartialMask => {
            ui.label(octa::i18n::t("anonymize.keep_end"));
            egui::ComboBox::from_id_salt(("anon_keep", i))
                .selected_text(match rule.keep_end {
                    KeepEnd::First => octa::i18n::t("keep_end.first"),
                    KeepEnd::Last => octa::i18n::t("keep_end.last"),
                })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut rule.keep_end,
                        KeepEnd::First,
                        octa::i18n::t("keep_end.first"),
                    );
                    ui.selectable_value(
                        &mut rule.keep_end,
                        KeepEnd::Last,
                        octa::i18n::t("keep_end.last"),
                    );
                });
            ui.label(octa::i18n::t("anonymize.keep_count"));
            ui.add(egui::TextEdit::singleline(&mut rule.mask_count).desired_width(40.0));
            ui.label(octa::i18n::t("anonymize.mask_char"));
            ui.add(egui::TextEdit::singleline(&mut rule.mask_char).desired_width(30.0));
        }
        AnonStrategyKind::Redact => {
            ui.checkbox(
                &mut rule.redact_use_null,
                octa::i18n::t("anonymize.redact_use_null"),
            );
            ui.add_enabled_ui(!rule.redact_use_null, |ui| {
                ui.label(octa::i18n::t("anonymize.redact_token"));
                ui.add(egui::TextEdit::singleline(&mut rule.redact_token).desired_width(110.0));
            });
        }
        AnonStrategyKind::Fake => {
            ui.label(octa::i18n::t("anonymize.fake_kind"));
            egui::ComboBox::from_id_salt(("anon_fake", i))
                .selected_text(fake_label(rule.fake_kind))
                .width(110.0)
                .show_ui(ui, |ui| {
                    for k in [
                        FakeKind::Name,
                        FakeKind::Email,
                        FakeKind::City,
                        FakeKind::Company,
                        FakeKind::Phone,
                        FakeKind::Uuid,
                    ] {
                        if ui
                            .selectable_label(rule.fake_kind == k, fake_label(k))
                            .clicked()
                        {
                            rule.fake_kind = k;
                        }
                    }
                });
        }
    }
}

fn fake_label(kind: FakeKind) -> String {
    match kind {
        FakeKind::Name => octa::i18n::t("fake_kind.name"),
        FakeKind::Email => octa::i18n::t("fake_kind.email"),
        FakeKind::City => octa::i18n::t("fake_kind.city"),
        FakeKind::Company => octa::i18n::t("fake_kind.company"),
        FakeKind::Phone => octa::i18n::t("fake_kind.phone"),
        FakeKind::Uuid => octa::i18n::t("fake_kind.uuid"),
    }
}

/// Build an [`AnonStrategy`] from a draft row's active fields.
fn draft_strategy(rule: &AnonRuleDraft) -> AnonStrategy {
    match rule.kind {
        AnonStrategyKind::Hash => AnonStrategy::Hash {
            algo: rule.hash_algo,
            length: if rule.hash_full {
                None
            } else {
                Some(rule.hash_length.trim().parse().unwrap_or(12).clamp(1, 64))
            },
        },
        AnonStrategyKind::PartialMask => AnonStrategy::PartialMask {
            keep: rule.keep_end,
            count: rule.mask_count.trim().parse().unwrap_or(4),
            mask_char: rule.mask_char.chars().next().unwrap_or('*'),
        },
        AnonStrategyKind::Redact => AnonStrategy::Redact {
            token: if rule.redact_use_null {
                RedactToken::Null
            } else {
                RedactToken::Fixed(rule.redact_token.clone())
            },
        },
        AnonStrategyKind::Fake => AnonStrategy::Fake {
            kind: rule.fake_kind,
        },
    }
}

/// Resolve drafts to an [`AnonSpec`] and apply per the chosen output mode.
fn apply_anonymize(app: &mut OctaApp, st: &AnonymizeState) -> Result<(), String> {
    if app.is_readonly() {
        return Err(octa::i18n::t("transform.readonly"));
    }
    let rules: Vec<AnonRule> = st
        .rules
        .iter()
        .filter(|r| !r.columns.is_empty())
        .map(|r| AnonRule {
            columns: r.columns.iter().copied().collect(),
            strategy: draft_strategy(r),
            new_column: {
                let n = r.new_column.trim();
                if n.is_empty() {
                    None
                } else {
                    Some(n.to_string())
                }
            },
        })
        .collect();
    if rules.is_empty() {
        return Err(octa::i18n::t("anonymize.need_rule"));
    }
    let spec = AnonSpec {
        rules,
        salt: st.salt.clone(),
    };
    let active = app.active_tab;
    let outputs = anonymize_table(&app.tabs[active].table, &spec);
    let n_out = outputs.len();

    match st.output {
        AnonymizeOutput::InPlace => {
            apply_outputs(&mut app.tabs[active].table, &outputs, false);
            app.tabs[active].table_state.widths_initialized = false;
            app.tabs[active].filter_dirty = true;
        }
        AnonymizeOutput::NewColumns => {
            apply_outputs(&mut app.tabs[active].table, &outputs, true);
            app.tabs[active].table_state.widths_initialized = false;
            app.tabs[active].filter_dirty = true;
        }
        AnonymizeOutput::NewTab => {
            let mut copy = clone_table(&app.tabs[active].table);
            apply_outputs(&mut copy, &outputs, false);
            copy.format_name = Some(octa::i18n::t("anonymize.new_tab_suffix"));
            let mut new_tab = TabState::new(app.settings.default_search_mode);
            new_tab.table = copy;
            new_tab.filter_dirty = true;
            app.tabs.push(new_tab);
            app.active_tab = app.tabs.len() - 1;
        }
    }
    app.status_message = Some((
        octa::i18n::t("anonymize.done_in_place").replace("{n}", &n_out.to_string()),
        std::time::Instant::now(),
    ));
    Ok(())
}

/// Apply the engine outputs to `tbl` as one undo step. `new_columns` forces
/// every output into a fresh column; otherwise `Column` outputs replace.
fn apply_outputs(
    tbl: &mut DataTable,
    outputs: &[octa::data::transform::AnonOutput],
    new_columns: bool,
) {
    let start = tbl.undo_stack.len();
    for o in outputs {
        match &o.source {
            AnonSource::Column(c) if !new_columns => {
                for (r, v) in o.values.iter().enumerate() {
                    tbl.set(r, *c, v.clone());
                }
            }
            AnonSource::Column(c) => {
                let base = tbl
                    .columns
                    .get(*c)
                    .map(|x| x.name.clone())
                    .unwrap_or_default();
                append_anon_column(tbl, &format!("{base}_anon"), &o.values);
            }
            AnonSource::Derived { name } => append_anon_column(tbl, name, &o.values),
        }
    }
    tbl.coalesce_undo_since(start);
}

/// Append a Utf8 column via insert_column (undoable) + set, uniquifying the name.
fn append_anon_column(tbl: &mut DataTable, name: &str, values: &[octa::data::CellValue]) {
    let mut unique = name.to_string();
    let mut k = 2;
    while tbl.columns.iter().any(|c| c.name == unique) {
        unique = format!("{name}_{k}");
        k += 1;
    }
    let idx = tbl.col_count();
    tbl.insert_column(idx, unique, "Utf8".into());
    for (r, v) in values.iter().enumerate() {
        tbl.set(r, idx, v.clone());
    }
}

/// A detached, edit-free clone of `src` with no source path (Save prompts for
/// one) - same convention as the Find-duplicates new-tab output.
fn clone_table(src: &DataTable) -> DataTable {
    DataTable {
        columns: src.columns.clone(),
        rows: src.rows.clone(),
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}
