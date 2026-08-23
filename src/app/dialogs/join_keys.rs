//! "Join key finder" dialog: which columns of the open tabs would actually
//! join?
//!
//! Answers the question you have before opening the Join dialog on two
//! unfamiliar tables. The scoring is the pure `octa::data::join_keys` engine,
//! also behind the `suggest_join_keys` MCP tool; this is just the picker and
//! the ranked list, plus a button that hands a chosen pair to the existing
//! Join dialog rather than reimplementing the join.

use eframe::egui;
use egui::RichText;

use octa::data::join::{JoinOp, JoinType};
use octa::data::join_keys::{DEFAULT_SAMPLE_ROWS, KeyCandidate, suggest_keys};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::{JoinCondDraft, JoinState, OctaApp};

pub(crate) struct JoinKeysState {
    pub(crate) size: DialogSize,
    /// Tab indices to compare; at least two must be ticked.
    pub(crate) tabs: Vec<usize>,
    /// Sample size buffer (comma-tolerant text, empty = the default).
    pub(crate) sample_buf: String,
    /// Ranked candidates, recomputed on Scan. `None` = not scanned yet.
    pub(crate) candidates: Option<Vec<KeyCandidate>>,
}

impl Default for JoinKeysState {
    fn default() -> Self {
        Self {
            size: DialogSize::Normal,
            tabs: Vec::new(),
            sample_buf: DEFAULT_SAMPLE_ROWS.to_string(),
            candidates: None,
        }
    }
}

impl OctaApp {
    /// Entry from Analyse -> Join key finder...
    pub(crate) fn open_join_keys_dialog(&mut self) {
        if self.tabs.iter().filter(|t| t.table.col_count() > 0).count() < 2 {
            self.status_message = Some((t("joinkeys.need_two"), std::time::Instant::now()));
            return;
        }
        // Pre-tick the active tab and the first other one with columns, which
        // is the pairing the user almost always means.
        let mut ticked = vec![self.active_tab];
        if let Some(other) = (0..self.tabs.len())
            .find(|&i| i != self.active_tab && self.tabs[i].table.col_count() > 0)
        {
            ticked.push(other);
        }
        self.join_keys_dialog = Some(JoinKeysState {
            tabs: ticked,
            ..Default::default()
        });
    }
}

/// Short label for a tab, matching the Join dialog's.
fn tab_label(app: &OctaApp, idx: usize) -> String {
    app.tabs
        .get(idx)
        .map(|t| t.title_display())
        .unwrap_or_else(|| format!("#{}", idx + 1))
}

pub(crate) fn render_join_keys_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.join_keys_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut scan = false;
    let mut use_pair: Option<(usize, usize, usize, usize)> = None;
    let mut st = app.join_keys_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // Tabs worth offering: anything with columns.
    let candidates_tabs: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| (i, tab_label(app, i)))
        .collect();

    let dialog_id = egui::Id::new("octa_join_keys_dialog");
    let window = egui::Window::new("octa_join_keys")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(420.0)
            .min_width(400.0)
            .min_height(240.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("join_keys_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("joinkeys.title")).strong().size(16.0));
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

        egui::Panel::bottom("join_keys_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let ready = st.tabs.len() >= 2;
                    let btn = ui.add_enabled(ready, egui::Button::new(t("joinkeys.scan")));
                    if btn.clicked() {
                        scan = true;
                    }
                    if !ready {
                        btn.on_disabled_hover_text(t("joinkeys.need_two"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("joinkeys.body"));
            ui.add_space(6.0);

            ui.label(RichText::new(t("joinkeys.tables")).strong())
                .on_hover_text(t("joinkeys.tables_hint"));
            ui.horizontal_wrapped(|ui| {
                for (idx, label) in &candidates_tabs {
                    let mut on = st.tabs.contains(idx);
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            st.tabs.push(*idx);
                        } else {
                            st.tabs.retain(|i| i != idx);
                        }
                        // The old ranking described a different set of tables.
                        st.candidates = None;
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("joinkeys.sample"))
                    .on_hover_text(t("joinkeys.sample_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.sample_buf)
                        .desired_width(90.0)
                        .hint_text(DEFAULT_SAMPLE_ROWS.to_string()),
                )
                .on_hover_text(t("joinkeys.sample_hint"));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match &st.candidates {
                None => {
                    ui.weak(t("joinkeys.not_scanned"));
                }
                Some(list) if list.is_empty() => {
                    ui.label(t("joinkeys.no_candidates"));
                }
                Some(list) => {
                    egui::ScrollArea::vertical()
                        .id_salt("join_keys_results")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for k in list {
                                let (lt, lc) = k.left;
                                let (rt, rc) = k.right;
                                let (Some(ltab), Some(rtab)) =
                                    (st.tabs.get(lt).copied(), st.tabs.get(rt).copied())
                                else {
                                    continue;
                                };
                                let lname = app.tabs[ltab]
                                    .table
                                    .columns
                                    .get(lc)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_default();
                                let rname = app.tabs[rtab]
                                    .table
                                    .columns
                                    .get(rc)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "{}.{lname} -> {}.{rname}",
                                        tab_label(app, ltab),
                                        tab_label(app, rtab)
                                    ));
                                    ui.weak(
                                        t("joinkeys.stats")
                                            .replace(
                                                "{overlap}",
                                                &format!("{:.0}", k.overlap * 100.0),
                                            )
                                            .replace(
                                                "{distinct}",
                                                &format!(
                                                    "{:.0}",
                                                    k.left_distinct.max(k.right_distinct) * 100.0
                                                ),
                                            ),
                                    );
                                    // Both directions. Two candidates tie
                                    // whenever both tables number their rows
                                    // from 1, and only the count read from the
                                    // child side tells them apart - which side
                                    // that is, the finder cannot know.
                                    ui.weak(
                                        t("joinkeys.unmatched")
                                            .replace("{left}", &k.left_orphans.to_string())
                                            .replace("{lefttotal}", &k.left_values.to_string())
                                            .replace("{right}", &k.right_orphans.to_string())
                                            .replace("{righttotal}", &k.right_values.to_string()),
                                    )
                                    .on_hover_text(t("joinkeys.unmatched_hint"));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button(t("joinkeys.use_in_join"))
                                                .on_hover_text(t("joinkeys.use_in_join_hint"))
                                                .clicked()
                                            {
                                                use_pair = Some((ltab, lc, rtab, rc));
                                            }
                                        },
                                    );
                                });
                            }
                        });
                }
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if scan {
        let sample = st
            .sample_buf
            .trim()
            .replace([',', '_', '.'], "")
            .parse::<usize>()
            .unwrap_or(DEFAULT_SAMPLE_ROWS)
            .max(1);
        // Snapshot each side with edits applied, so the ranking describes what
        // is on screen rather than what was last saved.
        let snaps: Vec<octa::data::DataTable> = st
            .tabs
            .iter()
            .filter_map(|&i| app.tabs.get(i))
            .map(|tab| {
                let mut t = tab.table.clone();
                t.apply_edits();
                t
            })
            .collect();
        let refs: Vec<&octa::data::DataTable> = snaps.iter().collect();
        st.candidates = Some(suggest_keys(&refs, sample));
    }

    if let Some((ltab, lc, rtab, rc)) = use_pair {
        // Hand the pair to the Join dialog rather than joining here: one join
        // implementation, one place where its options live.
        app.join_dialog = Some(JoinState {
            left_tab: ltab,
            right_tab: rtab,
            conds: vec![JoinCondDraft {
                left_col: lc,
                op: JoinOp::Eq,
                right_col: rc,
            }],
            join_type: JoinType::Left,
            error: None,
            size: DialogSize::default(),
        });
        return; // this dialog closes; the Join dialog takes over
    }

    if !close {
        app.join_keys_dialog = Some(st);
    }
}
