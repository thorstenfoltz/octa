pub mod chat_models;
pub mod chat_profiles;
pub mod chat_tools;
pub mod chat_troubleshoot;
pub mod cloud_secrets;
pub mod db_secrets;
mod dialog;
pub mod secrets;
pub mod write_options_ui;
pub use write_options_ui::render_write_options;

mod app_settings;
mod enums;

pub use app_settings::*;
pub use enums::*;
// Declared inside `dialog/` so the per-section renderers, which are its
// children, can reach `SettingsDialog`'s private fields.
pub use dialog::{ChatTestRequest, SecretPurge, SettingsDialog, ShortcutTakeover};
// Chrome shared by ~70 dialogs. It lives in `ui::dialog_chrome` now; the
// re-export keeps every `ui::settings::DialogSize` import working.
pub use super::dialog_chrome::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
