//! The hover-switching top-level menu button used across the toolbar. Split out
//! of the main toolbar module purely for navigability - no behaviour change.

use egui::{Ui, WidgetText};

/// Top-level menu button that auto-switches on hover, restoring the
/// MS-Office-style behaviour that egui 0.31's `MenuRoot::stationary_interaction`
/// provided and that egui 0.34's `MenuButton` no longer does.
///
/// Click toggles open / closed. Hovering this button while a *different* top
/// popup is already open force-opens this one (the singleton popup state
/// replaces the previously open menu in one shot). When this menu's popup is
/// already the open one, hovering is a no-op so the popup doesn't churn.
///
/// We mirror `Popup::menu`'s setup (kind, layout, style, gap, and the
/// `MenuConfig` stack tag with `bar=false`) so submenu buttons rendered inside
/// `content` see `is_in_menu(ui) == true` and dispatch to
/// `SubMenuButton`, which carries its own hover-open logic for nested menus.
pub(super) fn top_menu_button(
    ui: &mut Ui,
    label: impl Into<WidgetText>,
    content: impl FnOnce(&mut Ui),
) -> egui::Response {
    let resp = ui.add(egui::Button::new(label));
    let ctx = ui.ctx().clone();
    let popup_id = resp.id;
    let was_open = egui::Popup::is_id_open(&ctx, popup_id);
    let any_open = egui::Popup::is_any_open(&ctx);

    let set_open = if resp.clicked() {
        Some(egui::SetOpenCommand::Toggle)
    } else if !was_open && resp.hovered() && any_open {
        Some(egui::SetOpenCommand::Bool(true))
    } else {
        None
    };

    // `MenuConfig::default()` already gives `bar: false`, which is what makes
    // `is_in_menu(ui)` inside `content` return true and dispatch submenu
    // buttons to `SubMenuButton`.
    let config = egui::containers::menu::MenuConfig::default();
    egui::Popup::from_response(&resp)
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .gap(0.0)
        .open_memory(set_open)
        .info(
            egui::UiStackInfo::new(egui::UiKind::Menu)
                .with_tag_value(egui::containers::menu::MenuConfig::MENU_CONFIG_TAG, config),
        )
        .show(|ui| {
            // Never wrap menu-item labels: in non-English locales longer strings
            // (e.g. German/Russian) would otherwise break onto a second line and
            // look cramped. `Extend` lets the popup grow to the widest item
            // instead, applied once here so every top menu inherits it.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            content(ui);
        });

    resp
}
