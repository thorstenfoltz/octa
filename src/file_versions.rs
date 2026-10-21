//! Earlier versions of a file. Before a save overwrites a file, a copy of
//! what it replaces is kept, so it can be opened, compared or restored later
//! (File -> Earlier versions). Off by default; the GUI decides when to call
//! [`keep`] and, through [`Store`], where the copies go.
//!
//! In a shared folder (Octa's own, or one the user chose) the layout is
//! `<root>/<key>/<stamp>-<file name>`, where `key` is the first 16 hex digits
//! of the SHA-256 of the file's absolute path and `<key>/source.txt` holds
//! that path. Next to the file it is `.octa-versions/<file name>/<stamp>-<file
//! name>` (a subfolder) or `<stem>.<stamp>.<ext>` (right beside it). The
//! stamp is the replaced file's own modification time, `YYYYMMDD-HHMMSS-mmm`,
//! so names sort oldest first and the list can say "the version from 14:02"
//! without reading the copies. Every name keeps the extension, so a version
//! opens with the right reader like any other file.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// Length of the `YYYYMMDD-HHMMSS-mmm` stamp in every version name.
const STAMP_LEN: usize = 19;
const SOURCE_FILE: &str = "source.txt";
/// The folder [`Store::Subfolder`] puts beside a file.
pub const SUBFOLDER: &str = ".octa-versions";

/// Where versions are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    /// One folder for every file, Octa's own or one the user chose:
    /// `<root>/<key>/<stamp>-<name>`.
    Shared(PathBuf),
    /// Beside the file, out of the way: `.octa-versions/<name>/<stamp>-<name>`.
    Subfolder,
    /// Right beside the file: `<stem>.<stamp>.<ext>`.
    Beside,
}

impl Store {
    /// The folder holding `file`'s versions.
    pub fn dir(&self, file: &Path) -> PathBuf {
        match self {
            Self::Shared(root) => dir_for(root, file),
            Self::Subfolder => folder_of(file).join(SUBFOLDER).join(name_of(file)),
            Self::Beside => folder_of(file),
        }
    }

    /// The name of a version of `name` stamped `stamp`; `n` > 0 tells apart
    /// two versions with the same stamp.
    fn version_name(&self, name: &str, stamp: &str, n: u32) -> String {
        let n = if n == 0 {
            String::new()
        } else {
            format!("-{n}")
        };
        match (self, split_ext(name)) {
            (Self::Beside, (stem, Some(ext))) => format!("{stem}.{stamp}{n}.{ext}"),
            (Self::Beside, (_, None)) => format!("{name}.{stamp}{n}"),
            _ => format!("{stamp}{n}-{name}"),
        }
    }

    /// When the version called `entry` of the file called `name` was
    /// written, or `None` when `entry` is not one. A shared or sub folder
    /// holds nothing but that file's versions; beside it, every other file of
    /// the folder sits there too.
    fn stamp_of(&self, entry: &str, name: &str) -> Option<String> {
        if *self != Self::Beside {
            return parse_stamp(entry);
        }
        let (stem, ext) = split_ext(name);
        let rest = entry.strip_prefix(stem)?.strip_prefix('.')?;
        let stamp = rest.get(..STAMP_LEN)?;
        let tail = &rest[STAMP_LEN..];
        let tail = match tail.strip_prefix('-') {
            Some(t) if t.starts_with(|c: char| c.is_ascii_digit()) => {
                t.trim_start_matches(|c: char| c.is_ascii_digit())
            }
            _ => tail,
        };
        let matches = match ext {
            Some(ext) => tail.strip_prefix('.') == Some(ext),
            None => tail.is_empty(),
        };
        if matches { parse_stamp(stamp) } else { None }
    }

    /// `file`'s versions, oldest first.
    fn versions_of(&self, file: &Path) -> Vec<Version> {
        let name = name_of(file);
        versions_in(&self.dir(file), |entry| self.stamp_of(entry, &name))
    }

    /// Drop what `limits` no longer allows among the versions this store
    /// sees from `file`: every file's in a shared folder or in the subfolder
    /// beside it, only `file`'s own right beside it.
    fn prune_for(&self, file: &Path, limits: Limits) -> anyhow::Result<()> {
        match self {
            Self::Shared(root) => prune(root, limits),
            Self::Subfolder => {
                let root = folder_of(file).join(SUBFOLDER);
                prune(&root, limits)?;
                // Only goes when empty: no `.octa-versions` left behind for nothing.
                let _ = std::fs::remove_dir(&root);
                Ok(())
            }
            Self::Beside => prune_lists(vec![self.versions_of(file)], limits),
        }
    }
}

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
pub fn keep(store: &Store, file: &Path, limits: Limits) -> anyhow::Result<Option<PathBuf>> {
    let Ok(meta) = std::fs::metadata(file) else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let dir = store.dir(file);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating the versions folder {}", dir.display()))?;
    // A shared folder names a file's versions by a hash of its path; this
    // says which file that was. Beside the file the folder says it already.
    if matches!(store, Store::Shared(_)) {
        std::fs::write(
            dir.join(SOURCE_FILE),
            absolute(file).to_string_lossy().as_bytes(),
        )
        .with_context(|| format!("writing {}", dir.join(SOURCE_FILE).display()))?;
    }

    let modified: chrono::DateTime<chrono::Local> = meta
        .modified()
        .map(Into::into)
        .unwrap_or_else(|_| chrono::Local::now());
    let stamp = modified.format("%Y%m%d-%H%M%S-%3f").to_string();
    let name = name_of(file);
    let mut target = dir.join(store.version_name(&name, &stamp, 0));
    let mut n = 1u32;
    while target.exists() {
        target = dir.join(store.version_name(&name, &stamp, n));
        n += 1;
    }
    std::fs::copy(file, &target)
        .with_context(|| format!("copying {} to {}", file.display(), target.display()))?;
    store.prune_for(file, limits)?;
    Ok(Some(target))
}

/// The versions kept of `file` in `store`, newest first.
pub fn list(store: &Store, file: &Path) -> Vec<Version> {
    let mut out = store.versions_of(file);
    out.reverse();
    out
}

/// The versions of `file` in any of `stores`, newest first: versions kept
/// before the place was changed stay reachable.
pub fn list_all(stores: &[Store], file: &Path) -> Vec<Version> {
    let mut out: Vec<Version> = stores.iter().flat_map(|s| s.versions_of(file)).collect();
    out.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| b.path.cmp(&a.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Put `version` back as `file`. What `file` holds now is kept as a version
/// in `store` first, so a restore can be undone the same way.
pub fn restore(store: &Store, version: &Path, file: &Path, limits: Limits) -> anyhow::Result<()> {
    // Limits apply only once the version is back: pruning while keeping the
    // current file could otherwise delete the very version being restored
    // (the oldest, with a per-file limit already reached).
    keep(store, file, Limits::default())?;
    crate::formats::write_atomically(file, |tmp| {
        std::fs::copy(version, tmp)
            .with_context(|| format!("copying {} back", version.display()))?;
        Ok(())
    })?;
    store.prune_for(file, limits)
}

/// Drop versions under the shared `root` until `limits` hold: per file
/// first, then the oldest across all files while the total is too big, never
/// the newest of a file. Folders left empty go too.
pub fn prune(root: &Path, limits: Limits) -> anyhow::Result<()> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    let dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|d| d.is_dir())
        .collect();
    prune_lists(
        dirs.iter().map(|d| versions_in(d, parse_stamp)).collect(),
        limits,
    )?;
    for dir in dirs {
        if versions_in(&dir, parse_stamp).is_empty() {
            // Best effort: a folder we cannot remove costs a few bytes.
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(())
}

/// [`prune`] over one list of versions per file, each oldest first.
fn prune_lists(mut files: Vec<Vec<Version>>, limits: Limits) -> anyhow::Result<()> {
    if let Some(max) = limits.per_file {
        let max = max.max(1) as usize;
        for versions in &mut files {
            while versions.len() > max {
                remove(&versions.remove(0).path)?;
            }
        }
    }
    if let Some(cap) = limits.total_bytes {
        // Every version but each file's newest may go, oldest first.
        let mut removable: Vec<&Version> =
            files.iter().flat_map(|v| v.iter().rev().skip(1)).collect();
        // `when` has whole seconds; the name's stamp has the milliseconds.
        removable.sort_by(|a, b| {
            a.when
                .cmp(&b.when)
                .then_with(|| file_name(&a.path).cmp(file_name(&b.path)))
        });
        let mut total: u64 = files.iter().flatten().map(|v| v.size).sum();
        for v in removable {
            if total <= cap {
                break;
            }
            remove(&v.path)?;
            total = total.saturating_sub(v.size);
        }
    }
    Ok(())
}

/// The version copies in `dir`, oldest first; `stamp` names the entries
/// that are versions and when they were written.
fn versions_in(dir: &Path, stamp: impl Fn(&str) -> Option<String>) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let when = stamp(file_name(&path))?;
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

fn name_of(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folder `file` sits in, made absolute so a bare `t.csv` has one.
fn folder_of(file: &Path) -> PathBuf {
    absolute(file)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// `sales.csv` -> (`sales`, `csv`); a name with no extension, or only a
/// leading dot (`.env`), has none.
fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    }
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
