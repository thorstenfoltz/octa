//! The **Copy as** submenu: one for the Edit menu and both cell menus, so the
//! three cannot list different formats.

use crate::data::copy_formats::CopyAs;
use crate::i18n::t;
use crate::ui::menu_entry::disabled_tip;

/// `Copy as >` with its entries; the one picked this frame, if any. No
/// entry ends in `...`: each copies at once. `why` is `None` when copying can
/// run; otherwise the submenu is greyed and its tooltip says why.
pub fn copy_as_menu(ui: &mut egui::Ui, why: Option<&str>) -> Option<CopyAs> {
    let mut picked = None;
    let resp = ui
        .add_enabled_ui(why.is_none(), |ui| {
            ui.menu_button(t("copy_as.menu"), |ui| {
                for f in CopyAs::ALL {
                    if ui
                        .button(t(f.label_key()))
                        .on_hover_text(t(f.hint_key()))
                        .clicked()
                    {
                        picked = Some(f);
                        ui.close();
                    }
                }
            })
            .response
        })
        .inner;
    match why {
        None => resp.on_hover_text(t("copy_as.menu_hint")),
        Some(why) => resp.on_disabled_hover_text(disabled_tip(&t("copy_as.menu_hint"), why)),
    };
    picked
}
