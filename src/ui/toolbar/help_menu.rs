//! The **Help** menu: documentation, shortcuts, updates, diagnostics, about.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};

pub(super) fn help_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx { colors, .. } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.help")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(180.0);
            if ui
                .button(crate::i18n::t("help_menu.documentation"))
                .on_hover_text(crate::i18n::t("help_menu.documentation_hint"))
                .clicked()
            {
                action.show_documentation = true;
                ui.close();
            }
            ui.separator();
            if ui
                .button(crate::i18n::t("help_menu.settings"))
                .on_hover_text(crate::i18n::t("help_menu.settings_hint"))
                .clicked()
            {
                action.show_settings = true;
                ui.close();
            }
            // Hidden on Store (MSIX) builds. The Store installs updates
            // itself, and Octa cannot replace its own files under
            // WindowsApps, so the entry never had an install to offer. It
            // could not even report reliably: the packaged process fails the
            // GitHub request with a certificate error, so all it produced was
            // a scary answer to a question the user never had to ask.
            // A Store copy gets its updates from the Store instead, on the
            // Store's own schedule; this takes the user straight to Octa's
            // page there, where an Update button waits when one is out. The
            // start-up check still runs there when switched on, because the
            // Store can be days late, and its message points to this entry
            // (see `app::update_loop`).
            ui.separator();
            if !crate::platform::is_store_packaged() {
                if ui
                    .button(crate::i18n::t("help_menu.check_updates"))
                    .on_hover_text(crate::i18n::t("help_menu.check_updates_hint"))
                    .clicked()
                {
                    action.check_for_updates = true;
                    ui.close();
                }
            } else if ui
                .button(crate::i18n::t("help_menu.store_updates"))
                .on_hover_text(crate::i18n::t("help_menu.store_updates_hint"))
                .clicked()
            {
                action.open_store_page = true;
                ui.close();
            }
            ui.separator();
            if ui
                .button(crate::i18n::t("ai_report.menu"))
                .on_hover_text(crate::i18n::t("ai_report.menu_hint"))
                .clicked()
            {
                action.show_ai_report = true;
                ui.close();
            }
            ui.separator();
            if ui
                .button(crate::i18n::t("diagnostics.menu_export"))
                .on_hover_text(crate::i18n::t("diagnostics.menu_export_hint"))
                .clicked()
            {
                action.export_debug_report = true;
                ui.close();
            }
            ui.separator();
            if ui
                .button(crate::i18n::t("help_menu.about"))
                .on_hover_text(crate::i18n::t("help_menu.about_hint"))
                .clicked()
            {
                action.show_about = true;
                ui.close();
            }
        },
    );
}
