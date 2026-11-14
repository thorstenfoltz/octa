use super::*;
use strum::IntoEnumIterator;

/// No two actions may ship with the same default key combination, otherwise
/// one of them would be silently unreachable until rebound. (Unbound
/// defaults are exempt - several actions can legitimately ship with no key.)
#[test]
fn default_combos_are_unique() {
    let mut seen: HashMap<KeyCombo, ShortcutAction> = HashMap::new();
    for action in ShortcutAction::iter() {
        let combo = action.default_combo();
        if combo == KeyCombo::UNBOUND {
            continue;
        }
        if let Some(prev) = seen.insert(combo, action) {
            panic!(
                "default key {} is bound to both {:?} and {:?}",
                combo.label(),
                prev,
                action
            );
        }
    }
}

/// Every action must report a label and a group (the matches are
/// exhaustive, so this mostly guards the `strum` iterator wiring).
#[test]
fn every_action_has_label_and_group() {
    for action in ShortcutAction::iter() {
        assert!(!action.label().is_empty());
        let _ = action.group();
    }
}

#[test]
fn a_queued_command_fires_on_the_next_frame_only() {
    let s = Shortcuts::default();
    let input = egui::InputState::default();
    queue_command(ShortcutAction::OpenPivot);
    assert!(
        !s.triggered(ShortcutAction::OpenPivot, &input),
        "not before the frame starts"
    );
    begin_frame();
    assert!(s.triggered(ShortcutAction::OpenPivot, &input));
    assert!(
        s.triggered(ShortcutAction::OpenPivot, &input),
        "every site in that frame sees it"
    );
    assert!(!s.triggered(ShortcutAction::OpenChart, &input));
    begin_frame();
    assert!(
        !s.triggered(ShortcutAction::OpenPivot, &input),
        "gone after one frame"
    );
}

#[test]
fn find_command_is_ctrl_k_and_nothing_else_is() {
    let k = KeyCombo::ctrl(egui::Key::K);
    assert_eq!(ShortcutAction::FindCommand.default_combo(), k);
    let others = ShortcutAction::iter()
        .filter(|a| *a != ShortcutAction::FindCommand && a.default_combo() == k)
        .count();
    assert_eq!(others, 0);
}

#[test]
fn consume_removes_only_the_matching_key_press() {
    let combo = KeyCombo::ctrl(egui::Key::K);
    let mut input = egui::InputState::default();
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    for key in [egui::Key::K, egui::Key::J] {
        input.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        });
    }
    combo.consume(&mut input);
    assert_eq!(input.events.len(), 1);
}

/// Every command has a name: its menu's key or a `command.*` key, present in
/// the English catalogue (the parity test carries it to the others).
#[test]
fn every_action_has_a_name() {
    for a in ShortcutAction::iter() {
        let key = a.name_key();
        assert_ne!(crate::i18n::t(key), key, "{a:?} has no {key}");
    }
}

#[test]
fn a_menu_command_is_named_as_its_menu_names_it() {
    assert_eq!(ShortcutAction::OpenPivot.name_key(), "analyse_menu.pivot");
    assert_eq!(ShortcutAction::OpenPivot.place_key(), "menu.analyse");
    assert_eq!(ShortcutAction::ZoomIn.name_key(), "command.zoom_in");
    assert_eq!(ShortcutAction::NextTab.place_key(), "shortcut_group.tabs");
    assert_eq!(
        ShortcutAction::ToggleColumnNavigator.place_key(),
        "menu.columns"
    );
}

#[test]
fn every_place_is_a_real_key() {
    for a in ShortcutAction::iter() {
        let k = a.place_key();
        assert!(
            k.starts_with("menu.") || k.starts_with("shortcut_group."),
            "{a:?}: {k}"
        );
        assert_ne!(crate::i18n::t(k), k, "{a:?}: {k} does not resolve");
    }
}
