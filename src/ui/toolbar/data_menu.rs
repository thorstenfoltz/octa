//! The **Data** menu: sort, filter, validation, bookmarks and the mark filter.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::data::ViewMode;
use crate::i18n::t;
use crate::ui::menu_entry::menu_entry;

pub(super) fn data_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        current_view_mode,
        mark_filter_active,
        source_has_more,
        edits_blocked,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.data")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(200.0);
            if edits_blocked {
                ui.weak(crate::i18n::t("context_menu.header_row_readonly"));
                ui.separator();
            }
            // Reasons, in order: no table, read-only, the entry's own.
            let no_table = (!has_data).then(|| t("greyed.need_table"));
            let base = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("context_menu.header_row_readonly")));
            let entry = |ui: &mut Ui, label: &str, hint: &str, why: Option<String>| {
                menu_entry(ui, &t(label), &t(hint), why.as_deref())
            };
            if entry(
                ui,
                "toolbar.time_calc",
                "toolbar.time_calc_hint",
                base.clone(),
            ) {
                action.time_calc = true;
                ui.close();
            }
            if entry(ui, "refdata.menu", "refdata.menu_hint", base.clone()) {
                action.open_reference_data = true;
                ui.close();
            }
            if entry(ui, "transform.menu", "transform.menu_hint", base.clone()) {
                action.open_transform = true;
                ui.close();
            }
            let why_load = no_table
                .clone()
                .or_else(|| (!source_has_more).then(|| t("loadall.menu_disabled_hint")));
            if entry(ui, "loadall.menu", "loadall.menu_hint", why_load) {
                action.load_whole_table = true;
                ui.close();
            }
            // Filter to marked: label flips to the "clear" variant when
            // the filter is already active on this tab.
            let filter_marked_label = if mark_filter_active {
                crate::i18n::t("edit_menu.filter_to_marked_clear")
            } else {
                crate::i18n::t("edit_menu.filter_to_marked")
            };
            if menu_entry(
                ui,
                &filter_marked_label,
                &t("edit_menu.filter_to_marked_hint"),
                no_table.as_deref(),
            ) {
                action.filter_to_marked = true;
                ui.close();
            }
            if entry(ui, "bookmarks.add", "bookmarks.add_hint", no_table.clone()) {
                action.add_bookmark = true;
                ui.close();
            }

            ui.separator();

            // Find duplicates covers highlight / new tab / filter / drop.
            if entry(
                ui,
                "search_menu.find_duplicates",
                "search_menu.find_duplicates_hint",
                no_table.clone(),
            ) {
                action.show_find_duplicates = true;
                ui.close();
            }
            if entry(
                ui,
                "fuzzy_dup.menu",
                "fuzzy_dup.menu_hint",
                no_table.clone(),
            ) {
                action.open_fuzzy_duplicates = true;
                ui.close();
            }
            if entry(ui, "tabmem.menu", "tabmem.menu_hint", no_table.clone()) {
                action.open_tab_memory = true;
                ui.close();
            }
            ui.separator();
            if entry(ui, "impute.menu", "impute.menu_hint", base.clone()) {
                action.open_impute = true;
                ui.close();
            }
            if entry(ui, "anonymize.menu", "anonymize.menu_hint", base.clone()) {
                action.open_anonymize = true;
                ui.close();
            }
            if entry(ui, "testdata.menu", "testdata.menu_hint", no_table.clone()) {
                action.open_test_data = true;
                ui.close();
            }
            if entry(ui, "tidyup.menu", "tidyup.menu_hint", base.clone()) {
                action.open_tidy_up = true;
                ui.close();
            }

            // Data validation lives here rather than under
            // Columns: it is a statement about the data,
            // like dedupe and impute above it, not about
            // how a column is displayed.
            if entry(
                ui,
                "edit_menu.validation",
                "edit_menu.validation_hint",
                no_table.clone(),
            ) {
                action.open_validation = true;
                ui.close();
            }

            // Multi-table / file-writing data ops act on the active
            // table, so they're shown only on Table-view tabs (same
            // gate they had in the Analyse menu before the reorg).
            let why_view = no_table.clone().or_else(|| {
                (current_view_mode != ViewMode::Table).then(|| t("greyed.need_table_view"))
            });
            ui.separator();
            if entry(ui, "union.menu", "union.menu_hint", why_view.clone()) {
                action.open_union = true;
                ui.close();
            }
            if entry(ui, "join.menu", "join.menu_hint", why_view.clone()) {
                action.open_join = true;
                ui.close();
            }
            if entry(
                ui,
                "fuzzy_join.menu",
                "fuzzy_join.menu_hint",
                why_view.clone(),
            ) {
                action.open_fuzzy_join = true;
                ui.close();
            }
            if entry(
                ui,
                "partition.menu",
                "partition.menu_hint",
                why_view.clone(),
            ) {
                action.open_partition = true;
                ui.close();
            }
        },
    );
}
