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
            // a scary answer to a question the user never had to ask. The
            // startup check is skipped for the same reason - see
            // `app::update_loop`.
            if !crate::platform::is_store_packaged() {
                ui.separator();
                if ui
                    .button(crate::i18n::t("help_menu.check_updates"))
                    .on_hover_text(crate::i18n::t("help_menu.check_updates_hint"))
                    .clicked()
                {
                    action.check_for_updates = true;
                    ui.close();
                }
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
