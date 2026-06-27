//! Thin wrappers over the `git` CLI. We shell out rather than depend on a git
//! crate: it is binary-safe (`show` returns raw bytes), needs no new
//! dependency, and every function degrades to `None`/`Err` when `git` is
//! missing or the file is untracked. No panics.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One commit touching a file, for the revision picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub rel_time: String,
}

/// Repository root containing `path`, or `None` if not in a git work tree.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

/// `path` relative to `root`, forward-slashed (git wants POSIX separators).
pub fn relative_path(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Init a temp repo with one committed file, then a working-tree edit.
    /// Returns None (test skips) if `git` is unavailable on the runner.
    fn temp_repo() -> Option<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().ok()?;
        let root = dir.path();
        let run = |args: &[&str]| Command::new("git").arg("-C").arg(root).args(args).output();
        run(&["init", "-q"]).ok()?.status.success().then_some(())?;
        run(&["config", "user.email", "t@example.com"]).ok()?;
        run(&["config", "user.name", "Test"]).ok()?;
        // Don't inherit the user's global commit.gpgsign: signing needs an
        // interactive pinentry that is unavailable on CI / sandboxed runners.
        run(&["config", "commit.gpgsign", "false"]).ok()?;
        let file = root.join("data.csv");
        fs::write(&file, "a,b\n1,2\n").ok()?;
        run(&["add", "data.csv"]).ok()?;
        run(&["commit", "-q", "-m", "initial commit"]).ok()?;
        // Working-tree edit (uncommitted).
        fs::write(&file, "a,b\n1,2\n3,4\n").ok()?;
        Some((dir, file))
    }

    #[test]
    fn repo_root_finds_toplevel() {
        let Some((dir, file)) = temp_repo() else {
            return; // git not available; skip
        };
        let root = repo_root(&file).expect("should find repo root");
        assert_eq!(
            fs::canonicalize(&root).unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn recent_commits_lists_the_commit() {
        let Some((_dir, file)) = temp_repo() else {
            return;
        };
        let root = repo_root(&file).unwrap();
        let rel = relative_path(&file, &root).unwrap();
        let commits = recent_commits(&root, &rel, 20);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "initial commit");
        assert!(!commits[0].sha.is_empty());
    }

    #[test]
    fn show_at_head_returns_committed_bytes() {
        let Some((_dir, file)) = temp_repo() else {
            return;
        };
        let root = repo_root(&file).unwrap();
        let rel = relative_path(&file, &root).unwrap();
        let bytes = show_at(&root, "HEAD", &rel).unwrap();
        // HEAD has the original 2-row file, not the working-tree 3-row edit.
        assert_eq!(String::from_utf8_lossy(&bytes), "a,b\n1,2\n");
    }
}
