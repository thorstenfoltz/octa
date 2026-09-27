//! "Merge versions" dialog: two or more edited versions of a table merged
//! per row and per cell by `octa::data::merge_versions`, the engine behind
//! `--merge` and the `merge_tables` tool.
//!
//! Two phases in one window. **Pick**: any number of versions (two or more),
//! each an open tab, a file, or (when the active tab's file sits in a git
//! merge conflict) a git index stage; one of them may be marked as the
//! original, which is what lets Octa tell who changed what. A git conflict
//! fills all three stages in and marks stage 1 as the original. The Merge
//! button reads them on a worker. **Review**: the loaded tables stay in the
//! state, so changing the key columns re-merges instantly; conflicts are
//! settled by clicking a value, and the result opens as a tab with every row
//! marked by what happened to it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::merge_versions::{
    ConflictKind, MergeResult, RowStatus, merge_versions, suggest_key,
};
use octa::data::{DataTable, MarkColor, MarkKey};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

/// The versions as read, with the original split out.
pub(crate) struct Loaded {
    original: Option<DataTable>,
    versions: Vec<DataTable>,
    /// One label per entry of `versions`, for the conflict grid.
    labels: Vec<String>,
    /// Column names every table has, the only ones that can be a key.
    common: Vec<String>,
}

type LoadSlot = Arc<Mutex<Option<Result<(Loaded, Option<String>), String>>>>;

/// Where one version comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeSource {
    Tab(usize),
    File(Option<PathBuf>),
    /// Git index stage 1 (original), 2 (ours) or 3 (theirs) of `GitConflict`.
    Git(u8),
}

/// The active tab's file while it sits in a git merge conflict.
#[derive(Debug, Clone)]
pub(crate) struct GitConflict {
    root: PathBuf,
    relpath: String,
    /// The working-tree file, which the merged tab saves back into.
    working: PathBuf,
}

pub(crate) struct MergeVersionsState {
    size: DialogSize,
    sources: Vec<MergeSource>,
    /// Index into `sources` of the original, if one is marked.
    original: Option<usize>,
    git: Option<GitConflict>,
    job: Option<LoadSlot>,
    loaded: Option<Loaded>,
    /// Indices into `Loaded::common`.
    keys: Vec<usize>,
    result: Option<MergeResult>,
    /// Version index the "take all from" button uses.
    take_from: usize,
    msg: Option<(bool, String)>,
}

impl MergeSource {
    fn is_ready(&self, app: &OctaApp) -> bool {
        match self {
            Self::Tab(i) => app.tabs.get(*i).is_some_and(|t| t.table.col_count() > 0),
            Self::File(p) => p.is_some(),
            Self::Git(_) => true,
        }
    }

    fn label(&self, app: &OctaApp) -> String {
        match self {
            Self::Tab(i) => app
                .tabs
                .get(*i)
                .map(|t| t.title_display())
                .unwrap_or_default(),
            Self::File(p) => p
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            Self::Git(1) => t("mergev.git_base"),
            Self::Git(2) => t("mergev.git_ours"),
            Self::Git(_) => t("mergev.git_theirs"),
        }
    }
}

/// A version resolved on the UI thread, ready to move into the worker.
enum Resolved {
    Table(Box<DataTable>),
    File(PathBuf),
    Git(GitConflict, u8),
}

impl OctaApp {
    /// Entry from the File menu and the shortcut. A git conflict on the
    /// active tab's file fills in the three stages, the first as original;
    /// otherwise the active tab and a file to pick, no original.
    pub(crate) fn open_merge_versions_dialog(&mut self) {
        let git = self.tabs[self.active_tab]
            .table
            .source_path
            .as_deref()
            .and_then(|p| git_conflict(std::path::Path::new(p)));
        let (sources, original) = if git.is_some() {
            (
                vec![
                    MergeSource::Git(1),
                    MergeSource::Git(2),
                    MergeSource::Git(3),
                ],
                Some(0),
            )
        } else {
            (
                vec![MergeSource::Tab(self.active_tab), MergeSource::File(None)],
                None,
            )
        };
        self.merge_versions_dialog = Some(MergeVersionsState {
            size: DialogSize::Normal,
            sources,
            original,
            git,
            job: None,
            loaded: None,
            keys: Vec::new(),
            result: None,
            take_from: 0,
            msg: None,
        });
    }

    fn resolve_merge_source(
        &self,
        src: &MergeSource,
        git: &Option<GitConflict>,
    ) -> Option<Resolved> {
        match src {
            MergeSource::Tab(i) => {
                let mut snap = self.tabs.get(*i)?.table.clone();
                snap.apply_edits();
                Some(Resolved::Table(Box::new(snap)))
            }
            MergeSource::File(p) => p.clone().map(Resolved::File),
            MergeSource::Git(stage) => git.clone().map(|g| Resolved::Git(g, *stage)),
        }
    }

    fn spawn_merge_load(&self, st: &mut MergeVersionsState, ctx: &egui::Context) {
        let resolved: Option<Vec<Resolved>> = st
            .sources
            .iter()
            .map(|s| self.resolve_merge_source(s, &st.git))
            .collect();
        let Some(resolved) = resolved else {
            st.msg = Some((false, t("mergev.need_all")));
            return;
        };
        let labels: Vec<String> = st
            .sources
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != st.original)
            .map(|(i, s)| format!("{}: {}", i + 1, s.label(self)))
            .collect();
        let original_idx = st.original;
        let cap = if self.settings.max_decompressed_unlimited {
            u64::MAX
        } else {
            self.settings.max_decompressed_bytes
        };
        let slot: LoadSlot = Arc::new(Mutex::new(None));
        st.job = Some(slot.clone());
        st.msg = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<(Loaded, Option<String>)> {
                let mut original = None;
                let mut versions = Vec::new();
                for (i, r) in resolved.into_iter().enumerate() {
                    let table = match r {
                        Resolved::Table(t) => *t,
                        Resolved::File(p) => octa::formats::read_table_auto(&p, None, cap)?,
                        Resolved::Git(g, stage) => read_git_stage(&g, stage, cap)?,
                    };
                    if Some(i) == original_idx {
                        original = Some(table);
                    } else {
                        versions.push(table);
                    }
                }
                let all: Vec<&DataTable> = original.iter().chain(versions.iter()).collect();
                let key = suggest_key(&all);
                let common = all[0]
                    .columns
                    .iter()
                    .map(|c| c.name.clone())
                    .filter(|n| all.iter().all(|t| t.columns.iter().any(|c| &c.name == n)))
                    .collect();
                Ok((
                    Loaded {
                        original,
                        versions,
                        labels,
                        common,
                    },
                    key,
                ))
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// The merged table in a new tab, each row marked by what happened to
    /// it. From a git conflict the tab points at the conflicted file instead,
    /// unmarked (the xlsx writer saves marks as cell fills), so Save writes
    /// the resolution where git expects it.
    fn open_merge_result_tab(
        &mut self,
        table: DataTable,
        status: &[RowStatus],
        git: Option<&GitConflict>,
    ) {
        let mut new_tab = super::super::state::TabState::new(self.settings.default_search_mode);
        new_tab.table = table;
        let marks = if git.is_some() { &[][..] } else { status };
        for (row, s) in marks.iter().enumerate() {
            let color = match s {
                RowStatus::Unchanged => continue,
                RowStatus::Changed => MarkColor::Green,
                RowStatus::Added => MarkColor::Blue,
                RowStatus::Conflict => MarkColor::Orange,
            };
            new_tab.table.marks.insert(MarkKey::Row(row), color);
        }
        match git {
            Some(g) => {
                new_tab.table.format_name = octa::formats::FormatRegistry::new()
                    .reader_for_path(&g.working)
                    .map(|r| r.name().to_string());
                new_tab.table.source_path = Some(g.working.to_string_lossy().to_string());
                new_tab.table.structural_changes = true;
                new_tab.parse_error_banner = Some(t("mergev.git_saved_note"));
            }
            None => {
                new_tab.table.source_path = None;
                new_tab.table.format_name = None;
                new_tab.custom_tab_label = Some(t("mergev.tab_label"));
            }
        }
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }
}

/// `Some` when `path` is in a git repository and git holds an original
/// (stage 1) for it, which is what a merge conflict looks like in the index.
/// ponytail: reads the stage-1 blob once to find out; `git ls-files -u` if a
/// huge conflicted file makes the menu click feel slow.
fn git_conflict(path: &std::path::Path) -> Option<GitConflict> {
    let root = octa::git::repo_root(path)?;
    let relpath = octa::git::relative_path(path, &root)?;
    octa::git::show_at(&root, ":1", &relpath).ok()?;
    Some(GitConflict {
        root,
        relpath,
        working: path.to_path_buf(),
    })
}

/// Read one index stage. `read_table_at` names the temp file like the
/// original, since readers pick their format by name.
fn read_git_stage(g: &GitConflict, stage: u8, cap: u64) -> anyhow::Result<DataTable> {
    octa::git::history::read_table_at(&g.root, &format!(":{stage}"), &g.relpath, cap)
}

/// Re-run the merge on the loaded versions with the chosen keys.
fn remerge(st: &mut MergeVersionsState) {
    let Some(l) = &st.loaded else { return };
    let keys: Vec<String> = st.keys.iter().map(|&i| l.common[i].clone()).collect();
    let refs: Vec<&DataTable> = l.versions.iter().collect();
    match merge_versions(l.original.as_ref(), &refs, &keys) {
        Ok(r) => {
            st.result = Some(r);
            st.msg = None;
        }
        Err(e) => {
            st.result = None;
            st.msg = Some((false, format!("{e:#}")));
        }
    }
}

/// One version's row: its number, the original radio, the source picker and
/// a remove button. Returns (pick a file, remove this row).
fn source_row(
    ui: &mut egui::Ui,
    idx: usize,
    src: &mut MergeSource,
    original: &mut Option<usize>,
    tabs: &[(usize, String)],
    has_git: bool,
    removable: bool,
) -> (bool, bool) {
    let (mut pick, mut remove) = (false, false);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("mergev.version_n").replace("{n}", &(idx + 1).to_string())).strong(),
        );
        if ui
            .radio(*original == Some(idx), t("mergev.is_original"))
            .on_hover_text(t("mergev.is_original_hint"))
            .clicked()
        {
            *original = Some(idx);
        }
        if removable
            && ui
                .small_button("x")
                .on_hover_text(t("mergev.remove_hint"))
                .clicked()
        {
            remove = true;
        }
    });
    let kind: u8 = match src {
        MergeSource::Tab(_) => 0,
        MergeSource::File(_) => 1,
        MergeSource::Git(_) => 2,
    };
    ui.horizontal(|ui| {
        if ui
            .radio(kind == 0, t("mergev.source_tab"))
            .on_hover_text(t("mergev.source_tab_hint"))
            .clicked()
            && kind != 0
        {
            *src = MergeSource::Tab(tabs.first().map(|(i, _)| *i).unwrap_or(0));
        }
        if ui
            .radio(kind == 1, t("mergev.source_file"))
            .on_hover_text(t("mergev.source_file_hint"))
            .clicked()
            && kind != 1
        {
            *src = MergeSource::File(None);
        }
        let git = ui.add_enabled(
            has_git,
            egui::RadioButton::new(kind == 2, t("mergev.source_git")),
        );
        let git = if has_git {
            git.on_hover_text(t("mergev.source_git_hint"))
        } else {
            git.on_disabled_hover_text(t("mergev.source_git_disabled"))
        };
        if git.clicked() && kind != 2 {
            *src = MergeSource::Git((idx as u8 + 1).min(3));
        }
    });
    match src {
        MergeSource::Tab(sel) => {
            let selected = tabs
                .iter()
                .find(|(i, _)| i == sel)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt(format!("mergev_tab_{idx}"))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (i, name) in tabs {
                        ui.selectable_value(sel, *i, name);
                    }
                })
                .response
                .on_hover_text(t("mergev.source_tab_hint"));
        }
        MergeSource::File(path) => {
            ui.horizontal(|ui| {
                pick = ui
                    .button(t("mergev.choose_file"))
                    .on_hover_text(t("mergev.source_file_hint"))
                    .clicked();
                if let Some(p) = path {
                    ui.label(
                        RichText::new(p.file_name().unwrap_or_default().to_string_lossy())
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            });
        }
        MergeSource::Git(stage) => {
            ui.horizontal(|ui| {
                for (s, key) in [
                    (1u8, "mergev.git_base"),
                    (2, "mergev.git_ours"),
                    (3, "mergev.git_theirs"),
                ] {
                    ui.radio_value(stage, s, t(key))
                        .on_hover_text(t("mergev.source_git_hint"));
                }
            });
        }
    }
    (pick, remove)
}

/// The conflict list: one line per decision, every distinct value a clickable
/// choice labelled with the versions that hold it.
fn conflict_grid(ui: &mut egui::Ui, r: &mut MergeResult, labels: &[String]) {
    let cols = r.table.columns.clone();
    let from = |f: &[usize]| {
        f.iter()
            .map(|v| (v + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    egui::ScrollArea::vertical()
        .id_salt("mergev_conflicts")
        .max_height(280.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("mergev_conflict_grid")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for key in [
                        "mergev.col_row",
                        "mergev.col_column",
                        "mergev.col_original",
                        "mergev.col_choices",
                    ] {
                        ui.label(RichText::new(t(key)).strong());
                    }
                    ui.end_row();
                    for c in &mut r.conflicts {
                        ui.label((c.row + 1).to_string());
                        let choices: Vec<(String, String)> = match &c.kind {
                            ConflictKind::Cell {
                                col,
                                original,
                                options,
                                ..
                            } => {
                                ui.label(cols[*col].name.clone());
                                ui.label(
                                    RichText::new(
                                        original
                                            .as_ref()
                                            .map(|v| v.to_string())
                                            .unwrap_or_default(),
                                    )
                                    .color(ui.visuals().weak_text_color()),
                                );
                                options
                                    .iter()
                                    .map(|o| {
                                        let names: Vec<&str> = o
                                            .from
                                            .iter()
                                            .filter_map(|v| labels.get(*v).map(String::as_str))
                                            .collect();
                                        (
                                            format!("{}  [{}]", o.value, from(&o.from)),
                                            names.join("\n"),
                                        )
                                    })
                                    .collect()
                            }
                            ConflictKind::DeleteVsEdit {
                                deleted_by,
                                edited_by,
                            } => {
                                ui.label(t("mergev.whole_row"));
                                ui.label("");
                                vec![
                                    (
                                        format!(
                                            "{}  [{}]",
                                            t("mergev.delete_row"),
                                            from(deleted_by)
                                        ),
                                        t("mergev.delete_row_hint"),
                                    ),
                                    (
                                        format!("{}  [{}]", t("mergev.keep_row"), from(edited_by)),
                                        t("mergev.keep_row_hint"),
                                    ),
                                ]
                            }
                        };
                        ui.horizontal_wrapped(|ui| {
                            for (i, (text, hint)) in choices.into_iter().enumerate() {
                                if ui
                                    .selectable_label(c.choice == Some(i), text)
                                    .on_hover_text(format!("{}\n{hint}", t("mergev.pick_hint")))
                                    .clicked()
                                {
                                    c.choice = Some(i);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
        });
}

pub(crate) fn render_merge_versions_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.merge_versions_dialog.take() else {
        return;
    };
    let mut close = false;
    let mut run = false;
    let mut pick: Option<usize> = None;
    let mut remove: Option<usize> = None;
    let mut open_result = false;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    if let Some(slot) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.job = None;
        match res {
            Ok((loaded, key)) => {
                st.keys = key
                    .and_then(|k| loaded.common.iter().position(|c| *c == k))
                    .into_iter()
                    .collect();
                st.take_from = 0;
                st.loaded = Some(loaded);
                remerge(&mut st);
            }
            Err(e) => st.msg = Some((false, e)),
        }
    }
    let running = st.job.is_some();

    let tabs: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| (i, app.tabs[i].title_display()))
        .collect();
    let enough = st.sources.len() >= 2;
    let ready = enough && st.sources.iter().all(|s| s.is_ready(app));

    let dialog_id = egui::Id::new("octa_merge_versions_dialog");
    let window = egui::Window::new("octa_merge_versions")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(640.0)
            .default_height(500.0)
            .min_width(440.0)
            .min_height(260.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("mergev_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("mergev.title")).strong().size(16.0));
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

        egui::Panel::bottom("mergev_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(t("mergev.running"));
                    } else if let (Some(r), Some(l)) = (&mut st.result, &st.loaded) {
                        let open = r.unresolved();
                        let btn =
                            ui.add_enabled(open == 0, egui::Button::new(t("mergev.open_result")));
                        if open == 0 {
                            open_result = btn.on_hover_text(t("mergev.open_result_hint")).clicked();
                        } else {
                            btn.on_disabled_hover_text(
                                t("mergev.open_result_disabled").replace("{n}", &open.to_string()),
                            );
                        }
                        if !r.conflicts.is_empty() {
                            egui::ComboBox::from_id_salt("mergev_take_from")
                                .selected_text(
                                    l.labels.get(st.take_from).cloned().unwrap_or_default(),
                                )
                                .show_ui(ui, |ui| {
                                    for (i, name) in l.labels.iter().enumerate() {
                                        ui.selectable_value(&mut st.take_from, i, name);
                                    }
                                })
                                .response
                                .on_hover_text(t("mergev.take_all_hint"));
                            if ui
                                .button(t("mergev.take_all"))
                                .on_hover_text(t("mergev.take_all_hint"))
                                .clicked()
                            {
                                for c in &mut r.conflicts {
                                    c.prefer(st.take_from);
                                }
                            }
                        }
                    } else {
                        let btn = ui.add_enabled(ready, egui::Button::new(t("mergev.merge")));
                        if ready {
                            run = btn.on_hover_text(t("mergev.merge_hint")).clicked();
                        } else {
                            btn.on_disabled_hover_text(t("mergev.need_all"));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            close = true;
                        }
                        if st.loaded.is_some()
                            && !running
                            && ui
                                .button(t("mergev.back"))
                                .on_hover_text(t("mergev.back_hint"))
                                .clicked()
                        {
                            st.loaded = None;
                            st.result = None;
                            st.msg = None;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if st.loaded.is_none() {
                ui.label(t("mergev.hint"));
                if st.git.is_some() {
                    ui.label(
                        RichText::new(t("mergev.git_detected")).color(ui.visuals().warn_fg_color),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .radio(st.original.is_none(), t("mergev.no_original"))
                        .on_hover_text(t("mergev.no_original_hint"))
                        .clicked()
                    {
                        st.original = None;
                    }
                });
                ui.add_space(4.0);
                let has_git = st.git.is_some();
                let removable = st.sources.len() > 2;
                egui::ScrollArea::vertical()
                    .id_salt("mergev_sources")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (i, src) in st.sources.iter_mut().enumerate() {
                            let (p, r) =
                                source_row(ui, i, src, &mut st.original, &tabs, has_git, removable);
                            if p {
                                pick = Some(i);
                            }
                            if r {
                                remove = Some(i);
                            }
                            ui.add_space(6.0);
                        }
                        if ui
                            .button(t("mergev.add_version"))
                            .on_hover_text(t("mergev.add_version_hint"))
                            .clicked()
                        {
                            st.sources.push(MergeSource::File(None));
                        }
                    });
            } else {
                let mut keys_changed = false;
                if let Some(l) = &st.loaded {
                    ui.label(RichText::new(t("mergev.keys")).strong())
                        .on_hover_text(t("mergev.keys_hint"));
                    let before = st.keys.clone();
                    super::widgets::multi_col_picker(ui, "mergev_keys", &mut st.keys, &l.common);
                    keys_changed = before != st.keys;
                    if st.keys.is_empty() {
                        ui.label(
                            RichText::new(t("mergev.by_position"))
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                    if l.original.is_none() {
                        ui.label(
                            RichText::new(t("mergev.without_original_note"))
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                }
                if keys_changed {
                    remerge(&mut st);
                }
                ui.add_space(8.0);
                if let (Some(r), Some(l)) = (&mut st.result, &st.loaded) {
                    let count =
                        |s: RowStatus| r.status.iter().filter(|x| **x == s).count().to_string();
                    ui.label(
                        t("mergev.summary")
                            .replace("{unchanged}", &count(RowStatus::Unchanged))
                            .replace("{changed}", &count(RowStatus::Changed))
                            .replace("{added}", &count(RowStatus::Added))
                            .replace("{conflicts}", &r.conflicts.len().to_string()),
                    );
                    ui.label(
                        RichText::new(l.labels.join("   ")).color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(6.0);
                    if r.conflicts.is_empty() {
                        ui.label(t("mergev.conflicts_none"));
                    } else {
                        conflict_grid(ui, r, &l.labels);
                    }
                }
            }
            if let Some((ok, msg)) = &st.msg {
                ui.add_space(8.0);
                draw_result_message(ui, *ok, msg);
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if let Some(i) = remove {
        st.sources.remove(i);
        st.original = match st.original {
            Some(o) if o == i => None,
            Some(o) if o > i => Some(o - 1),
            o => o,
        };
    }
    if let Some(i) = pick
        && let Some(p) = rfd::FileDialog::new().pick_file()
    {
        st.sources[i] = MergeSource::File(Some(p));
    }
    if run {
        app.spawn_merge_load(&mut st, ctx);
    }
    if open_result && let Some(r) = &st.result {
        match r.finish_with_status() {
            Ok((table, status)) => {
                app.open_merge_result_tab(table, &status, st.git.as_ref());
                return; // the result is the tab
            }
            Err(e) => st.msg = Some((false, format!("{e:#}"))),
        }
    }
    if !close {
        app.merge_versions_dialog = Some(st);
    }
}
