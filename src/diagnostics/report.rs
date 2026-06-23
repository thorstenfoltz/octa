//! The shareable debug report. `redact()` and `build_report()` are pure and
//! tested; `export_debug_report()` gathers the inputs, redacts them, writes the
//! file to the config directory, and returns its path. Secrets are stripped and
//! home directory / username are masked so the file is safe to post publicly.

use std::io;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ui::settings::AppSettings;

use super::crash;

/// Metadata header for the report.
pub struct ReportMeta {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub store_packaged: bool,
    pub locale: String,
    pub theme: String,
    pub timestamp: String,
}

/// Mask the user's home directory (-> `~`), username (-> `%USER%`), and any
/// API-key-shaped token. Defence in depth: the settings key map is also blanked
/// separately before this runs.
pub fn redact(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(h) = home
        && !h.is_empty()
    {
        out = out.replace(h, "~");
    }
    if let Some(u) = user
        && !u.is_empty()
    {
        out = out.replace(u, "%USER%");
    }
    let key_re = Regex::new(r"(?i)sk-[a-z0-9_-]{20,}").unwrap();
    key_re.replace_all(&out, "sk-REDACTED").into_owned()
}

/// Assemble the report text from already-redacted parts.
pub fn build_report(
    meta: &ReportMeta,
    settings_toml: &str,
    log_tail: &str,
    last_crash: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str("# Octa debug report\n\n");
    s.push_str(&format!("Generated:      {}\n", meta.timestamp));
    s.push_str(&format!("Version:        {}\n", meta.version));
    s.push_str(&format!("OS / Arch:      {} / {}\n", meta.os, meta.arch));
    s.push_str(&format!("Store-packaged: {}\n", meta.store_packaged));
    s.push_str(&format!("Locale:         {}\n", meta.locale));
    s.push_str(&format!("Theme:          {}\n\n", meta.theme));
    if let Some(c) = last_crash {
        s.push_str("## Last crash\n\n");
        s.push_str(c.trim_end());
        s.push_str("\n\n");
    }
    s.push_str("## Settings (redacted)\n\n");
    s.push_str(settings_toml.trim_end());
    s.push_str("\n\n## Log tail (redacted)\n\n");
    s.push_str(log_tail.trim_end());
    s.push('\n');
    s
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
}

fn username() -> Option<String> {
    std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("USERNAME").ok())
}

/// Read the last `max_bytes` of a file as UTF-8 (lossy), or "" if absent.
fn read_tail(path: &Path, max_bytes: u64) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(max_bytes as usize);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// Build the report and write it to `config_dir/logs/octa-debug-<timestamp>.txt`.
/// Returns the written path.
pub fn export_debug_report(settings: &AppSettings) -> io::Result<PathBuf> {
    let dir = super::logs_dir().ok_or_else(|| io::Error::other("no config directory"))?;
    std::fs::create_dir_all(&dir)?;

    // Blank the API-key map before serialising, then redact the rest.
    let mut redacted_settings = settings.clone();
    redacted_settings.chat_api_keys.clear();
    let settings_toml = toml::to_string_pretty(&redacted_settings).unwrap_or_default();

    let home = home_dir();
    let user = username();
    let settings_red = redact(&settings_toml, home.as_deref(), user.as_deref());

    let log_tail_raw = read_tail(&dir.join("octa.log"), 256 * 1024);
    let log_tail = redact(&log_tail_raw, home.as_deref(), user.as_deref());

    let last_crash = crash::take_last_crash().map(|c| redact(&c, home.as_deref(), user.as_deref()));

    let meta = ReportMeta {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        store_packaged: crate::platform::is_store_packaged(),
        locale: settings.language.clone(),
        theme: format!("{:?}", settings.default_theme),
        timestamp: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };

    let report = build_report(&meta, &settings_red, &log_tail, last_crash.as_deref());
    let file_ts = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let path = dir.join(format!("octa-debug-{file_ts}.txt"));
    std::fs::write(&path, report)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_masks_home_user_and_keys() {
        let text = "path /home/alice/data.csv key sk-ABCDEFGHIJKLMNOPQRSTUVWX user alice";
        let out = redact(text, Some("/home/alice"), Some("alice"));
        assert!(out.contains('~'), "home should be masked: {out}");
        assert!(out.contains("%USER%"), "user should be masked: {out}");
        assert!(out.contains("sk-REDACTED"), "key should be masked: {out}");
        assert!(!out.contains("alice"), "username leaked: {out}");
        assert!(
            !out.contains("ABCDEFGHIJKLMNOPQRSTUVWX"),
            "key leaked: {out}"
        );
    }

    #[test]
    fn build_report_includes_meta_and_crash() {
        let meta = ReportMeta {
            version: "9.9.9".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            store_packaged: false,
            locale: "en".into(),
            theme: "Light".into(),
            timestamp: "2026-06-20T10:00:00".into(),
        };
        let out = build_report(&meta, "chat_api_keys = {}", "all good", Some("boom at x"));
        assert!(out.contains("9.9.9"));
        assert!(out.contains("linux"));
        assert!(out.contains("## Last crash"));
        assert!(out.contains("boom at x"));
        assert!(out.contains("all good"));
    }
}
