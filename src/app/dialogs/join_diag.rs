//! "Join diagnostics" dialog: why do these two key columns not join?
//!
//! The sibling of `join_keys`, and deliberately built the same way: the pure
//! `octa::data::join_diag` engine does the work, this is the picker plus the
//! report, and the hand-off button prefills the existing Join dialog rather
//! than joining anything itself.
//!
//! Runs synchronously like `join_keys` does. The engine is sampled at
//! `DEFAULT_SAMPLE_ROWS` and builds a handful of hash sets, so a worker thread
//! would add machinery without buying responsiveness.

use eframe::egui;
use egui::RichText;

use octa::data::join::{JoinOp, JoinType};
use octa::data::join_diag::{JoinDiagnosis, diagnose};
use octa::data::join_keys::DEFAULT_SAMPLE_ROWS;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::{JoinCondDraft, JoinState, OctaApp};

pub(crate) struct JoinDiagState {
    pub(crate) size: DialogSize,
    pub(crate) left_tab: usize,
    pub(crate) left_col: usize,
    pub(crate) right_tab: usize,
    pub(crate) right_col: usize,
    /// Sample size buffer (comma-tolerant text, empty = the default).
    pub(crate) sample_buf: String,
    /// `None` until the user runs it.
    pub(crate) result: Option<JoinDiagnosis>,
}

impl Default for JoinDiagState {
    fn default() -> Self {
        Self {
            size: DialogSize::Normal,
            left_tab: 0,
            left_col: 0,
            right_tab: 0,
            right_col: 0,
            sample_buf: DEFAULT_SAMPLE_ROWS.to_string(),
            result: None,
        }
    }
}

impl OctaApp {
    /// Entry from Analyse -> Join diagnostics...
    pub(crate) fn open_join_diag_dialog(&mut self) {
        if self.tabs.iter().filter(|t| t.table.col_count() > 0).count() < 2 {
            self.status_message = Some((t("joindiag.need_two"), std::time::Instant::now()));
            return;
        }
        let right = (0..self.tabs.len())
            .find(|&i| i != self.active_tab && self.tabs[i].table.col_count() > 0)
            .unwrap_or(self.active_tab);
        self.join_diag_dialog = Some(JoinDiagState {
            left_tab: self.active_tab,
            right_tab: right,
            ..Default::default()
        });
    }
}

fn tab_label(app: &OctaApp, idx: usize) -> String {
    app.tabs
        .get(idx)
        .map(|t| t.title_display())
        .unwrap_or_else(|| format!("#{}", idx + 1))
}

/// Which side of the join a picker row describes. Carries its own widget id and
/// i18n keys so the three can never be mismatched at a call site, and keeps
/// `side_picker` under clippy's argument-count threshold.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn id(self) -> &'static str {
        match self {
            Side::Left => "jd_left",
            Side::Right => "jd_right",
        }
    }
    fn label(self) -> String {
        t(match self {
            Side::Left => "joindiag.left_side",
            Side::Right => "joindiag.right_side",
        })
    }
    fn hint(self) -> String {
        t(match self {
            Side::Left => "joindiag.left_side_hint",
            Side::Right => "joindiag.right_side_hint",
        })
    }
}

/// Tab + column pickers for one side. Returns true when either changed, so the
/// caller can drop a report that now describes different columns.
fn side_picker(
    ui: &mut egui::Ui,
    app: &OctaApp,
    side: Side,
    tabs: &[(usize, String)],
    tab_idx: &mut usize,
    col_idx: &mut usize,
) -> bool {
    let id = side.id();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(side.label()).strong())
            .on_hover_text(side.hint());
        egui::ComboBox::from_id_salt(format!("{id}_tab"))
            .selected_text(tab_label(app, *tab_idx))
            .show_ui(ui, |ui| {
                for (i, name) in tabs {
                    if ui.selectable_value(tab_idx, *i, name).changed() {
                        changed = true;
                    }
                }
            });

        let cols: Vec<String> = app
            .tabs
            .get(*tab_idx)
            .map(|t| t.table.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        if *col_idx >= cols.len() {
            *col_idx = 0;
        }
        egui::ComboBox::from_id_salt(format!("{id}_col"))
            .selected_text(cols.get(*col_idx).cloned().unwrap_or_default())
            .height(420.0)
            .show_ui(ui, |ui| {
                for (i, name) in cols.iter().enumerate() {
                    if ui.selectable_value(col_idx, i, name).changed() {
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn render_report(ui: &mut egui::Ui, d: &JoinDiagnosis) {
    egui::Grid::new("join_diag_counts")
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t("joindiag.left")).strong());
            ui.label(RichText::new(t("joindiag.right")).strong());
            ui.end_row();

            ui.label(t("joindiag.rows"));
            ui.label(d.left_rows.to_string());
            ui.label(d.right_rows.to_string());
            ui.end_row();

            ui.label(t("joindiag.distinct"));
            ui.label(d.distinct_left.to_string());
            ui.label(d.distinct_right.to_string());
            ui.end_row();

            ui.label(RichText::new(t("joindiag.matched")).strong());
            ui.label(RichText::new(d.matched_left.to_string()).strong());
            ui.label(RichText::new(d.matched_right.to_string()).strong());
            ui.end_row();
        });

    if d.capped {
        ui.add_space(4.0);
        ui.weak(t("joindiag.capped"));
    }

    ui.add_space(8.0);
    ui.label(RichText::new(t("joindiag.fixes")).strong());
    if d.fixes.is_empty() {
        ui.weak(t("joindiag.no_fixes"));
    } else {
        for f in &d.fixes {
            ui.label(format!(
                "{}  ({} {})",
                t(f.kind.i18n_key()),
                f.would_match,
                t("joindiag.fix_would_match")
            ));
        }
    }

    for (title, sample) in [
        (t("joindiag.unmatched_left"), &d.unmatched_left),
        (t("joindiag.unmatched_right"), &d.unmatched_right),
    ] {
        if sample.is_empty() {
            continue;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong());
        for v in sample {
            ui.weak(format!("  {v}"));
        }
    }
}

pub(crate) fn render_join_diag_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.join_diag_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut use_pair = false;
    let mut st = app.join_diag_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let tabs: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| (i, tab_label(app, i)))
        .collect();

    let dialog_id = egui::Id::new("octa_join_diag_dialog");
    let window = egui::Window::new("octa_join_diag")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(620.0)
            .default_height(480.0)
            .min_width(460.0)
            .min_height(260.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("join_diag_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(t("joindiag.title")).strong().size(16.0),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("join_diag_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(t("joindiag.run"))
                        .on_hover_text(t("joindiag.run_hint"))
                        .clicked()
                    {
                        run = true;
                    }
                    let has = st.result.is_some();
                    let btn = ui.add_enabled(has, egui::Button::new(t("joindiag.use_in_join")));
                    if btn.clicked() {
                        use_pair = true;
                    }
                    if has {
                        btn.on_hover_text(t("joindiag.use_in_join_hint"));
                    } else {
                        btn.on_disabled_hover_text(t("joindiag.use_in_join_disabled"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("joindiag.body"));
            ui.add_space(6.0);

            let mut changed = side_picker(
                ui,
                app,
                Side::Left,
                &tabs,
                &mut st.left_tab,
                &mut st.left_col,
            );
            changed |= side_picker(
                ui,
                app,
                Side::Right,
                &tabs,
                &mut st.right_tab,
                &mut st.right_col,
            );
            if changed {
                // The old report described different columns.
                st.result = None;
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("joindiag.sample"))
                    .on_hover_text(t("joindiag.sample_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.sample_buf)
                        .desired_width(90.0)
                        .hint_text(DEFAULT_SAMPLE_ROWS.to_string()),
                )
                .on_hover_text(t("joindiag.sample_hint"));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match &st.result {
                None => {
                    ui.weak(t("joindiag.not_run"));
                }
                Some(d) => {
                    egui::ScrollArea::vertical()
                        .id_salt("join_diag_report")
                        .auto_shrink([false, false])
                        .show(ui, |ui| render_report(ui, d));
                }
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        let sample = st
            .sample_buf
            .trim()
            .replace([',', '_', '.'], "")
            .parse::<usize>()
            .unwrap_or(DEFAULT_SAMPLE_ROWS)
            .max(1);
        // Snapshot with edits applied, so the report describes what is on
        // screen rather than what was last saved.
        let snap = |idx: usize| {
            let mut t = app.tabs[idx].table.clone();
            t.apply_edits();
            t
        };
        let left = snap(st.left_tab);
        let right = snap(st.right_tab);
        st.result = Some(diagnose(&left, st.left_col, &right, st.right_col, sample));
    }

    if use_pair {
        // Hand the pair to the Join dialog: one join implementation, one place
        // where its options live.
        app.join_dialog = Some(JoinState {
            left_tab: st.left_tab,
            right_tab: st.right_tab,
            conds: vec![JoinCondDraft {
                left_col: st.left_col,
                op: JoinOp::Eq,
                right_col: st.right_col,
            }],
            join_type: JoinType::Left,
            error: None,
            size: DialogSize::default(),
        });
        return; // this dialog closes; the Join dialog takes over
    }

    if !close {
        app.join_diag_dialog = Some(st);
    }
}
