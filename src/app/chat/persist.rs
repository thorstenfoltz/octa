//! Session persistence: one pretty-printed JSON file per session under
//! `<config_dir>/chat_sessions/<id>.json`. The neutral `Message` model already
//! derives Serde, so saving is a thin wrapper.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::settings::AppSettings;

use super::session::ChatSessionState;
use super::types::Message;

/// The on-disk shape of a saved session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    pub messages: Vec<Message>,
}

/// Lightweight metadata for the session list (no message bodies).
#[derive(Clone, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub updated_unix: u64,
}

/// `<config_dir>/chat_sessions`.
pub fn sessions_dir() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("chat_sessions"))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Build a `SavedSession` from the live state.
pub fn snapshot(state: &ChatSessionState) -> SavedSession {
    SavedSession {
        id: state.id.clone(),
        title: state.title.clone(),
        provider: state.provider_id.clone(),
        model: state.model.clone(),
        created_unix: now_unix(),
        updated_unix: now_unix(),
        messages: state.messages.clone(),
    }
}

/// Persist a session. No-op-safe: returns an error string the caller can log
/// but need not surface aggressively.
pub fn save(session: &SavedSession) -> Result<(), String> {
    let dir = sessions_dir().ok_or_else(|| "no config directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json", sanitize_id(&session.id)));
    let json =
        serde_json::to_string_pretty(session).map_err(|e| format!("serialise session: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Load a session by id.
pub fn load(id: &str) -> Result<SavedSession, String> {
    let dir = sessions_dir().ok_or_else(|| "no config directory".to_string())?;
    let path = dir.join(format!("{}.json", sanitize_id(id)));
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// List saved sessions, most recently updated first. Unreadable files are
/// skipped rather than failing the whole listing.
pub fn list() -> Vec<SessionMeta> {
    let Some(dir) = sessions_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut metas: Vec<SessionMeta> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(s) = serde_json::from_str::<SavedSession>(&text) {
            metas.push(SessionMeta {
                id: s.id,
                title: s.title,
                updated_unix: s.updated_unix,
            });
        }
    }
    metas.sort_by_key(|m| std::cmp::Reverse(m.updated_unix));
    metas
}

/// Delete a saved session. Missing files are not an error.
pub fn delete(id: &str) -> Result<(), String> {
    let Some(dir) = sessions_dir() else {
        return Ok(());
    };
    let path = dir.join(format!("{}.json", sanitize_id(id)));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete {}: {e}", path.display())),
    }
}

/// Delete every saved session. Returns how many files were removed. Best
/// effort: unreadable / locked files are skipped rather than aborting.
pub fn delete_all() -> usize {
    let Some(dir) = sessions_dir() else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// Keep the filename to the safe id charset we generate (`[0-9a-f-]`), so a
/// crafted id can never escape `chat_sessions/`.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}
