//! **View -> Tab memory...**: what each open tab is holding, and a way to
//! give it back.
//!
//! Octa keeps every open tab's rows in memory, so eleven tabs of parquet
//! parts is eleven tables. This dialog answers "which one is it" and lets a
//! tab be unloaded: its rows are dropped, the tab stays, and the file is
//! re-read the next time the tab is selected.
//!
//! Unloading is refused, not warned about, whenever it would lose data. The
//! rows are the only copy of anything unsaved, and a tab with no file behind
//! it has nothing to re-read.

use eframe::egui;
use egui::RichText;

use octa::data::tab_memory::{estimate_bytes, format_estimate};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, TabState};

/// Why a tab cannot be unloaded. Both cases lose data if ignored, which is
/// why this is a hard refusal with a reason rather than a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnloadRefusal {
    /// Unsaved cell edits or structural changes live only in memory.
    PendingEdits,
    /// Nothing on disk to reload from (a new table, a SQL result, a chart
    /// tab, a paste).
    NoSourcePath,
}

impl UnloadRefusal {
    /// The locale key explaining this refusal on the disabled button.
    fn i18n_key(self) -> &'static str {
        match self {
            UnloadRefusal::PendingEdits => "tabmem.refuse_edits",
            UnloadRefusal::NoSourcePath => "tabmem.refuse_no_file",
        }
    }
}

/// One line of the dialog: which tab, its name, its row count, its
/// estimated size, and whether it can be unloaded.
type MemoryRow = (usize, String, usize, u64, Result<(), UnloadRefusal>);

/// Whether `tab`'s rows can be dropped and re-read later.
pub(crate) fn can_unload(tab: &TabState) -> Result<(), UnloadRefusal> {
    if !tab.table.edits.is_empty() || tab.table.structural_changes {
        return Err(UnloadRefusal::PendingEdits);
    }
    if tab.table.source_path.is_none() {
        return Err(UnloadRefusal::NoSourcePath);
    }
    Ok(())
}

/// The estimate for one tab, computing it only when the cache is cold.
///
/// Walking every cell of every tab on every frame would make the dialog the
/// most expensive thing on screen, so the number is cached on the tab and
/// invalidated where the data changes.
fn cached_estimate(tab: &mut TabState) -> u64 {
    match tab.memory_estimate {
        Some(bytes) => bytes,
        None => {
            let bytes = estimate_bytes(&tab.table);
            tab.memory_estimate = Some(bytes);
            bytes
        }
    }
}

pub(crate) fn render_tab_memory_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_tab_memory {
        return;
    }
    let mut close = false;
    let mut unload: Option<usize> = None;
    let mut size = app.tab_memory_size;
    let minimized = size == DialogSize::Minimized;

    // Gathered up front so the row loop borrows nothing from `app`.
    let rows: Vec<MemoryRow> = (0..app.tabs.len())
        .map(|i| {
            let refusal = can_unload(&app.tabs[i]);
            let label = app.tabs[i].title_display();
            let row_count = app.tabs[i].table.row_count();
            let bytes = cached_estimate(&mut app.tabs[i]);
            (i, label, row_count, bytes, refusal)
        })
        .collect();
    let total: u64 = rows.iter().map(|r| r.3).sum();

    let dialog_id = egui::Id::new("octa_tab_memory_dialog");
    let window = egui::Window::new("octa_tab_memory")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(360.0)
            .min_width(380.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("tabmem_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("tabmem.title")).strong().size(16.0));
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

        egui::Panel::bottom("tabmem_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("tabmem.total").replace("{size}", &format_estimate(total)))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(t("tabmem.close"))
                            .on_hover_text(t("tabmem.close_hint"))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.weak(t("tabmem.estimate_note"));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("tabmem_grid")
                        .num_columns(4)
                        .spacing([16.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new(t("tabmem.col_tab")).strong());
                            ui.label(RichText::new(t("tabmem.col_rows")).strong());
                            ui.label(RichText::new(t("tabmem.col_size")).strong());
                            ui.label("");
                            ui.end_row();

                            for (index, label, row_count, bytes, refusal) in &rows {
                                ui.label(label);
                                ui.label(row_count.to_string());
                                ui.label(format_estimate(*bytes));
                                let btn = ui.add_enabled(
                                    refusal.is_ok(),
                                    egui::Button::new(t("tabmem.unload")),
                                );
                                let btn = match refusal {
                                    Ok(()) => btn.on_hover_text(t("tabmem.unload_hint")),
                                    Err(r) => btn.on_disabled_hover_text(t(r.i18n_key())),
                                };
                                if btn.clicked() {
                                    unload = Some(*index);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    app.tab_memory_size = size;
    if let Some(index) = unload {
        app.unload_tab(index);
    }
    if close {
        app.show_tab_memory = false;
    }
}

impl OctaApp {
    /// Drop a tab's rows, keeping the tab, its path, its view mode and its
    /// view state. The file is re-read the next time the tab is selected.
    ///
    /// Guarded by [`can_unload`] a second time: the dialog disables the
    /// button, but a refusal here is what makes losing data impossible
    /// rather than merely hard.
    pub(crate) fn unload_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if can_unload(tab).is_err() {
            return;
        }
        tab.table.rows.clear();
        tab.table.rows.shrink_to_fit();
        tab.filtered_rows.clear();
        tab.needs_reload = true;
        tab.memory_estimate = None;
        tab.filter_dirty = true;
        self.status_message = Some((t("tabmem.unloaded"), std::time::Instant::now()));
    }
}

impl OctaApp {
    /// Open the dialog with fresh numbers.
    ///
    /// The estimate is cached per tab because the walk touches every cell of
    /// every table, which must not happen per frame. Recomputing on open
    /// rather than tracking every mutation keeps one invalidation point
    /// instead of a dozen that can each be forgotten, and the dialog is
    /// transient enough that "as of when you opened it" is the truth a
    /// reader wants anyway.
    pub(crate) fn open_tab_memory(&mut self) {
        for tab in &mut self.tabs {
            tab.memory_estimate = None;
        }
        self.show_tab_memory = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::SearchMode;

    fn tab_with_source(path: &str) -> TabState {
        let mut tab = TabState::new(SearchMode::Plain);
        tab.table.source_path = Some(path.to_string());
        tab
    }

    #[test]
    fn a_tab_with_pending_edits_refuses_to_unload() {
        let mut tab = tab_with_source("/tmp/x.csv");
        assert!(can_unload(&tab).is_ok());
        tab.table
            .edits
            .insert((0, 0), octa::data::CellValue::Int(1));
        assert!(matches!(can_unload(&tab), Err(UnloadRefusal::PendingEdits)));
    }

    /// A structural change is not in the edit overlay, so checking only
    /// `edits` would let a deleted column be thrown away silently.
    #[test]
    fn a_tab_with_structural_changes_refuses_too() {
        let mut tab = tab_with_source("/tmp/x.csv");
        tab.table.structural_changes = true;
        assert!(matches!(can_unload(&tab), Err(UnloadRefusal::PendingEdits)));
    }

    #[test]
    fn a_tab_with_no_file_behind_it_refuses_to_unload() {
        let tab = TabState::new(SearchMode::Plain);
        assert!(
            matches!(can_unload(&tab), Err(UnloadRefusal::NoSourcePath)),
            "an in-memory table cannot be reloaded, so unloading it would lose it"
        );
    }

    /// Every refusal explains itself on the disabled button, or the user is
    /// left with a dead control and no reason.
    #[test]
    fn every_refusal_has_a_message() {
        for r in [UnloadRefusal::PendingEdits, UnloadRefusal::NoSourcePath] {
            assert!(r.i18n_key().starts_with("tabmem.refuse_"));
        }
    }
}
