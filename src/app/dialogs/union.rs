//! Union-tables dialog (Analyse -> Union tables...).
//!
//! The user picks two or more open tabs to stack row-by-row, then reviews a
//! reconciliation plan: one row per merged column with a keep checkbox and a
//! target-type dropdown, pre-filled by [`octa::data::union::plan_union`].
//!
//! Applying calls [`octa::data::union::union_tables`] and opens the result in
//! a new tab (same pattern as the Pivot dialog). The active tab is pre-ticked
//! when the dialog opens.

use eframe::egui;
use egui::RichText;

use octa::data::union::{UnionPlan, plan_union, union_tables};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, TabState, UnionState};

/// Arrow type names the reconciliation dropdown offers.
const TYPE_OPTIONS: &[&str] = &["Int64", "Float64", "Utf8", "Date", "DateTime", "Bool"];

pub(crate) fn render_union_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.union_dialog.is_none() {
        return;
    }

    // Need at least one tab.
    if app.tabs.is_empty() {
        app.union_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.union_dialog.take().unwrap();

    // File mode: the sources are files picked in the directory sidebar, already
    // read into `file_tables`. The open-tab machinery below is bypassed.
    let file_mode = !st.file_sources.is_empty();

    // Guard: if tabs were added or closed since the dialog opened, resize the
    // selection vector so we never index out of bounds. Not applicable in file
    // mode, where the sources have nothing to do with the open tabs.
    if !file_mode && st.selected_tabs.len() != app.tabs.len() {
        let active = app.active_tab;
        st.selected_tabs = vec![false; app.tabs.len()];
        if active < st.selected_tabs.len() {
            st.selected_tabs[active] = true;
        }
        st.plan = recompute_plan(app, &st.selected_tabs);
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_union_dialog");
    let window = egui::Window::new("octa_union")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(480.0)
            .min_width(360.0)
            .min_height(300.0)
    });

    // Track whether the source selection changes so we know when to recompute
    // the plan.
    let prev_selected = if file_mode {
        st.file_selected.clone()
    } else {
        st.selected_tabs.clone()
    };

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("union_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("union.title"))
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

        egui::Panel::bottom("union_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("union.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("union.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // --- Source picker ---
            ui.label(
                RichText::new(octa::i18n::t("union.sources_label"))
                    .strong()
                    .size(13.0),
            );

            // Collect the source labels once (avoid borrow conflicts inside the
            // closure below). In file mode these are the picked files' names,
            // otherwise the open tabs' labels.
            let labels: Vec<String> = if file_mode {
                st.file_sources
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.to_string_lossy().to_string())
                    })
                    .collect()
            } else {
                app.tabs
                    .iter()
                    .enumerate()
                    .map(|(i, tab)| {
                        tab.table
                            .source_path
                            .as_ref()
                            .and_then(|p| {
                                std::path::Path::new(p)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                            })
                            .or_else(|| tab.custom_tab_label.clone())
                            .unwrap_or_else(|| format!("Untitled {}", i + 1))
                    })
                    .collect()
            };
            // Full paths for the hover tooltips (file mode only): several files
            // in different folders can share a name.
            let hovers: Vec<String> = if file_mode {
                st.file_sources
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect()
            } else {
                Vec::new()
            };

            egui::ScrollArea::vertical()
                .id_salt("union_tab_list")
                .max_height(140.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (idx, label) in labels.iter().enumerate() {
                        let sel = if file_mode {
                            st.file_selected.get_mut(idx)
                        } else {
                            st.selected_tabs.get_mut(idx)
                        };
                        if let Some(sel) = sel {
                            let resp = ui.checkbox(sel, label.as_str());
                            if let Some(hover) = hovers.get(idx) {
                                resp.on_hover_text(hover);
                            }
                        }
                    }
                });

            ui.add_space(8.0);
            ui.separator();

            // --- Reconciliation plan ---
            if !st.plan.columns.is_empty() {
                ui.label(
                    RichText::new(octa::i18n::t("union.columns_label"))
                        .strong()
                        .size(13.0),
                );

                // Header row.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("union.keep_col"))
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(octa::i18n::t("union.type_col"))
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });

                egui::ScrollArea::vertical()
                    .id_salt("union_col_list")
                    .max_height(200.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (ci, col) in st.plan.columns.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                // Keep checkbox.
                                ui.checkbox(&mut col.include, col.name.as_str());

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // Target-type dropdown.
                                        egui::ComboBox::from_id_salt(
                                            egui::Id::new("union_type").with(ci),
                                        )
                                        .selected_text(col.target_type.as_str())
                                        .width(90.0)
                                        .show_ui(
                                            ui,
                                            |ui| {
                                                for &ty in TYPE_OPTIONS {
                                                    ui.selectable_value(
                                                        &mut col.target_type,
                                                        ty.to_string(),
                                                        ty,
                                                    );
                                                }
                                            },
                                        );
                                    },
                                );
                            });
                        }
                    });
            }

            // Inline error.
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

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    // Recompute plan when the selection changes (user's manual type/keep edits
    // are discarded on source-set change; that is intentional).
    let sel_now = if file_mode {
        &st.file_selected
    } else {
        &st.selected_tabs
    };
    if *sel_now != prev_selected {
        st.plan = if file_mode {
            recompute_plan_for_files(&st.file_tables, &st.file_selected)
        } else {
            recompute_plan(app, &st.selected_tabs)
        };
        st.error = None;
    }

    if apply {
        match apply_union(app, &st) {
            Ok(()) => {
                // Success: drop the dialog.
                return;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.union_dialog = Some(st);
    }
}

/// Recompute the `UnionPlan` from the currently-selected tabs.
fn recompute_plan(app: &OctaApp, selected: &[bool]) -> UnionPlan {
    let schemas: Vec<&[octa::data::ColumnInfo]> = app
        .tabs
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.get(*i).copied().unwrap_or(false))
        .map(|(_, tab)| tab.table.columns.as_slice())
        .collect();
    if schemas.is_empty() {
        UnionPlan { columns: vec![] }
    } else {
        plan_union(&schemas)
    }
}

/// Recompute the `UnionPlan` from the currently-selected files (file mode).
fn recompute_plan_for_files(tables: &[octa::data::DataTable], selected: &[bool]) -> UnionPlan {
    let schemas: Vec<&[octa::data::ColumnInfo]> = tables
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.get(*i).copied().unwrap_or(false))
        .map(|(_, t)| t.columns.as_slice())
        .collect();
    if schemas.is_empty() {
        UnionPlan { columns: vec![] }
    } else {
        plan_union(&schemas)
    }
}

/// Run the union engine and open the result in a new tab.
fn apply_union(app: &mut OctaApp, st: &UnionState) -> Result<(), String> {
    // Sources are either files read from disk (picked in the sidebar) or
    // snapshots of the selected open tabs.
    let owned_tables: Vec<octa::data::DataTable> = if !st.file_sources.is_empty() {
        st.file_tables
            .iter()
            .enumerate()
            .filter(|(i, _)| st.file_selected.get(*i).copied().unwrap_or(false))
            .map(|(_, t)| t.clone())
            .collect()
    } else {
        // Apply pending cell edits so the engine sees the visible values.
        st.selected_tabs
            .iter()
            .enumerate()
            .filter(|(_, sel)| **sel)
            .map(|(i, _)| {
                let mut snap = app.tabs[i].table.clone();
                snap.apply_edits();
                snap
            })
            .collect()
    };
    if owned_tables.len() < 2 {
        return Err(octa::i18n::t("union.need_two"));
    }

    let borrow_refs: Vec<&octa::data::DataTable> = owned_tables.iter().collect();

    let result = union_tables(&borrow_refs, &st.plan).map_err(|e| e.to_string())?;

    // Open result in a new tab (mirrors the pivot dialog pattern).
    let mut new_tab = TabState::new(app.settings.default_search_mode);
    new_tab.table = result;
    new_tab.table.source_path = None;
    new_tab.table.format_name = None;
    new_tab.custom_tab_label = Some(octa::i18n::t("union.title"));
    new_tab.filter_dirty = true;
    if new_tab.table.row_count() > 0 && new_tab.table.col_count() > 0 {
        new_tab.table_state.selected_cell = Some((0, 0));
    }
    app.tabs.push(new_tab);
    app.active_tab = app.tabs.len() - 1;

    Ok(())
}

impl OctaApp {
    /// Open the Union dialog over a set of files picked in the directory
    /// sidebar, without opening a tab per file. Each file is read once through
    /// the registry; files that cannot be read are skipped and reported.
    ///
    /// This is the "I have 40 parquet files in a folder and want one table"
    /// path: the dialog then behaves exactly as it does for open tabs, with the
    /// same column reconciliation plan.
    pub(crate) fn open_union_for_files(&mut self, files: Vec<std::path::PathBuf>) {
        let mut file_sources = Vec::new();
        let mut file_tables = Vec::new();
        let mut skipped = 0usize;

        for path in files {
            let read = self
                .registry
                .reader_for_path(&path)
                .ok_or_else(|| anyhow::anyhow!("no reader"))
                .and_then(|r| r.read_file(&path));
            match read {
                Ok(table) => {
                    file_sources.push(path);
                    file_tables.push(table);
                }
                Err(_) => skipped += 1,
            }
        }

        if file_tables.len() < 2 {
            self.status_message =
                Some((octa::i18n::t("union.need_two"), std::time::Instant::now()));
            return;
        }
        if skipped > 0 {
            self.status_message = Some((
                format!("{} {skipped}", octa::i18n::t("union_tree.skipped")),
                std::time::Instant::now(),
            ));
        }

        let file_selected = vec![true; file_tables.len()];
        let plan = recompute_plan_for_files(&file_tables, &file_selected);
        self.union_dialog = Some(UnionState {
            selected_tabs: Vec::new(),
            plan,
            error: None,
            size: octa::ui::settings::DialogSize::default(),
            file_sources,
            file_tables,
            file_selected,
        });
    }
}
