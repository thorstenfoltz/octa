//! "Join diagnostics" dialog: why do these two key columns not join?
//!
//! The sibling of `join_keys`, and deliberately built the same way: the pure
//! `octa::data::join_diag` engine does the work, this is the picker plus the
//! report, and the hand-off button prefills the existing Join dialog rather
//! than joining anything itself.
//!
//! On loaded rows it runs synchronously: the engine is sampled at
//! `DEFAULT_SAMPLE_ROWS` and builds a handful of hash sets. On a live-database
//! tab that does not hold every row it runs on the server in a worker
//! (`octa::db::pushdown::join_diag`), and the fixes the engine's SQL cannot
//! spell are computed on the loaded rows and listed apart.

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::join::{JoinOp, JoinType};
use octa::data::join_diag::{FixKind, JoinDiagnosis, SuggestedFix, diagnose};
use octa::data::join_keys::DEFAULT_SAMPLE_ROWS;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::pushdown::{DialogNote, ServerTask};
use super::super::state::{JoinCondDraft, JoinState, OctaApp};

/// What the server run hands back: its diagnosis, the fixes it could not
/// check, and those fixes computed on the loaded rows.
pub(crate) struct DiagOut {
    pub(crate) diag: JoinDiagnosis,
    pub(crate) not_checked: Vec<FixKind>,
    pub(crate) local_fixes: Vec<SuggestedFix>,
}

/// The loaded-row fixes of just `kinds` (the ones the engine cannot spell).
fn local_fixes_for(
    left: &DataTable,
    lcol: usize,
    right: &DataTable,
    rcol: usize,
    sample: usize,
    kinds: &[FixKind],
) -> Vec<SuggestedFix> {
    if kinds.is_empty() {
        return Vec::new();
    }
    diagnose(left, lcol, right, rcol, sample)
        .fixes
        .into_iter()
        .filter(|f| kinds.contains(&f.kind))
        .collect()
}

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
    /// The diagnosis running on the server.
    pub(crate) server: Option<ServerTask<DiagOut>>,
    pub(crate) note: Option<DialogNote>,
    pub(crate) server_error: Option<String>,
    /// Fixes the server could not check, computed on the loaded rows.
    pub(crate) local_fixes: Vec<SuggestedFix>,
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
            server: None,
            note: None,
            server_error: None,
            local_fixes: Vec::new(),
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
    // One grid row, not a `horizontal`: the two sides are rendered as two rows
    // of the SAME grid, so the combo boxes line up under each other however
    // wide "Left"/"Right" happen to be - which differs per locale.
    {
        let ui = &mut *ui;
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
        ui.end_row();
    }
    changed
}

/// `has_local`: a loaded-row fix list follows, so "no fixes" would mislead.
fn render_report(ui: &mut egui::Ui, d: &JoinDiagnosis, has_local: bool) {
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
        if !has_local {
            ui.weak(t("joindiag.no_fixes"));
        }
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
    use crate::app::pushdown::{TaskPoll, mixed_sources, server_sources_for};
    match st.server.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Pending) => ctx.request_repaint(),
        Some(TaskPoll::Ready(out)) => {
            st.server = None;
            st.result = Some(out.diag);
            st.local_fixes = out.local_fixes;
            if let Some(n) = st.note.as_mut() {
                n.local = out.not_checked.iter().map(|k| t(k.i18n_key())).collect();
            }
        }
        Some(TaskPoll::Cancelled) => {
            st.server = None;
            st.note = None;
            st.server_error = Some(t("pushdown.cancelled"));
        }
        Some(TaskPoll::Failed(e)) => {
            st.server = None;
            st.note = None;
            st.server_error = Some(e);
        }
        None => {}
    }
    let (on_server, mixed, loaded) = {
        let picked: Vec<&crate::app::state::TabState> = [st.left_tab, st.right_tab]
            .iter()
            .filter_map(|&i| app.tabs.get(i))
            .collect();
        let (on, conns) = (app.settings.db_pushdown, &app.settings.db_connections);
        (
            server_sources_for(&picked, on, conns).is_some(),
            mixed_sources(&picked, on, conns),
            // A self-join lists its tab twice; count its rows once.
            picked
                .iter()
                .take(if st.left_tab == st.right_tab { 1 } else { 2 })
                .map(|t| t.table.rows.len())
                .sum::<usize>(),
        )
    };
    let running = st.server.is_some();
    let mut run_local = false;
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
                    let btn = ui.add_enabled(!running, egui::Button::new(t("joindiag.run")));
                    if btn.clicked() {
                        run = true;
                    }
                    if running {
                        btn.on_disabled_hover_text(t("pushdown.running"));
                    } else {
                        btn.on_hover_text(t("joindiag.run_hint"));
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

            // Both sides in one grid: three columns (label, table, column),
            // so the pickers sit in a straight line instead of starting
            // wherever the label happens to end.
            let mut changed = false;
            egui::Grid::new("join_diag_sides")
                .num_columns(3)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    changed = side_picker(
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
                });
            if changed {
                // The old report described different columns.
                st.result = None;
                st.note = None;
                st.server_error = None;
                st.local_fixes.clear();
                st.server = None;
            }

            ui.add_space(6.0);
            octa::ui::control_row::control_row(ui, |ui| {
                ui.label(t("joindiag.sample"))
                    .on_hover_text(t("joindiag.sample_hint"));
                ui.add_enabled(
                    !on_server,
                    egui::TextEdit::singleline(&mut st.sample_buf)
                        .desired_width(90.0)
                        .hint_text(DEFAULT_SAMPLE_ROWS.to_string()),
                )
                .on_hover_text(t("joindiag.sample_hint"))
                .on_disabled_hover_text(t("pushdown.sample_server_hint"));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match crate::app::pushdown::server_status_ui(
                ui,
                st.server.is_some(),
                st.server_error.as_deref(),
                true,
            ) {
                crate::app::pushdown::ServerUi::Cancel => {
                    // Say so at once: Oracle cannot cancel one running
                    // statement, so the worker may take a while to stop.
                    // Dropping the task cancels the statement.
                    st.server = None;
                    st.note = None;
                    st.server_error = Some(t("pushdown.cancelled"));
                }
                crate::app::pushdown::ServerUi::RunLocal => run_local = true,
                crate::app::pushdown::ServerUi::Idle => {}
            }
            if let Some(note) = &st.note
                && st.result.is_some()
            {
                crate::app::pushdown::dialog_note_ui(ui, note);
            } else if mixed {
                octa::ui::message::partial_note_label(ui, &t("pushdown.mixed_sources"));
            } else if on_server && st.result.is_some() {
                // Run on loaded rows: say how many.
                octa::ui::message::partial_note(ui, loaded, None);
            }

            match &st.result {
                None => {
                    ui.weak(t("joindiag.not_run"));
                }
                Some(d) => {
                    egui::ScrollArea::vertical()
                        .id_salt("join_diag_report")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            render_report(ui, d, !st.local_fixes.is_empty());
                            if !st.local_fixes.is_empty() {
                                ui.add_space(8.0);
                                ui.label(RichText::new(t("pushdown.local_fixes")).strong());
                                for f in &st.local_fixes {
                                    ui.label(format!(
                                        "{}  ({} {})",
                                        t(f.kind.i18n_key()),
                                        f.would_match,
                                        t("joindiag.fix_would_match")
                                    ));
                                }
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

    if run || run_local {
        st.server_error = None;
        st.note = None;
        st.local_fixes.clear();
        // The field is greyed out while the tabs are on the database: what
        // runs on the loaded rows then uses every one of them.
        let sample = if on_server {
            usize::MAX
        } else {
            st.sample_buf
                .trim()
                .replace([',', '_', '.'], "")
                .parse::<usize>()
                .unwrap_or(DEFAULT_SAMPLE_ROWS)
                .max(1)
        };
        // Snapshot with edits applied, so the report describes what is on
        // screen rather than what was last saved.
        let snap = |idx: usize| {
            let mut t = app.tabs[idx].table.clone();
            t.apply_edits();
            t
        };
        let (l, r) = (&app.tabs[st.left_tab], &app.tabs[st.right_tab]);
        let srcs = (!run_local)
            .then(|| {
                server_sources_for(
                    &[l, r],
                    app.settings.db_pushdown,
                    &app.settings.db_connections,
                )
            })
            .flatten();
        let names = (
            l.table.columns.get(st.left_col).map(|c| c.name.clone()),
            r.table.columns.get(st.right_col).map(|c| c.name.clone()),
        );
        let unsaved = l.table.is_modified() || r.table.is_modified();
        let loaded = if st.left_tab == st.right_tab {
            l.table.rows.len()
        } else {
            l.table.rows.len() + r.table.rows.len()
        };
        if let (Some(mut srcs), (Some(lc), Some(rc))) = (srcs, names) {
            let right = srcs.pop().expect("two sources");
            let left = srcs.pop().expect("two sources");
            st.note = Some(DialogNote {
                unsaved,
                loaded,
                engine: Some(left.engine()),
                ..Default::default()
            });
            st.result = None;
            // Clone the loaded rows only when the engine leaves fixes to them.
            let snaps = (!octa::db::pushdown::join_diag::unchecked_fixes(left.engine()).is_empty())
                .then(|| (snap(st.left_tab), snap(st.right_tab)));
            let (lci, rci) = (st.left_col, st.right_col);
            let conn = left.conn.clone();
            st.server = Some(
                app.spawn_server_task(conn, t("joindiag.title"), move |c, stop| {
                    let (diag, not_checked) =
                        octa::db::pushdown::join_diag::run(c, &left, &lc, &right, &rc, stop)?;
                    let local_fixes = snaps
                        .as_ref()
                        .map(|(ls, rs)| local_fixes_for(ls, lci, rs, rci, sample, &not_checked))
                        .unwrap_or_default();
                    Ok(DiagOut {
                        diag,
                        not_checked,
                        local_fixes,
                    })
                }),
            );
        } else {
            let left = snap(st.left_tab);
            let right = snap(st.right_tab);
            st.result = Some(diagnose(&left, st.left_col, &right, st.right_col, sample));
        }
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
            spatial: None,
            error: None,
            size: DialogSize::default(),
        });
        return; // this dialog closes; the Join dialog takes over
    }

    if !close {
        app.join_diag_dialog = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use octa::data::join_diag::FixKind;
    use octa::data::{CellValue, ColumnInfo, DataTable};

    fn col(vals: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vals
            .iter()
            .map(|v| vec![CellValue::String((*v).into())])
            .collect();
        t
    }

    /// Only the kinds the server could not check are taken from the loaded
    /// rows, so the two lists never repeat a fix.
    #[test]
    fn local_fixes_keep_only_the_unchecked_kinds() {
        let l = col(&["a  b", "x-1", " c"]);
        let r = col(&["a b", "x 1", "c"]);
        let got = super::local_fixes_for(&l, 0, &r, 0, usize::MAX, &[FixKind::StripPunctuation]);
        assert_eq!(
            got.iter().map(|f| f.kind).collect::<Vec<_>>(),
            [FixKind::StripPunctuation]
        );
        assert!(super::local_fixes_for(&l, 0, &r, 0, usize::MAX, &[]).is_empty());
    }
}
