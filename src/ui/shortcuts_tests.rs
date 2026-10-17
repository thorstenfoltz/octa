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
