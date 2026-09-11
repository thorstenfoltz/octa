//! The **Search** menu: find, replace, column filter and multi-search.
//! The duplicate finders live in the Data menu beside Drop duplicate rows.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};

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
            if !has_data {
                ui.weak(crate::i18n::t("menu.need_table"));
                return;
            }
            ui.set_min_width(180.0);
            if ui
                .button(crate::i18n::t("search_menu.find"))
                .on_hover_text(crate::i18n::t("search_menu.find_hint"))
                .clicked()
            {
                action.search_focus = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("search_menu.find_replace"))
                .on_hover_text(crate::i18n::t("search_menu.find_replace_hint"))
                .clicked()
            {
                action.toggle_replace_bar = true;
                ui.close();
            }
            ui.separator();
            // Excel-style per-column value filter. Deliberately *not*
            // suffixed with the shortcut combo (Ctrl+Shift+F by default)
            // - same convention as the F8 read-only menu entry.
            let filter_btn = ui
                .add_enabled(
                    has_data,
                    egui::Button::new(crate::i18n::t("search_menu.column_filter")),
                )
                .on_hover_text(crate::i18n::t("search_menu.column_filter_hint"));
            if filter_btn.clicked() {
                action.show_column_filter = Some(None);
                ui.close();
            }
            ui.separator();
            if ui
                .button(crate::i18n::t("search_menu.multi_search"))
                .on_hover_text(crate::i18n::t("search_menu.multi_search_hint"))
                .clicked()
            {
                action.toggle_multi_search = true;
                ui.close();
            }
        },
    );
}
