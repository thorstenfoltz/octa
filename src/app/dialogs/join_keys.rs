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
use octa::db::pushdown::ServerSource;
use octa::db::pushdown::row_estimate::RowCount;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::pushdown::{DialogNote, ServerTask};
use super::super::state::{JoinCondDraft, JoinState, OctaApp};

pub(crate) struct JoinKeysState {
    pub(crate) size: DialogSize,
    /// Tab indices to compare; at least two must be ticked.
    pub(crate) tabs: Vec<usize>,
    /// Sample size buffer (comma-tolerant text, empty = the default).
    pub(crate) sample_buf: String,
    /// Ranked candidates, recomputed on Scan. `None` = not scanned yet.
    pub(crate) candidates: Option<Vec<KeyCandidate>>,
    /// Row counts for the cost question, before a server run.
    pub(crate) counting: Option<ServerTask<Vec<RowCount>>>,
    /// The tables the cost question is about.
    pub(crate) pending: Option<Vec<(ServerSource, Vec<octa::data::ColumnInfo>)>>,
    /// The cost question's lines, shown until the user picks.
    pub(crate) cost: Option<Vec<String>>,
    /// The ranking running on the server.
    pub(crate) server: Option<ServerTask<Vec<KeyCandidate>>>,
    /// The ranking over every loaded row of database tabs, on a worker.
    pub(crate) local: Option<ServerTask<Vec<KeyCandidate>>>,
    pub(crate) note: Option<DialogNote>,
    pub(crate) server_error: Option<String>,
}

#[derive(Clone, Copy)]
enum Choice {
    All,
    Likely,
    Loaded,
    Dismiss,
}

impl Default for JoinKeysState {
    fn default() -> Self {
        Self {
            size: DialogSize::Normal,
            tabs: Vec::new(),
            sample_buf: DEFAULT_SAMPLE_ROWS.to_string(),
            candidates: None,
            counting: None,
            pending: None,
            cost: None,
            server: None,
            local: None,
            note: None,
            server_error: None,
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

/// The cost question's lines for these tables (labels and column counts in
/// tab order) and their row counts.
fn question_lines(labels: &[String], widths: &[usize], counts: &[RowCount]) -> Vec<String> {
    let tables: Vec<(String, RowCount)> =
        labels.iter().cloned().zip(counts.iter().copied()).collect();
    crate::app::pushdown::cost_lines(&tables, crate::app::pushdown::column_pairs(widths))
}

/// The ticked tabs with their edits applied, so the ranking describes what
/// is on screen rather than what was last saved.
fn snapshots(app: &OctaApp, st: &JoinKeysState) -> Vec<octa::data::DataTable> {
    st.tabs
        .iter()
        .filter_map(|&i| app.tabs.get(i))
        .map(|tab| {
            let mut t = tab.table.clone();
            t.apply_edits();
            t
        })
        .collect()
}

fn rank(snaps: &[octa::data::DataTable], sample: usize) -> Vec<KeyCandidate> {
    let refs: Vec<&octa::data::DataTable> = snaps.iter().collect();
    suggest_keys(&refs, sample)
}

/// Rank on the loaded rows. File tabs read the sample and stay instant.
/// With the tables on the database the sample field is greyed out and every
/// loaded row is used, which takes seconds on big pages, so that runs on a
/// worker and lands in `candidates` when it is done.
fn start_local(app: &OctaApp, st: &mut JoinKeysState, on_server: bool) {
    let snaps = snapshots(app, st);
    if on_server {
        st.candidates = None;
        st.local = Some(crate::app::pushdown::spawn_local_task(move || {
            rank(&snaps, usize::MAX)
        }));
    } else {
        let sample = st
            .sample_buf
            .trim()
            .replace([',', '_', '.'], "")
            .parse::<usize>()
            .unwrap_or(DEFAULT_SAMPLE_ROWS)
            .max(1);
        st.candidates = Some(rank(&snaps, sample));
    }
}

pub(crate) fn render_join_keys_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.join_keys_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut scan = false;
    let mut use_pair: Option<(usize, usize, usize, usize)> = None;
    let mut st = app.join_keys_dialog.take().unwrap();
    use crate::app::pushdown::{TaskPoll, mixed_sources, server_sources_for};
    // The row counts for the question.
    match st.counting.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Pending) => ctx.request_repaint(),
        Some(TaskPoll::Ready(counts)) => {
            st.counting = None;
            if let Some(tables) = &st.pending {
                let labels: Vec<String> = st.tabs.iter().map(|&i| tab_label(app, i)).collect();
                let widths: Vec<usize> = tables.iter().map(|(_, cols)| cols.len()).collect();
                st.cost = Some(question_lines(&labels, &widths, &counts));
            }
        }
        Some(TaskPoll::Cancelled) => {
            st.counting = None;
            st.pending = None;
            st.note = None;
            st.server_error = Some(t("pushdown.cancelled"));
        }
        Some(TaskPoll::Failed(e)) => {
            st.counting = None;
            st.pending = None;
            st.note = None;
            st.server_error = Some(e);
        }
        None => {}
    }
    // The ranking itself.
    match st.server.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Pending) => ctx.request_repaint(),
        Some(TaskPoll::Ready(list)) => {
            st.candidates = Some(list);
            st.server = None;
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
    match st.local.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Pending) => ctx.request_repaint(),
        Some(TaskPoll::Ready(list)) => {
            st.candidates = Some(list);
            st.local = None;
        }
        Some(TaskPoll::Cancelled | TaskPoll::Failed(_)) => {
            st.local = None;
            st.server_error = Some(t("pushdown.stopped"));
        }
        None => {}
    }
    let picked: Vec<&crate::app::state::TabState> =
        st.tabs.iter().filter_map(|&i| app.tabs.get(i)).collect();
    let on_server = server_sources_for(
        &picked,
        app.settings.db_pushdown,
        &app.settings.db_connections,
    )
    .is_some();
    let mixed = mixed_sources(
        &picked,
        app.settings.db_pushdown,
        &app.settings.db_connections,
    );
    let loaded: usize = picked.iter().map(|t| t.table.rows.len()).sum();
    drop(picked);
    let running = st.server.is_some() || st.counting.is_some() || st.local.is_some();
    let mut choice: Option<Choice> = None;
    let mut run_local = false;
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
                    let btn = ui.add_enabled(
                        ready && !running && st.cost.is_none(),
                        egui::Button::new(t("joinkeys.scan")),
                    );
                    if btn.clicked() {
                        scan = true;
                    }
                    if running {
                        btn.on_disabled_hover_text(t("pushdown.running"));
                    } else if !ready {
                        btn.on_disabled_hover_text(t("joinkeys.need_two"));
                    } else if st.cost.is_some() {
                        btn.on_disabled_hover_text(t("pushdown.keys_cost_question"));
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
                        st.note = None;
                        st.server_error = None;
                        st.server = None;
                        st.counting = None;
                        st.local = None;
                        st.pending = None;
                        st.cost = None;
                    }
                }
            });

            ui.add_space(6.0);
            octa::ui::control_row::control_row(ui, |ui| {
                ui.label(t("joinkeys.sample"))
                    .on_hover_text(t("joinkeys.sample_hint"));
                ui.add_enabled(
                    !on_server,
                    egui::TextEdit::singleline(&mut st.sample_buf)
                        .desired_width(90.0)
                        .hint_text(DEFAULT_SAMPLE_ROWS.to_string()),
                )
                .on_hover_text(t("joinkeys.sample_hint"))
                .on_disabled_hover_text(t("pushdown.sample_server_hint"));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            if st.counting.is_some() {
                octa::ui::control_row::control_row(ui, |ui| {
                    ui.spinner();
                    ui.label(t("pushdown.keys_counting"));
                });
            }
            if let Some(lines) = &st.cost {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(t("pushdown.keys_cost_title")).strong());
                    for line in lines {
                        ui.label(line);
                    }
                    ui.label(t("pushdown.keys_cost_question"));
                    octa::ui::control_row::control_row(ui, |ui| {
                        for (label, hint, c) in [
                            ("pushdown.keys_all", "pushdown.keys_all_hint", Choice::All),
                            (
                                "pushdown.keys_likely",
                                "pushdown.keys_likely_hint",
                                Choice::Likely,
                            ),
                            (
                                "pushdown.keys_loaded",
                                "pushdown.keys_loaded_hint",
                                Choice::Loaded,
                            ),
                            (
                                "common.cancel",
                                "pushdown.prompt_cancel_hint",
                                Choice::Dismiss,
                            ),
                        ] {
                            if ui.button(t(label)).on_hover_text(t(hint)).clicked() {
                                choice = Some(c);
                            }
                        }
                    });
                });
            }
            if st.local.is_some() {
                octa::ui::control_row::control_row(ui, |ui| {
                    ui.spinner();
                    ui.label(t("joinkeys.scanning_loaded"));
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("joinkeys.cancel_scan_hint"))
                        .clicked()
                    {
                        // Dropping the task discards its result; the worker
                        // finishes on its own.
                        st.local = None;
                    }
                });
            }
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
                && st.candidates.is_some()
            {
                crate::app::pushdown::dialog_note_ui(ui, note);
            } else if mixed {
                octa::ui::message::partial_note_label(ui, &t("pushdown.mixed_sources"));
            } else if on_server && st.candidates.is_some() {
                // Loaded rows only, or Run on loaded rows: say how many.
                octa::ui::message::partial_note(ui, loaded, None);
            }

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
        st.server_error = None;
        st.note = None;
        st.candidates = None;
        st.local = None;
        let picked: Vec<&crate::app::state::TabState> =
            st.tabs.iter().filter_map(|&i| app.tabs.get(i)).collect();
        match server_sources_for(
            &picked,
            app.settings.db_pushdown,
            &app.settings.db_connections,
        ) {
            Some(srcs) => {
                let tables: Vec<(ServerSource, Vec<octa::data::ColumnInfo>)> = srcs
                    .into_iter()
                    .zip(picked.iter().map(|t| t.table.columns.clone()))
                    .collect();
                let unsaved = picked.iter().any(|t| t.table.is_modified());
                drop(picked);
                let conn = tables[0].0.conn.clone();
                let srcs: Vec<ServerSource> = tables.iter().map(|(s, _)| s.clone()).collect();
                st.counting =
                    Some(
                        app.spawn_server_task(conn, t("joinkeys.title"), move |c, stop| {
                            srcs.iter()
                                .map(|s| octa::db::pushdown::row_estimate::row_count(c, s, stop))
                                .collect()
                        }),
                    );
                st.pending = Some(tables);
                st.note = Some(DialogNote {
                    unsaved,
                    ..Default::default()
                });
            }
            None => {
                drop(picked);
                start_local(app, &mut st, on_server);
            }
        }
    }
    if run_local {
        st.server_error = None;
        st.note = None;
        start_local(app, &mut st, on_server);
    }
    match choice {
        Some(c @ (Choice::All | Choice::Likely)) => {
            st.cost = None;
            if let Some(tables) = st.pending.take() {
                let conn = tables[0].0.conn.clone();
                if matches!(c, Choice::All) {
                    st.server = Some(app.spawn_server_task(
                        conn,
                        t("joinkeys.title"),
                        move |c, stop| octa::db::pushdown::join_keys::run(c, &tables, stop),
                    ));
                } else {
                    // The candidates come from every loaded row, so they are
                    // found on the worker too, before the server checks them.
                    let snaps = snapshots(app, &st);
                    if let Some(n) = st.note.as_mut() {
                        n.rescored = true;
                    }
                    st.server =
                        Some(
                            app.spawn_server_task(conn, t("joinkeys.title"), move |c, stop| {
                                let pairs: Vec<octa::db::pushdown::join_keys::ColPair> =
                                    rank(&snaps, usize::MAX)
                                        .iter()
                                        .map(|k| (k.left, k.right))
                                        .collect();
                                octa::db::pushdown::join_keys::rescore(c, &tables, &pairs, stop)
                            }),
                        );
                }
            }
        }
        Some(Choice::Loaded) => {
            st.cost = None;
            st.pending = None;
            st.note = None;
            start_local(app, &mut st, on_server);
        }
        Some(Choice::Dismiss) => {
            st.cost = None;
            st.pending = None;
            st.note = None;
        }
        None => {}
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
            spatial: None,
            error: None,
            size: DialogSize::default(),
        });
        return; // this dialog closes; the Join dialog takes over
    }

    if !close {
        app.join_keys_dialog = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use octa::db::pushdown::row_estimate::RowCount;

    #[test]
    fn the_question_counts_pairs_from_the_tab_columns() {
        let widths = [3, 2];
        let lines = super::question_lines(
            &["orders".to_string(), "customers".to_string()],
            &widths,
            &[
                RowCount {
                    rows: 10,
                    estimate: false,
                },
                RowCount {
                    rows: 5,
                    estimate: true,
                },
            ],
        );
        assert_eq!(lines.len(), 4);
        assert!(lines[2].contains('6'), "{}", lines[2]);
    }
}
