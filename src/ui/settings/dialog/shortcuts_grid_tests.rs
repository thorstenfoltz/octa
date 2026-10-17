use super::is_modifier_key;

#[test]
fn modifiers_are_not_bindable_keys_on_their_own() {
    // egui reports these as ordinary key events, and treating one as the
    // recorded key is what made every Ctrl/Shift/Alt combination
    // impossible to bind (it recorded "Ctrl + the Ctrl key").
    for key in [
        egui::Key::ControlLeft,
        egui::Key::ControlRight,
        egui::Key::ShiftLeft,
        egui::Key::ShiftRight,
        egui::Key::AltLeft,
        egui::Key::AltRight,
        egui::Key::SuperLeft,
        egui::Key::SuperRight,
    ] {
        assert!(is_modifier_key(key), "{key:?} must be skipped");
    }
    for key in [egui::Key::S, egui::Key::F5, egui::Key::Comma] {
        assert!(!is_modifier_key(key), "{key:?} must be recordable");
    }
}
