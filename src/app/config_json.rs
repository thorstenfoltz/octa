//! Small JSON lists kept in the config folder: search history, SQL snippets,
//! chat prompts. A missing, unreadable or malformed file reads as empty; a
//! save is best-effort (errors ignored, like settings saves) and atomic, so a
//! crash mid-write leaves the previous file rather than a truncated one.

use serde::Serialize;
use serde::de::DeserializeOwned;

use octa::ui::settings::AppSettings;

/// Read `<config_dir>/<file>`, or the default when there is nothing usable.
pub(crate) fn load<T: DeserializeOwned + Default>(file: &str) -> T {
    AppSettings::config_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(file)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write `value` as pretty JSON to `<config_dir>/<file>`.
pub(crate) fn save<T: Serialize + ?Sized>(file: &str, value: &T) {
    let Some(dir) = AppSettings::config_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let _ = octa::formats::write_atomically(&dir.join(file), |tmp| {
            std::fs::write(tmp, text)?;
            Ok(())
        });
    }
}
