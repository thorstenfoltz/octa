//! Greyed, never hidden (user, 2026-10-31): a menu entry or setting whose use
//! depends on state is always shown. When it cannot run now it is greyed and
//! its tooltip says what it does and why it is greyed, so a user never hunts
//! for an entry that only appears after something else happened.
//! `tests/greyed_not_hidden_tests.rs` guards the rule.

use egui::Ui;

/// A menu button that is always there. `why` is `None` when it can run;
/// otherwise the button is greyed and `why` is added to its tooltip.
/// Returns true on a click.
pub fn menu_entry(ui: &mut Ui, label: &str, hint: &str, why: Option<&str>) -> bool {
    let resp = ui.add_enabled(why.is_none(), egui::Button::new(label));
    match why {
        None if hint.is_empty() => resp.clicked(),
        None => resp.on_hover_text(hint).clicked(),
        Some(why) => {
            resp.on_disabled_hover_text(disabled_tip(hint, why));
            false
        }
    }
}

/// The tooltip of a greyed control: what it does, a blank line, why it is
/// greyed. For checkboxes, submenus and combos drawn with `add_enabled`.
pub fn disabled_tip(hint: &str, why: &str) -> String {
    if hint.is_empty() {
        why.to_string()
    } else {
        format!("{hint}\n\n{why}")
    }
}

/// The three tile layouts as menu entries (Side by side, Stacked, Grid), shared
/// by the tab menu, the View menu and the group chip menu. `current` gets a
/// highlight, and `count` is how many tabs the layout would hold: Grid is greyed
/// while it would look the same as Side by side. Returns the layout clicked.
pub fn tile_layout_entries(
    ui: &mut Ui,
    current: Option<crate::ui::tiles::TileLayout>,
    count: usize,
) -> Option<crate::ui::tiles::TileLayout> {
    use crate::i18n::t;
    use crate::ui::tiles::{TileLayout as L, grid_adds_nothing};
    let mut picked = None;
    for (layout, key) in [
        (L::SideBySide, "tiles.side_by_side"),
        (L::Stacked, "tiles.stacked"),
        (L::Grid, "tiles.grid"),
    ] {
        let hint = t(&format!("{key}_hint"));
        let why =
            (layout == L::Grid && grid_adds_nothing(count)).then(|| t("greyed.grid_needs_three"));
        let resp = ui.add_enabled(
            why.is_none(),
            egui::Button::new(t(key)).selected(current == Some(layout)),
        );
        let resp = match &why {
            None => resp.on_hover_text(hint),
            Some(why) => resp.on_disabled_hover_text(disabled_tip(&hint, why)),
        };
        if resp.clicked() {
            picked = Some(layout);
        }
    }
    picked
}

/// A submenu of the three tile layouts for `count` tabs, greyed with `why` (and
/// the reason on hover) when set. Closes the menu on a pick and returns the
/// layout.
pub fn tile_submenu(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    why: Option<&str>,
    count: usize,
) -> Option<crate::ui::tiles::TileLayout> {
    let mut picked = None;
    let resp = ui
        .add_enabled_ui(why.is_none(), |ui| {
            ui.menu_button(label, |ui| {
                picked = tile_layout_entries(ui, None, count);
                if picked.is_some() {
                    ui.close();
                }
            })
            .response
        })
        .inner;
    match why {
        None => resp.on_hover_text(hint),
        Some(why) => resp.on_disabled_hover_text(disabled_tip(hint, why)),
    };
    picked
}

#[cfg(test)]
#[path = "menu_entry_tests.rs"]
mod tests;
