//! The **Search** menu: find, replace, multi-search and Find a command.
//! The duplicate finders live in the Data menu beside Drop duplicate rows.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::i18n::t;
use crate::ui::menu_entry::menu_entry;

pub(super) fn search_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors, has_data, ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.search")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(180.0);
            let no_table = (!has_data).then(|| t("greyed.need_table"));
            if menu_entry(
                ui,
                &t("search_menu.find"),
                &t("search_menu.find_hint"),
                no_table.as_deref(),
            ) {
                action.search_focus = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &t("search_menu.find_replace"),
                &t("search_menu.find_replace_hint"),
                no_table.as_deref(),
            ) {
                action.toggle_replace_bar = true;
                ui.close();
            }
            ui.separator();
            // Searches a folder, so it works with no table open.
            if menu_entry(
                ui,
                &t("search_menu.multi_search"),
                &t("search_menu.multi_search_hint"),
                None,
            ) {
                action.toggle_multi_search = true;
                ui.close();
            }
            ui.separator();
            // Finds any command, so it needs no table either.
            if menu_entry(
                ui,
                &t("help_menu.find_command"),
                &t("help_menu.find_command_hint"),
                None,
            ) {
                action.open_command_search = true;
                ui.close();
            }
        },
    );
}
