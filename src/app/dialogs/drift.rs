//! "Data drift" dialog: how did the same dataset move between two versions?
//!
//! Computes nothing itself. Both sides are resolved to a `DataTable` (an open
//! tab's snapshot, or a file read through the normal registry) and handed to
//! `octa::data::drift::compare_profiles`, the same engine behind
//! `--drift-report` and the `data_drift` tool, so the three surfaces cannot
//! give different answers. Reading a file is IO, so the work runs on a worker
//! thread with a polled result slot, exactly like the database compare dialog
//! next door.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::drift::{DEFAULT_CATEGORY_CAP, DriftOptions, compare_profiles, report_table};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

/// What the worker hands back: the ready-made report table plus whether either
/// side sat on the row cap, which would make every count partial.
pub(crate) struct DriftOutcome {
    pub(crate) table: DataTable,
    pub(crate) capped: bool,
}

type DriftSlot = Arc<Mutex<Option<Result<DriftOutcome, String>>>>;

/// Where one side of the comparison comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DriftSource {
    /// Index into `OctaApp::tabs`.
    Tab(usize),
    /// A file on disk, `None` until the user picks one.
    File(Option<PathBuf>),
}

impl DriftSource {
    fn is_file(&self) -> bool {
        matches!(self, DriftSource::File(_))
    }

    /// Whether this side names something that can actually be read.
    fn is_ready(&self, app: &OctaApp) -> bool {
        match self {
            DriftSource::Tab(i) => app
                .tabs
                .get(*i)
                .is_some_and(|tab| tab.table.col_count() > 0),
            DriftSource::File(p) => p.is_some(),
        }
    }
}

/// A side resolved on the UI thread, ready to move into the worker. Tabs are
/// snapshotted here because `TabState` never crosses a thread boundary.
enum ResolvedSide {
    Table(Box<DataTable>),
    File(PathBuf),
}

pub(crate) struct DriftState {
    pub(crate) size: DialogSize,
    pub(crate) side_a: DriftSource,
    pub(crate) side_b: DriftSource,
    /// Free text so a half-typed number does not snap back to the default.
    pub(crate) category_cap: String,
    /// In-flight worker slot; `Some` while a comparison runs.
    pub(crate) job: Option<DriftSlot>,
    pub(crate) result_msg: Option<(bool, String)>,
}

impl OctaApp {
    /// Entry from the Analyse menu and the shortcut.
    pub(crate) fn open_drift_dialog(&mut self) {
        // Default to the active tab against the next tab that has columns, so
        // the common case (two versions open side by side) needs no clicks.
        let side_a = DriftSource::Tab(self.active_tab);
        let other = (0..self.tabs.len())
            .find(|&i| i != self.active_tab && self.tabs[i].table.col_count() > 0);
        let side_b = match other {
            Some(i) => DriftSource::Tab(i),
            None => DriftSource::File(None),
        };
        self.drift_dialog = Some(DriftState {
            size: DialogSize::Normal,
            side_a,
            side_b,
            category_cap: DEFAULT_CATEGORY_CAP.to_string(),
            job: None,
            result_msg: None,
        });
    }

    /// Snapshot a tab (edits applied, so the comparison reflects what the user
    /// sees) or pass a path straight through.
    fn resolve_drift_side(&self, side: &DriftSource) -> Option<ResolvedSide> {
        match side {
            DriftSource::Tab(i) => {
                let tab = self.tabs.get(*i)?;
                let mut snap = tab.table.clone();
                snap.apply_edits();
                Some(ResolvedSide::Table(Box::new(snap)))
            }
            DriftSource::File(p) => p.clone().map(ResolvedSide::File),
        }
    }

    fn spawn_drift(&self, st: &mut DriftState, ctx: &egui::Context) {
        let (Some(a), Some(b)) = (
            self.resolve_drift_side(&st.side_a),
            self.resolve_drift_side(&st.side_b),
        ) else {
            st.result_msg = Some((false, t("datadrift.need_both")));
            return;
        };

        let opts = DriftOptions {
            category_cap: st
                .category_cap
                .trim()
                .parse()
                .unwrap_or(DEFAULT_CATEGORY_CAP),
            thresholds: Vec::new(),
        };
        let cap_bytes = if self.settings.max_decompressed_unlimited {
            u64::MAX
        } else {
            self.settings.max_decompressed_bytes
        };

        let slot: DriftSlot = Arc::new(Mutex::new(None));
        st.job = Some(slot.clone());
        st.result_msg = None;
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<DriftOutcome> {
                let read = |side: ResolvedSide| -> anyhow::Result<DataTable> {
                    match side {
                        ResolvedSide::Table(t) => Ok(*t),
                        ResolvedSide::File(p) => {
                            Ok(octa::formats::read_table_auto(&p, None, cap_bytes)?)
                        }
                    }
                };
                let a = read(a)?;
                let b = read(b)?;
                let row_cap = octa::formats::initial_load_rows();
                let capped = a.row_count() >= row_cap || b.row_count() >= row_cap;
                let report = compare_profiles(&a, &b, &opts)?;
                Ok(DriftOutcome {
                    table: report_table(&report),
                    capped,
                })
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Open a finished comparison in a detached tab, the way Summary does.
    fn open_drift_result_tab(&mut self, outcome: DriftOutcome) {
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::super::state::TabState::new(default_search_mode);
        new_tab.table = outcome.table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(t("datadrift.tab_label"));
        // Column headers are machine-readable ids (`null_rate`, not "Null
        // rate"), which is right for export and useless for reading. The
        // tooltips carry the explanation, the same way the Summary tab does.
        new_tab.table_state.header_tooltips = [
            "datadrift.col_column",
            "datadrift.col_metric",
            "datadrift.col_before",
            "datadrift.col_after",
            "datadrift.col_change",
            "datadrift.col_breached",
        ]
        .iter()
        .map(|k| t(k))
        .collect();
        if outcome.capped {
            new_tab.parse_error_banner = Some(t("datadrift.capped"));
        }
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }
}

/// One side's radio pair plus its picker. Returns a path to open when the
/// user pressed the File button (the picker itself runs in the caller so the
/// borrow of `app` stays out of this closure-heavy body).
fn side_row(
    ui: &mut egui::Ui,
    id: &str,
    label_key: &str,
    hint_key: &str,
    side: &mut DriftSource,
    tabs: &[(usize, String)],
) -> bool {
    let mut pick = false;
    ui.label(RichText::new(t(label_key)).strong())
        .on_hover_text(t(hint_key));
    ui.horizontal(|ui| {
        let mut is_file = side.is_file();
        if ui
            .radio_value(&mut is_file, false, t("datadrift.source_tab"))
            .on_hover_text(t("datadrift.source_tab_hint"))
            .clicked()
            && side.is_file()
        {
            *side = DriftSource::Tab(tabs.first().map(|(i, _)| *i).unwrap_or(0));
        }
        if ui
            .radio_value(&mut is_file, true, t("datadrift.source_file"))
            .on_hover_text(t("datadrift.source_file_hint"))
            .clicked()
            && !side.is_file()
        {
            *side = DriftSource::File(None);
        }
    });

    match side {
        DriftSource::Tab(sel) => {
            let selected = tabs
                .iter()
                .find(|(i, _)| i == sel)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt(format!("drift_tab_{id}"))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (i, name) in tabs {
                        ui.selectable_value(sel, *i, name);
                    }
                })
                .response
                .on_hover_text(t("datadrift.source_tab_hint"));
        }
        DriftSource::File(path) => {
            ui.horizontal(|ui| {
                if ui
                    .button(t("datadrift.source_file"))
                    .on_hover_text(t("datadrift.source_file_hint"))
                    .clicked()
                {
                    pick = true;
                }
                if let Some(p) = path {
                    ui.label(
                        RichText::new(p.file_name().unwrap_or_default().to_string_lossy())
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            });
        }
    }
    pick
}

pub(crate) fn render_drift_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.drift_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut pick_a = false;
    let mut pick_b = false;
    let mut st = app.drift_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // Drain a finished worker: success opens a tab and closes the dialog,
    // failure stays put with the message so the user can fix the input.
    let mut finished: Option<DriftOutcome> = None;
    if let Some(slot) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.job = None;
        match res {
            Ok(outcome) => finished = Some(outcome),
            Err(e) => st.result_msg = Some((false, e)),
        }
    }
    let running = st.job.is_some();

    let tabs: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| {
            (
                i,
                app.tabs
                    .get(i)
                    .map(|t| t.title_display())
                    .unwrap_or_else(|| format!("#{}", i + 1)),
            )
        })
        .collect();
    let ready = st.side_a.is_ready(app) && st.side_b.is_ready(app);

    let dialog_id = egui::Id::new("octa_drift_dialog");
    let window = egui::Window::new("octa_drift")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(320.0)
            .min_width(360.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("drift_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("datadrift.title")).strong().size(16.0));
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

        egui::Panel::bottom("drift_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(t("datadrift.running"));
                    } else {
                        let btn = ui.add_enabled(ready, egui::Button::new(t("datadrift.compare")));
                        if btn.clicked() {
                            run = true;
                        }
                        if ready {
                            btn.on_hover_text(t("datadrift.compare_hint"));
                        } else {
                            btn.on_disabled_hover_text(t("datadrift.need_both"));
                            ui.label(
                                RichText::new(t("datadrift.need_both"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("datadrift.hint"));
            ui.add_space(8.0);

            pick_a = side_row(
                ui,
                "a",
                "datadrift.side_a",
                "datadrift.side_a_hint",
                &mut st.side_a,
                &tabs,
            );
            ui.add_space(8.0);
            pick_b = side_row(
                ui,
                "b",
                "datadrift.side_b",
                "datadrift.side_b_hint",
                &mut st.side_b,
                &tabs,
            );

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(t("datadrift.category_cap"))
                    .on_hover_text(t("datadrift.category_cap_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.category_cap)
                        .desired_width(70.0)
                        .hint_text(DEFAULT_CATEGORY_CAP.to_string()),
                )
                .on_hover_text(t("datadrift.category_cap_hint"));
            });

            if let Some((ok, msg)) = &st.result_msg {
                ui.add_space(8.0);
                draw_result_message(ui, *ok, msg);
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if pick_a && let Some(p) = rfd::FileDialog::new().pick_file() {
        st.side_a = DriftSource::File(Some(p));
    }
    if pick_b && let Some(p) = rfd::FileDialog::new().pick_file() {
        st.side_b = DriftSource::File(Some(p));
    }
    if run {
        app.spawn_drift(&mut st, ctx);
    }

    if let Some(outcome) = finished {
        app.open_drift_result_tab(outcome);
        return; // dialog closes; the result is the tab
    }
    if !close {
        app.drift_dialog = Some(st);
    }
}
