//! Earlier versions of a file. Before a save overwrites a file, a copy of
//! what it replaces is kept in Octa's own folder, so it can be opened,
//! compared or restored later (File -> Earlier versions). Off by default;
//! the GUI decides when to call [`keep`].
//!
//! Layout: `<config>/versions/<key>/<stamp>-<file name>`, where `key` is the
//! first 16 hex digits of the SHA-256 of the file's absolute path and
//! `<key>/source.txt` holds that path. The stamp is the replaced file's own
//! modification time, `YYYYMMDD-HHMMSS-mmm`, so names sort oldest first and
//! the list can say "the version from 14:02" without reading the copies.
//! Keeping the file name after the stamp keeps the extension, so a version
//! opens with the right reader like any other file.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// Length of the `YYYYMMDD-HHMMSS-mmm` stamp that starts every version name.
const STAMP_LEN: usize = 19;
const SOURCE_FILE: &str = "source.txt";

/// How many versions may stay. `None` = no limit of that kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Versions per file; the oldest go first.
    pub per_file: Option<u32>,
    /// Bytes for all versions of all files together. The newest version of
    /// each file is never deleted for it, so one huge file still has one.
    pub total_bytes: Option<u64>,
}

/// One kept version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub path: PathBuf,
    /// When the replaced file was last written, as local time
    /// (`YYYY-MM-DD HH:MM:SS`).
    pub when: String,
    pub size: u64,
}

/// `<config>/versions`, or `None` without a config directory.
pub fn default_root() -> Option<PathBuf> {
    crate::ui::settings::AppSettings::config_dir().map(|d| d.join("versions"))
}

/// The folder holding `file`'s versions under `root`.
pub fn dir_for(root: &Path, file: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let abs = absolute(file);
    let digest = Sha256::digest(abs.to_string_lossy().as_bytes());
    let key: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    root.join(key)
}

/// Copy `file` into its versions folder, then drop what `limits` no longer
/// allows. `Ok(None)` when there is no file to keep (a Save As to a new
/// name). Returns the kept copy.
pub fn keep(root: &Path, file: &Path, limits: Limits) -> anyhow::Result<Option<PathBuf>> {
    let Ok(meta) = std::fs::metadata(file) else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let dir = dir_for(root, file);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating the versions folder {}", dir.display()))?;
    std::fs::write(
        dir.join(SOURCE_FILE),
        absolute(file).to_string_lossy().as_bytes(),
    )
    .with_context(|| format!("writing {}", dir.join(SOURCE_FILE).display()))?;

    let modified: chrono::DateTime<chrono::Local> = meta
        .modified()
        .map(Into::into)
        .unwrap_or_else(|_| chrono::Local::now());
    let stamp = modified.format("%Y%m%d-%H%M%S-%3f").to_string();
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut target = dir.join(format!("{stamp}-{name}"));
    let mut n = 1u32;
    while target.exists() {
        target = dir.join(format!("{stamp}-{n}-{name}"));
        n += 1;
    }
    std::fs::copy(file, &target)
        .with_context(|| format!("copying {} to {}", file.display(), target.display()))?;
    prune(root, limits)?;
    Ok(Some(target))
}

/// The versions kept of `file`, newest first.
pub fn list(root: &Path, file: &Path) -> Vec<Version> {
    let mut out = version_files(&dir_for(root, file));
    out.reverse();
    out
}

/// Put `version` back as `file`. What `file` holds now is kept as a version
/// first, so a restore can be undone the same way.
pub fn restore(root: &Path, version: &Path, file: &Path, limits: Limits) -> anyhow::Result<()> {
    // Limits apply only once the version is back: pruning while keeping the
    // current file could otherwise delete the very version being restored
    // (the oldest, with a per-file limit already reached).
    keep(root, file, Limits::default())?;
    crate::formats::write_atomically(file, |tmp| {
        std::fs::copy(version, tmp)
            .with_context(|| format!("copying {} back", version.display()))?;
        Ok(())
    })?;
    prune(root, limits)
}

/// Drop versions until `limits` hold: per file first, then the oldest across
/// all files while the total is too big, never the newest of a file. Folders
/// left empty go too.
pub fn prune(root: &Path, limits: Limits) -> anyhow::Result<()> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    // (versions oldest first) per file folder.
    let mut folders: Vec<(PathBuf, Vec<Version>)> = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let mut versions = version_files(&dir);
        if let Some(max) = limits.per_file {
            let max = max.max(1) as usize;
            while versions.len() > max {
                remove(&versions.remove(0).path)?;
            }
        }
        folders.push((dir, versions));
    }
    if let Some(cap) = limits.total_bytes {
        // Every version but each folder's newest may go, oldest first. The
        // stamp sorts by time across folders too.
        let mut removable: Vec<Version> = folders
            .iter()
            .flat_map(|(_, v)| v.iter().rev().skip(1).cloned())
            .collect();
        removable.sort_by(|a, b| file_name(&a.path).cmp(file_name(&b.path)));
        let mut total: u64 = folders
            .iter()
            .flat_map(|(_, v)| v.iter())
            .map(|v| v.size)
            .sum();
        for v in removable {
            if total <= cap {
                break;
            }
            remove(&v.path)?;
            total = total.saturating_sub(v.size);
        }
    }
    for (dir, _) in folders {
        if version_files(&dir).is_empty() {
            // Best effort: a folder we cannot remove costs a few bytes.
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(())
}

/// The version copies in one folder, oldest first.
fn version_files(dir: &Path) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = file_name(&path).to_string();
            let when = parse_stamp(&name)?;
            let size = e.metadata().ok().filter(|m| m.is_file())?.len();
            Some(Version { path, when, size })
        })
        .collect();
    out.sort_by(|a, b| file_name(&a.path).cmp(file_name(&b.path)));
    out
}

/// `20261007-140211-123-name` -> `2026-10-07 14:02:11`. `None` for anything
/// that is not a version copy (`source.txt`).
fn parse_stamp(name: &str) -> Option<String> {
    let stamp = name.get(..STAMP_LEN)?;
    let t = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S-%3f").ok()?;
    Some(t.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

fn remove(path: &Path) -> anyhow::Result<()> {
    std::fs::remove_file(path).with_context(|| format!("removing old version {}", path.display()))
}

/// The path as an absolute one, resolving links when the file exists, so the
/// same file always lands in the same folder.
fn absolute(file: &Path) -> PathBuf {
    std::fs::canonicalize(file)
        .or_else(|_| std::path::absolute(file))
        .unwrap_or_else(|_| file.to_path_buf())
}

#[cfg(test)]
#[path = "file_versions_tests.rs"]
mod tests;
