//! File -> Earlier versions on a Delta Lake or Apache Iceberg tab: the
//! table's own history instead of kept copies. `file_versions.rs` owns the
//! window and hands this file the footer and the list.
//!
//! Both compares run inside DuckDB over the two complete versions
//! (`lakehouse_reader::diff_versions`), because the open tab holds at most
//! the row cap and the Compare view would call every row it never loaded
//! "missing".

use std::path::Path;

use eframe::egui;
use egui::RichText;

use octa::formats::lakehouse_history::{History, LIST_LIMIT, TableVersion, VersionId};
use octa::formats::lakehouse_reader::{self, LakehouseKind};
use octa::i18n::t;

use super::file_versions::FileVersionsState;
use crate::app::state::OctaApp;

pub(crate) struct LakeVersions {
    pub(crate) kind: LakehouseKind,
    /// The history, or why there is none (shown selectable).
    pub(crate) history: Result<History, String>,
    /// The running comparison, if any.
    pub(crate) job: Option<DiffJob>,
}

/// A comparison running on a worker thread.
pub(crate) struct DiffJob {
    /// Labels of the two versions, for the result tab.
    pub(crate) older: String,
    pub(crate) newer: String,
    /// Stops the DuckDB query (Cancel, or closing the dialog).
    interrupt: std::sync::Arc<duckdb::InterruptHandle>,
    result: std::sync::Arc<std::sync::Mutex<Option<Result<lakehouse_reader::VersionDiff, String>>>>,
}

/// What the lakehouse footer asked for this frame.
pub(crate) enum LakeAct {
    Open(TableVersion),
    Diff {
        older: VersionId,
        newer: VersionId,
        older_label: String,
        newer_label: String,
    },
    Cancel,
}

impl LakeVersions {
    pub(crate) fn versions(&self) -> &[TableVersion] {
        self.history
            .as_ref()
            .map(|h| h.versions.as_slice())
            .unwrap_or(&[])
    }

    /// Stop a running comparison; its worker ends with an error nobody reads.
    pub(crate) fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.interrupt.interrupt();
        }
    }

    /// The finished comparison, once, with its two labels.
    pub(crate) fn take_finished(
        &mut self,
    ) -> Option<(
        Result<lakehouse_reader::VersionDiff, String>,
        String,
        String,
    )> {
        let done = self.job.as_ref()?.result.lock().ok()?.take()?;
        let job = self.job.take()?;
        Some((done, job.older, job.newer))
    }
}

/// `v12` for Delta, `snapshot 3` for Iceberg.
pub(crate) fn version_label(id: &VersionId) -> String {
    match id {
        VersionId::Delta(n) => format!("v{n}"),
        VersionId::Iceberg { sequence, .. } => {
            t("dialog.versions_lake_snapshot").replace("{n}", &sequence.to_string())
        }
    }
}

fn when_text(ms: Option<i64>) -> String {
    ms.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

/// `+120 -3 ~7`, only the parts the log recorded.
fn rows_text(v: &TableVersion) -> String {
    [(v.added, '+'), (v.removed, '-'), (v.updated, '~')]
        .iter()
        .filter_map(|(n, sign)| n.map(|n| format!("{sign}{n}")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

/// Footer for a lakehouse tab: Open, Compare with current, What changed,
/// Restore (always off), Close; a spinner and Cancel while comparing.
pub(crate) fn render_lake_footer(
    ui: &mut egui::Ui,
    lake: &LakeVersions,
    selected: Option<usize>,
    close: &mut bool,
) -> Option<LakeAct> {
    let mut act = None;
    let versions = lake.versions();
    let picked = selected.and_then(|i| versions.get(i));
    let newest = versions.first();
    let busy = lake.job.is_some();
    let pick_first = t("dialog.versions_pick");
    let why = |own: Option<String>| -> String {
        if busy {
            t("dialog.versions_lake_busy")
        } else {
            own.unwrap_or_else(|| pick_first.clone())
        }
    };
    octa::ui::control_row::control_line(ui, |ui| {
        if ui
            .add_enabled(
                picked.is_some() && !busy,
                egui::Button::new(t("common.open")),
            )
            .on_hover_text(t("dialog.versions_lake_open_hint"))
            .on_disabled_hover_text(why(None))
            .clicked()
        {
            act = picked.cloned().map(LakeAct::Open);
        }

        let is_newest = picked.is_some() && picked.map(|p| &p.id) == newest.map(|n| &n.id);
        if ui
            .add_enabled(
                picked.is_some() && !is_newest && !busy,
                egui::Button::new(t("dialog.versions_lake_compare")),
            )
            .on_hover_text(t("dialog.versions_lake_compare_hint"))
            .on_disabled_hover_text(why(
                is_newest.then(|| t("dialog.versions_lake_compare_newest"))
            ))
            .clicked()
            && let (Some(p), Some(n)) = (picked, newest)
        {
            act = Some(LakeAct::Diff {
                older_label: version_label(&p.id),
                newer_label: version_label(&n.id),
                older: p.id.clone(),
                newer: n.id.clone(),
            });
        }

        let previous = picked.and_then(|p| p.previous.clone());
        let first_reason = (picked.is_some() && previous.is_none())
            .then(|| t("dialog.versions_lake_changed_first"));
        if ui
            .add_enabled(
                previous.is_some() && !busy,
                egui::Button::new(t("dialog.versions_lake_changed")),
            )
            .on_hover_text(t("dialog.versions_lake_changed_hint"))
            .on_disabled_hover_text(why(first_reason))
            .clicked()
            && let (Some(p), Some(prev)) = (picked, previous)
        {
            act = Some(LakeAct::Diff {
                older_label: version_label(&prev),
                newer_label: version_label(&p.id),
                older: prev,
                newer: p.id.clone(),
            });
        }

        // Octa never writes a lakehouse table, so there is nothing to restore into.
        ui.add_enabled(false, egui::Button::new(t("dialog.versions_restore")))
            .on_disabled_hover_text(t("dialog.versions_lake_restore_off"));

        if busy {
            ui.spinner();
            ui.label(t("dialog.versions_lake_running"));
            if ui
                .button(t("common.cancel"))
                .on_hover_text(t("dialog.versions_lake_cancel_hint"))
                .clicked()
            {
                act = Some(LakeAct::Cancel);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t("common.close")).clicked() {
                *close = true;
            }
        });
    });
    act
}

/// The version list, newest first, or why there is none.
pub(crate) fn render_lake_list(
    ui: &mut egui::Ui,
    lake: &LakeVersions,
    selected: &mut Option<usize>,
) {
    let history = match &lake.history {
        Ok(h) => h,
        Err(why) => {
            let colour = ui.visuals().error_fg_color;
            octa::ui::message::selectable_message(ui, colour, why);
            return;
        }
    };
    ui.label(
        RichText::new(t("dialog.versions_lake_intro"))
            .small()
            .weak(),
    );
    if history.total > history.versions.len() {
        ui.label(
            RichText::new(
                t("dialog.versions_lake_shown")
                    .replace(
                        "{shown}",
                        &octa::ui::status_bar::format_number(history.versions.len()),
                    )
                    .replace(
                        "{total}",
                        &octa::ui::status_bar::format_number(history.total),
                    ),
            )
            .small()
            .weak(),
        );
    }
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, v) in history.versions.iter().enumerate() {
                let label = format!(
                    "{:<12}{}    {}    {}",
                    version_label(&v.id),
                    when_text(v.when_ms),
                    v.operation.as_deref().unwrap_or(""),
                    rows_text(v)
                );
                let mut resp = ui
                    .selectable_label(*selected == Some(i), label)
                    .on_hover_text(t("dialog.versions_lake_row_hint"));
                if let VersionId::Iceberg { snapshot_id, .. } = v.id {
                    resp = resp.on_hover_text(
                        t("dialog.versions_lake_snapshot_hint")
                            .replace("{id}", &snapshot_id.to_string()),
                    );
                }
                if resp.clicked() {
                    *selected = Some(i);
                }
            }
        });
}

impl OctaApp {
    /// Earlier versions on a lakehouse tab. `false` when `dir` is not a table
    /// folder, so the caller carries on with kept file copies.
    pub(crate) fn open_lake_versions(&mut self, dir: &Path) -> bool {
        let Some(kind) = lakehouse_reader::detect(dir) else {
            return false;
        };
        let history = match kind {
            LakehouseKind::Parts(_) => Err(t("dialog.versions_lake_none")),
            _ => octa::formats::lakehouse_history::list(dir, kind, LIST_LIMIT)
                .map_err(|e| format!("{e:#}")),
        };
        let selected = history
            .as_ref()
            .ok()
            .filter(|h| !h.versions.is_empty())
            .map(|_| 0);
        self.file_versions_dialog = Some(FileVersionsState {
            size: octa::ui::settings::DialogSize::default(),
            tab_idx: self.active_tab,
            file: dir.to_path_buf(),
            versions: Vec::new(),
            selected,
            lake: Some(LakeVersions {
                kind,
                history,
                job: None,
            }),
        });
        true
    }

    /// One version in a new tab, as a copy of the table at that version.
    pub(crate) fn open_lake_version(&mut self, dir: &Path, kind: LakehouseKind, v: &TableVersion) {
        match lakehouse_reader::read_version(dir, kind, &v.id) {
            Ok(mut table) => {
                table.source_path = None;
                table.format_name = None;
                let mut tab = crate::app::state::TabState::new(self.settings.default_search_mode);
                tab.table = table;
                tab.custom_tab_label =
                    Some(format!("{} @ {}", folder_name(dir), version_label(&v.id)));
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
            }
            Err(e) => self.status_message = Some((format!("{e:#}"), std::time::Instant::now())),
        }
    }

    /// Compare two versions on a worker thread. The connection is opened
    /// here so its interrupt handle exists before the query starts.
    pub(crate) fn start_lake_diff(
        &mut self,
        ctx: &egui::Context,
        lake: &mut LakeVersions,
        dir: std::path::PathBuf,
        older: (VersionId, String),
        newer: (VersionId, String),
    ) {
        let conn = match lakehouse_reader::open_connection(lake.kind) {
            Ok(c) => c,
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                return;
            }
        };
        let interrupt = conn.interrupt_handle();
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot = std::sync::Arc::clone(&result);
        let kind = lake.kind;
        let cap = octa::formats::initial_load_rows();
        let ctx = ctx.clone();
        let (older_id, older_label) = older;
        let (newer_id, newer_label) = newer;
        let task = self
            .long_tasks
            .start(octa::i18n::t("notify.task_lake_compare"));
        std::thread::spawn(move || {
            let _task = task;
            let out = lakehouse_reader::diff_versions(&conn, &dir, kind, &older_id, &newer_id, cap)
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut s) = slot.lock() {
                *s = Some(out);
            }
            ctx.request_repaint();
        });
        lake.job = Some(DiffJob {
            older: older_label,
            newer: newer_label,
            interrupt,
            result,
        });
    }

    /// A finished comparison as a new tab, the exact totals in its banner.
    pub(crate) fn open_lake_diff_tab(
        &mut self,
        dir: &Path,
        diff: lakehouse_reader::VersionDiff,
        older: &str,
        newer: &str,
    ) {
        let shown = diff.table.row_count();
        let mut banner = t("dialog.lake_diff_summary")
            .replace("{added}", &diff.added.to_string())
            .replace("{removed}", &diff.removed.to_string());
        for (cols, version) in [(&diff.only_in_newer, newer), (&diff.only_in_older, older)] {
            if !cols.is_empty() {
                banner.push(' ');
                banner.push_str(
                    &t("dialog.lake_diff_only_in")
                        .replace("{version}", version)
                        .replace("{cols}", &cols.join(", ")),
                );
            }
        }
        if (shown as u64) < diff.added + diff.removed {
            banner.push(' ');
            banner.push_str(&t("dialog.lake_diff_capped").replace("{shown}", &shown.to_string()));
        }
        let mut tab = crate::app::state::TabState::new(self.settings.default_search_mode);
        tab.table = diff.table;
        tab.parse_error_banner = Some(banner);
        tab.custom_tab_label = Some(format!("{}: {older} -> {newer}", folder_name(dir)));
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }
}
