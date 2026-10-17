//! The filter chip row: every active filter narrowing the view, in one row.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    pub(super) fn render_filter_chips(&mut self, ui: &mut egui::Ui) {
        // --- The filter chip row --------------------------------
        //
        // ONE wrapped row for everything currently narrowing the view:
        // the Ask-mode comparison filters, the duplicate filter and the
        // per-column value filters. They used to be separate blocks that
        // stacked into separate rows, which is the thing this row exists
        // to avoid: a user looking for "why am I seeing 12 rows" should
        // find every reason in one place.
        let chips: Vec<(String, ChipRemoval, &'static str)> = {
            let tab = &self.tabs[self.active_tab];
            let mut chips: Vec<(String, ChipRemoval, &'static str)> = tab
                .predicate_filters
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    (
                        f.label(&tab.table),
                        ChipRemoval::Predicate(i),
                        "search.ask_filter_remove",
                    )
                })
                .collect();
            if let Some(f) = tab.duplicate_filter.as_ref() {
                let label = octa::i18n::t(if f.keep_duplicates {
                    "search.dupfilter_dups"
                } else {
                    "search.dupfilter_unique"
                });
                chips.push((label, ChipRemoval::Duplicate, "search.dupfilter_remove"));
            }
            chips.extend(
                column_filter_chip_labels(&tab.table, &tab.column_filters)
                    .into_iter()
                    .map(|(col, label)| (label, ChipRemoval::Column(col), "facet.chip_remove")),
            );
            chips
        };

        if !chips.is_empty() {
            let predicate_count = self.tabs[self.active_tab].predicate_filters.len();
            let mut remove: Option<ChipRemoval> = None;
            let mut clear_all = false;
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                // Only the Ask-mode chips get this heading, and only
                // immediately before them: it says where those filters
                // came from, which is not true of the others.
                if predicate_count > 0 {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("search.ask_filters"))
                            .size(11.0)
                            .color(ui::theme::ThemeColors::for_mode(self.theme_mode).text_muted),
                    );
                }
                for (label, removal, hint) in &chips {
                    if ui
                        .small_button(format!("{label}  x"))
                        .on_hover_text(octa::i18n::t(hint))
                        .clicked()
                    {
                        remove = Some(*removal);
                    }
                }
                // Clears the whole row, which is what the row now means.
                if chips.len() > 1
                    && ui
                        .small_button(octa::i18n::t("search.ask_filters_clear"))
                        .on_hover_text(octa::i18n::t("search.ask_filters_clear_hint"))
                        .clicked()
                {
                    clear_all = true;
                }
            });
            ui.add_space(4.0);

            if clear_all || remove.is_some() {
                let tab = &mut self.tabs[self.active_tab];
                if clear_all {
                    tab.predicate_filters.clear();
                    tab.duplicate_filter = None;
                    tab.duplicate_filter_cache = None;
                    tab.column_filters.clear();
                } else {
                    match remove {
                        Some(ChipRemoval::Predicate(i)) if i < tab.predicate_filters.len() => {
                            tab.predicate_filters.remove(i);
                        }
                        Some(ChipRemoval::Duplicate) => {
                            tab.duplicate_filter = None;
                            tab.duplicate_filter_cache = None;
                        }
                        Some(ChipRemoval::Column(col)) => {
                            tab.column_filters.remove(&col);
                        }
                        _ => {}
                    }
                }
                tab.filter_dirty = true;
            }
        }
    }
}

/// Which filter a chip in the row removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipRemoval {
    /// An Ask-mode comparison filter, by its index.
    Predicate(usize),
    /// The duplicate filter.
    Duplicate,
    /// One column's value filter, by column index.
    Column(usize),
}

/// Chip text for each active column filter, in COLUMN order (a `HashMap`
/// iterates in whatever order it likes, and a row that reshuffles itself
/// between frames is unusable).
///
/// A single allowed value reads as the value itself; several read as a
/// count, because a chip listing fifteen values is not a chip any more. A
/// filter whose column has since been deleted is skipped rather than
/// rendered nameless.
pub(crate) fn column_filter_chip_labels(
    table: &octa::data::DataTable,
    filters: &std::collections::HashMap<usize, std::collections::HashSet<String>>,
) -> Vec<(usize, String)> {
    let mut cols: Vec<usize> = filters.keys().copied().collect();
    cols.sort_unstable();
    cols.into_iter()
        .filter_map(|col| {
            let name = &table.columns.get(col)?.name;
            let values = filters.get(&col)?;
            let label = match values.len() {
                0 => return None,
                1 => format!("{name}: {}", values.iter().next()?),
                n => format!(
                    "{name}: {}",
                    octa::i18n::t("facet.chip_values").replace("{count}", &n.to_string())
                ),
            };
            Some((col, label))
        })
        .collect()
}

#[cfg(test)]
#[path = "chips_chip_row_tests.rs"]
mod chip_row_tests;
