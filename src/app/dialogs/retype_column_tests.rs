use super::*;

/// The dialog must not offer Apply on a table whose preview says nothing
/// converts: that is a user error worth stopping, not a silent no-op.
#[test]
fn apply_is_offered_only_when_something_converts() {
    assert!(!apply_enabled(0, false), "nothing converts");
    assert!(apply_enabled(3, false), "some convert");
    assert!(!apply_enabled(3, true), "read-only mode always refuses");
}

/// Every target must have a locale key, or a chooser entry renders as its
/// own key name. `TargetType::ALL` is the chooser's source of truth, so
/// walking it is what catches a variant added without a label.
#[test]
fn every_target_has_a_label_key() {
    for &target in TargetType::ALL {
        let key = target.i18n_key();
        assert!(
            key.starts_with("retype.type_"),
            "{target:?} has no label key"
        );
    }
}
