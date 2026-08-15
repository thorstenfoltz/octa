//! Fuzzy join dialog: match rows that are similar rather than identical.
//!
//! Reached from **Data -> Fuzzy join...**. Modelled on `join.rs` for its
//! layout and on `schema_drift.rs` for its worker: without a blocking column
//! the comparison is quadratic, so it must never run on the UI thread.
//!
//! The engine (`octa::data::fuzzy_join`) is the same one the CLI
//! `--fuzzy-join` flag and the `fuzzy_join` MCP tool use.

use eframe::egui;
use egui::RichText;

use octa::data::fuzzy_duplicates::{NormalizeOpts, SimilarityMethod};
use octa::data::fuzzy_join::{FuzzyJoinStep, fuzzy_join};
use octa::data::join::JoinType;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{FuzzyJoinState, FuzzyStepDraft, OctaApp};

use super::widgets::col_combo;

use std::sync::atomic::Ordering;

const METHODS: &[(SimilarityMethod, &str)] = &[
    (SimilarityMethod::EditRatio, "Edit ratio"),
    (SimilarityMethod::JaroWinkler, "Jaro-Winkler"),
    (SimilarityMethod::TokenSet, "Token set"),
];

const JOIN_TYPES: &[(JoinType, &str)] = &[
    (JoinType::Inner, "Inner"),
    (JoinType::Left, "Left"),
    (JoinType::Right, "Right"),
    (JoinType::Full, "Full"),
];

/// Column names of a tab, for the pickers.
fn col_names(app: &OctaApp, tab: usize) -> Vec<String> {
    app.tabs
        .get(tab)
        .map(|t| t.table.columns.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default()
}

/// Tabs that actually have columns, as `(index, label)`.
fn joinable_tabs(app: &OctaApp) -> Vec<(usize, String)> {
    (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| {
            (
                i,
                crate::app::chat_panel::helpers::tab_display_name(&app.tabs[i], i),
            )
        })
        .collect()
}

fn tab_combo(ui: &mut egui::Ui, id: &str, sel: &mut usize, tabs: &[(usize, String)]) {
    let text = tabs
        .iter()
        .find(|(i, _)| i == sel)
        .map(|(_, n)| n.clone())
        .unwrap_or_default();
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(200.0)
        .show_ui(ui, |ui| {
            for (i, name) in tabs {
                ui.selectable_value(sel, *i, name);
            }
        });
}

pub(crate) fn render_fuzzy_join_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.fuzzy_join_dialog.is_none() {
        return;
    }

    let tabs = joinable_tabs(app);
    let mut close = false;
    let mut go = false;
    let mut add_step = false;
    let mut remove_step: Option<usize> = None;
    let mut st = app.fuzzy_join_dialog.take().unwrap();

    let running = st.running.load(Ordering::Relaxed);
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // The left-hand column list per step. Step 0 compares against the left
    // table; later steps compare against the table the previous step brought
    // in. The fold's accumulated columns are not offered: naming them exactly
    // would mean running the join to find out, and the previous right table is
    // what a user actually reaches for.
    let step_left_cols: Vec<Vec<String>> = std::iter::once(st.left_tab)
        .chain(st.steps.iter().map(|s| s.right_tab))
        .take(st.steps.len())
        .map(|tab| col_names(app, tab))
        .collect();

    {
        let dialog_id = egui::Id::new("octa_fuzzy_join_dialog_v1");
        let window = egui::Window::new("octa_fuzzy_join")
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(680.0)
                .default_height(480.0)
                .min_width(460.0)
                .min_height(280.0)
        });

        let inner = window.show(ctx, |ui| {
            egui::Panel::top("fuzzy_join_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(octa::i18n::t("fuzzy_join.title"))
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

            egui::Panel::bottom("fuzzy_join_footer")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if running {
                            ui.spinner();
                            ui.label(octa::i18n::t("fuzzy_join.running"));
                            if ui.button(octa::i18n::t("fuzzy_join.cancel")).clicked() {
                                st.cancel.store(true, Ordering::Relaxed);
                            }
                        } else {
                            let ready = tabs.len() >= 2
                                && st.steps.iter().all(|s| {
                                    s.pairs.iter().any(|(l, r)| l.is_some() && r.is_some())
                                });
                            if ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(octa::i18n::t("fuzzy_join.run")),
                                )
                                .on_hover_text(octa::i18n::t("fuzzy_join.run_hint"))
                                .on_disabled_hover_text(octa::i18n::t("fuzzy_join.pairs_hint"))
                                .clicked()
                            {
                                go = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button(octa::i18n::t("fuzzy_join.cancel")).clicked() {
                                        close = true;
                                    }
                                },
                            );
                        }
                    });
                });

            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("fuzzy_join_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(octa::i18n::t("fuzzy_join.intro"));
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label(octa::i18n::t("fuzzy_join.left"))
                                .on_hover_text(octa::i18n::t("fuzzy_join.left_hint"));
                            tab_combo(ui, "fj_left_tab", &mut st.left_tab, &tabs);
                        });

                        let step_count = st.steps.len();
                        for (i, step) in st.steps.iter_mut().enumerate() {
                            ui.separator();
                            let right_cols = col_names(app, step.right_tab);
                            let compare_cols = step_left_cols.get(i).cloned().unwrap_or_default();

                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("fuzzy_join.right"))
                                    .on_hover_text(octa::i18n::t("fuzzy_join.right_hint"));
                                tab_combo(
                                    ui,
                                    &format!("fj_right_tab_{i}"),
                                    &mut step.right_tab,
                                    &tabs,
                                );
                                if step_count > 1
                                    && ui.button(octa::i18n::t("fuzzy_join.remove_step")).clicked()
                                {
                                    remove_step = Some(i);
                                }
                            });

                            ui.add_space(4.0);
                            ui.label(octa::i18n::t("fuzzy_join.pairs"))
                                .on_hover_text(octa::i18n::t("fuzzy_join.pairs_hint"));
                            let pair_count = step.pairs.len();
                            let mut drop_pair: Option<usize> = None;
                            for (j, (l, r)) in step.pairs.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    col_combo(ui, &format!("fj_l_{i}_{j}"), l, &compare_cols);
                                    ui.label("~");
                                    col_combo(ui, &format!("fj_r_{i}_{j}"), r, &right_cols);
                                    if pair_count > 1 && ui.small_button("x").clicked() {
                                        drop_pair = Some(j);
                                    }
                                });
                            }
                            if let Some(j) = drop_pair {
                                step.pairs.remove(j);
                            }
                            if ui.small_button("+").clicked() {
                                step.pairs.push((None, None));
                            }

                            ui.add_space(4.0);
                            egui::Grid::new(format!("fj_grid_{i}"))
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label(octa::i18n::t("fuzzy_join.method"))
                                        .on_hover_text(octa::i18n::t("fuzzy_join.method_hint"));
                                    egui::ComboBox::from_id_salt(format!("fj_method_{i}"))
                                        .selected_text(
                                            METHODS
                                                .iter()
                                                .find(|(m, _)| *m == step.method)
                                                .map(|(_, n)| *n)
                                                .unwrap_or(""),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (m, name) in METHODS {
                                                ui.selectable_value(&mut step.method, *m, *name);
                                            }
                                        });
                                    ui.end_row();

                                    ui.label(octa::i18n::t("fuzzy_join.threshold"))
                                        .on_hover_text(octa::i18n::t("fuzzy_join.threshold_hint"));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut step.threshold_text)
                                            .desired_width(70.0),
                                    );
                                    ui.end_row();

                                    ui.label(octa::i18n::t("fuzzy_join.block"))
                                        .on_hover_text(octa::i18n::t("fuzzy_join.block_hint"));
                                    ui.horizontal(|ui| {
                                        col_combo(
                                            ui,
                                            &format!("fj_bl_{i}"),
                                            &mut step.block.0,
                                            &compare_cols,
                                        );
                                        ui.label("=");
                                        col_combo(
                                            ui,
                                            &format!("fj_br_{i}"),
                                            &mut step.block.1,
                                            &right_cols,
                                        );
                                        if (step.block.0.is_some() || step.block.1.is_some())
                                            && ui.small_button("x").clicked()
                                        {
                                            step.block = (None, None);
                                        }
                                        if step.block.0.is_none() && step.block.1.is_none() {
                                            ui.weak(octa::i18n::t("fuzzy_join.block_none"));
                                        }
                                    });
                                    ui.end_row();

                                    ui.label(octa::i18n::t("fuzzy_join.join_type"));
                                    egui::ComboBox::from_id_salt(format!("fj_how_{i}"))
                                        .selected_text(
                                            JOIN_TYPES
                                                .iter()
                                                .find(|(h, _)| *h == step.join_type)
                                                .map(|(_, n)| *n)
                                                .unwrap_or(""),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (h, name) in JOIN_TYPES {
                                                ui.selectable_value(&mut step.join_type, *h, *name);
                                            }
                                        });
                                    ui.end_row();

                                    ui.label(octa::i18n::t("fuzzy_join.max_rows"))
                                        .on_hover_text(octa::i18n::t("fuzzy_join.max_rows_hint"));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut step.max_rows_text)
                                            .desired_width(90.0),
                                    );
                                    ui.end_row();
                                });
                        }

                        ui.separator();
                        if ui
                            .button(octa::i18n::t("fuzzy_join.add_step"))
                            .on_hover_text(octa::i18n::t("fuzzy_join.add_step_hint"))
                            .clicked()
                        {
                            add_step = true;
                        }
                    });
            });
        });

        if let Some(inner) = inner.as_ref() {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
    }

    st.size = size;

    if let Some(i) = remove_step {
        st.steps.remove(i);
    }
    if add_step {
        let next = tabs
            .iter()
            .map(|(i, _)| *i)
            .find(|i| *i != st.left_tab)
            .unwrap_or(st.left_tab);
        st.steps.push(FuzzyStepDraft::new(next));
    }

    if close && !running {
        return; // dialog dropped
    }
    app.fuzzy_join_dialog = Some(st);
    if go {
        start_join(app, ctx);
    }
}

fn start_join(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(st) = &app.fuzzy_join_dialog else {
        return;
    };
    let left_tab = st.left_tab;
    let tab_order: Vec<usize> = std::iter::once(left_tab)
        .chain(st.steps.iter().map(|s| s.right_tab))
        .collect();

    // Snapshot every source before spawning: the worker must not borrow app
    // state, and the join should see the edits on screen.
    let mut snapshots = Vec::with_capacity(tab_order.len());
    for &idx in &tab_order {
        let Some(tab) = app.tabs.get_mut(idx) else {
            return;
        };
        tab.table.apply_edits();
        snapshots.push(tab.table.clone());
    }

    let Some(st) = &mut app.fuzzy_join_dialog else {
        return;
    };
    let steps: Vec<FuzzyJoinStep> = st
        .steps
        .iter()
        .map(|s| FuzzyJoinStep {
            pairs: s
                .pairs
                .iter()
                .filter_map(|(l, r)| Some(((*l)?, (*r)?)))
                .collect(),
            method: s.method,
            threshold: s
                .threshold_text
                .replace(',', ".")
                .trim()
                .parse::<f64>()
                .unwrap_or(0.85)
                .clamp(0.0, 1.0),
            normalize: NormalizeOpts::default(),
            block: match s.block {
                (Some(l), Some(r)) => Some((l, r)),
                _ => None,
            },
            how: s.join_type,
            max_rows: s
                .max_rows_text
                .replace([',', '.', ' '], "")
                .parse::<usize>()
                .unwrap_or(20_000)
                .max(1),
        })
        .collect();

    st.cancel.store(false, Ordering::Relaxed);
    st.running.store(true, Ordering::Relaxed);

    let running = std::sync::Arc::clone(&st.running);
    let cancel = std::sync::Arc::clone(&st.cancel);
    let result = std::sync::Arc::clone(&st.result);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        // Clears `running` however the worker ends: a panic here used to
        // wedge the flag true for the rest of the session.
        let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
        let refs: Vec<&octa::data::DataTable> = snapshots.iter().collect();
        let outcome = fuzzy_join(&refs, &steps, &cancel).map_err(|e| e.to_string());
        if let Ok(mut slot) = result.lock() {
            *slot = Some(outcome);
        }
        ctx.request_repaint();
    });
}

impl OctaApp {
    /// Drain a finished fuzzy join into a detached tab.
    pub(crate) fn drain_fuzzy_join(&mut self) {
        let Some(st) = &self.fuzzy_join_dialog else {
            return;
        };
        let outcome = match st.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => None,
        };
        let Some(outcome) = outcome else {
            return;
        };

        let result = match outcome {
            Ok(r) => r,
            Err(reason) => {
                // Keep the dialog open: the threshold or the columns are
                // usually what needs changing.
                self.status_message = Some((
                    octa::i18n::t("fuzzy_join.failed").replace("{error}", &reason),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        let matched: usize = result.steps.iter().map(|s| s.matched).sum();
        let ambiguous: usize = result.steps.iter().map(|s| s.ambiguous).sum();
        let rows = result.steps.first().map(|s| s.left_rows).unwrap_or(0);
        let mut note = octa::i18n::t("fuzzy_join.result")
            .replace("{matched}", &matched.to_string())
            .replace("{rows}", &rows.to_string())
            .replace("{ambiguous}", &ambiguous.to_string());
        if let Some(capped) = result.steps.iter().find(|s| s.capped) {
            note.push(' ');
            note.push_str(&octa::i18n::t("fuzzy_join.capped").replace(
                "{rows}",
                &capped.left_rows.max(capped.right_rows).to_string(),
            ));
        }

        let mut tab = super::super::state::TabState::new(self.settings.default_search_mode);
        tab.table = result.table;
        tab.custom_tab_label = Some(octa::i18n::t("fuzzy_join.tab_label"));
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.fuzzy_join_dialog = None;
        self.status_message = Some((note, std::time::Instant::now()));
    }
}

/// Open the dialog, or explain why it cannot open. Two tables with columns are
/// the minimum; saying so beats a menu entry that appears to do nothing.
pub(crate) fn open_fuzzy_join_dialog(app: &mut OctaApp) {
    let tabs = joinable_tabs(app);
    if tabs.len() < 2 {
        app.status_message = Some((
            octa::i18n::t("fuzzy_join.need_open"),
            std::time::Instant::now(),
        ));
        return;
    }
    app.fuzzy_join_dialog = Some(FuzzyJoinState::new(tabs[0].0, tabs[1].0));
}
