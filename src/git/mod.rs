//! Thin wrappers over the `git` CLI. We shell out rather than depend on a git
//! crate: it is binary-safe (`show` returns raw bytes), needs no new
//! dependency, and every function degrades to `None`/`Err` when `git` is
//! missing or the file is untracked. No panics.

use std::path::{Path, PathBuf};
use std::process::Command;

pub mod history;
pub mod marks;

/// One commit touching a file, for the revision picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub rel_time: String,
}

/// Repository root containing `path`, or `None` if not in a git work tree.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    repo_root_or_why(path).ok()
}

/// [`repo_root`], with git's own reason when there is none (not a
/// repository, `git` missing, a repository git refuses as unsafe), for a
/// greyed-out control to show.
pub fn repo_root_or_why(path: &Path) -> Result<PathBuf, String> {
    let dir = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| format!("{} has no folder", path.display()))?
    };
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if root.is_empty() {
        Err("git named no repository root".into())
    } else {
        Ok(PathBuf::from(root))
    }
}

/// `path` relative to `root`, forward-slashed (git wants POSIX separators).
///
/// `git rev-parse --show-toplevel` answers with the *real* path, so a file
/// opened through a symlinked folder, or by a relative path (`octa data.csv`
/// from a terminal), never starts with it. Both sides are resolved before
/// giving up; without that every Git feature said "not in a repository" for
/// such a file.
pub fn relative_path(path: &Path, root: &Path) -> Option<String> {
    let resolved;
    let rel = match path.strip_prefix(root) {
        Ok(rel) => rel,
        Err(_) => {
            let real_root = std::fs::canonicalize(root).ok()?;
            resolved = std::fs::canonicalize(path).ok()?;
            resolved.strip_prefix(&real_root).ok()?
        }
    };
    let s = rel.to_string_lossy().replace('\\', "/");
    if s.is_empty() { None } else { Some(s) }
}

/// Up to `n` commits that touched `relpath`, newest first.
pub fn recent_commits(root: &Path, relpath: &str, n: usize) -> Vec<Commit> {
    // %h short-sha, %s subject, %cr committer relative date, unit-separated.
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            &format!("-n{n}"),
            "--format=%h%x1f%s%x1f%cr",
            "--",
            relpath,
        ])
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            let sha = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").to_string();
            let rel_time = parts.next().unwrap_or("").to_string();
            if sha.is_empty() {
                None
            } else {
                Some(Commit {
                    sha,
                    subject,
                    rel_time,
                })
            }
        })
        .collect()
}

/// Raw bytes of `relpath` at revision `rev` (e.g. "HEAD", a short SHA).
/// Bytes, not String, so binary formats (Parquet, etc.) round-trip.
pub fn show_at(root: &Path, rev: &str, relpath: &str) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("{rev}:{relpath}")])
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("git show {rev}:{relpath} failed: {}", err.trim());
    }
    Ok(out.stdout)
}

/// The commit HEAD points at, or `None` outside a repository.
pub fn head_sha(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
