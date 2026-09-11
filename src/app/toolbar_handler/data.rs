//! Dispatch for the Data menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::data_menu`, so a menu entry and
//! the code it runs sit in matching files.

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Data-menu action set on `action`.
    pub(super) fn dispatch_data_menu(&mut self, action: &ui::toolbar::ToolbarAction) {
        if action.time_calc {
            self.open_time_calc_dialog();
        }
        if action.open_fuzzy_join {
            crate::app::dialogs::fuzzy_join::open_fuzzy_join_dialog(self);
        }
        if action.open_validation {
            let tab = &mut self.tabs[self.active_tab];
            if tab.table.col_count() > 0 {
                tab.show_validation = true;
            }
        }
        if action.open_transform
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            self.transform_dialog = Some(crate::app::state::TransformState::default());
        }
        if action.open_conditional_column
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            self.conditional_column_dialog =
                Some(crate::app::state::ConditionalColumnState::default());
        }
        if action.open_anonymize
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            self.anonymize_dialog = Some(crate::app::state::AnonymizeState::default());
        }
        if action.open_impute
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            self.impute_dialog = Some(crate::app::state::ImputeState::default());
        }
        if action.open_multi_sort && self.tabs[self.active_tab].table.col_count() > 0 {
            self.multi_sort_dialog = Some(crate::app::state::MultiSortState::default());
        }
        if action.open_tidy_up
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            self.tidy_up_dialog = Some(crate::app::state::TidyUpState::default());
        }
        if action.filter_to_marked {
            self.toggle_filter_to_marked();
        }
        if action.add_bookmark {
            self.begin_add_bookmark();
        }
        if action.open_partition
            && !self.tabs.is_empty()
            && self.tabs[self.active_tab].table.col_count() > 0
        {
            self.partition_dialog = Some(crate::app::state::PartitionState {
                col: 0,
                out_dir: None,
                format: String::new(),
                layout: octa::data::partition::PartitionLayout::default(),
                error: None,
                size: octa::ui::settings::DialogSize::default(),
            });
        }
        // Union and Join both need a second open table. Surface a status
        // message instead of silently doing nothing when only one tab is open.
        if (action.open_union || action.open_join) && self.tabs.len() < 2 {
            self.status_message =
                Some((octa::i18n::t("union.need_open"), std::time::Instant::now()));
        }
        if action.open_union && self.tabs.len() >= 2 {
            let active = self.active_tab;
            let mut selected = vec![false; self.tabs.len()];
            if active < selected.len() {
                selected[active] = true;
            }
            let plan = octa::data::union::plan_union(
                &self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| selected.get(*i).copied().unwrap_or(false))
                    .map(|(_, t)| t.table.columns.as_slice())
                    .collect::<Vec<_>>(),
                false,
            );
            self.union_dialog = Some(crate::app::state::UnionState {
                ignore_case: false,
                selected_tabs: selected,
                plan,
                error: None,
                size: octa::ui::settings::DialogSize::default(),
                file_sources: Vec::new(),
                file_tables: Vec::new(),
                file_selected: Vec::new(),
            });
        }
        if action.open_join && self.tabs.len() >= 2 {
            self.join_dialog = Some(self.default_join_state());
        }
    }
}
