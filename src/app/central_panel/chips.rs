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
mod chip_row_tests {
    use super::*;
    use octa::data::{ColumnInfo, DataTable};
    use std::collections::{HashMap, HashSet};

    fn two_col_table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "city".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "year".into(),
                data_type: "Int64".into(),
            },
        ];
        t
    }

    /// A chip has to say what it is doing without being read twice. One
    /// allowed value reads as the value itself; several read as a count,
    /// because a chip listing fifteen values is not a chip any more.
    #[test]
    fn chip_labels_name_the_column_and_the_value_count() {
        let t = two_col_table();
        let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
        filters.insert(0, ["Aachen".to_string()].into_iter().collect());
        filters.insert(
            1,
            ["2025".to_string(), "2026".to_string()]
                .into_iter()
                .collect(),
        );

        let labels = column_filter_chip_labels(&t, &filters);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], (0, "city: Aachen".to_string()));
        assert_eq!(labels[1].0, 1);
        assert!(
            labels[1].1.starts_with("year: "),
            "names the column: {}",
            labels[1].1
        );
        assert!(
            labels[1].1.contains('2'),
            "several values report a count: {}",
            labels[1].1
        );
    }

    /// Chips come back in column order whatever order the map iterates, or
    /// the row would reshuffle itself between frames.
    #[test]
    fn chips_are_ordered_by_column_not_by_hash() {
        let t = two_col_table();
        let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
        filters.insert(1, ["2025".to_string()].into_iter().collect());
        filters.insert(0, ["Aachen".to_string()].into_iter().collect());
        let labels = column_filter_chip_labels(&t, &filters);
        assert_eq!(
            labels.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    /// A filter on a column that no longer exists (deleted while filtered)
    /// must be skipped, not panic and not render a nameless chip.
    #[test]
    fn a_filter_on_a_missing_column_is_skipped() {
        let t = two_col_table();
        let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
        filters.insert(9, ["x".to_string()].into_iter().collect());
        assert!(column_filter_chip_labels(&t, &filters).is_empty());
    }
}
