//! A file's history, commit by commit, as tables: the data side of Cell
//! history. Shells out to `git` like the rest of this module, follows
//! renames, and reads every revision through the normal format registry, so
//! any format kept in Git works, not only CSV.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::data::DataTable;
use crate::data::cell_history::{CommitInfo, Version};

/// One commit that touched the file, with the path the file had then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRevision {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub path: String,
}

/// Up to `n` commits touching `relpath` after skipping `skip`, newest first,
/// following renames.
pub fn file_history(root: &Path, relpath: &str, skip: usize, n: usize) -> Vec<FileRevision> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            "--follow",
            // Not `--skip`: git skips before `--follow` sees the rename, so
            // every page past a rename came back empty. Fetch and drop.
            // ponytail: re-reads `skip` log entries per page; fine for the
            // tens of commits a page shows.
            &format!("-n{}", skip + n),
            "--date=format:%Y-%m-%d %H:%M",
            "--format=%x1e%h%x1f%s%x1f%an%x1f%ad",
            "--name-only",
            "--",
            relpath,
        ])
        .output();
    let Ok(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .split('\u{1e}')
        .filter_map(|chunk| {
            let mut lines = chunk.lines();
            let mut f = lines.next()?.split('\u{1f}');
            let sha = f.next()?.to_string();
            if sha.is_empty() {
                return None;
            }
            let subject = f.next().unwrap_or("").to_string();
            let author = f.next().unwrap_or("").to_string();
            let date = f.next().unwrap_or("").to_string();
            let path = lines
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or(relpath)
                .to_string();
            Some(FileRevision {
                sha,
                subject,
                author,
                date,
                path,
            })
        })
        .skip(skip)
        .collect()
}

/// `relpath` as it was at `rev`. Readers pick their format by name, so the
/// bytes go into a temp folder under the file's own name (which keeps
/// `.csv.gz` and `access.log.1` recognisable).
pub fn read_table_at(root: &Path, rev: &str, relpath: &str, cap: u64) -> anyhow::Result<DataTable> {
    let bytes = super::show_at(root, rev, relpath)?;
    let name = Path::new(relpath)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let dir = tempfile::tempdir()?;
    let tmp = dir.path().join(name);
    std::fs::write(&tmp, bytes)?;
    crate::formats::read_table_auto(&tmp, None, cap)
}

/// Whether the file on disk differs from HEAD (or is not in HEAD at all).
pub fn working_copy_differs(root: &Path, relpath: &str, path: &Path) -> bool {
    match super::show_at(root, "HEAD", relpath) {
        Ok(head) => std::fs::read(path)
            .map(|disk| disk != head)
            .unwrap_or(false),
        Err(_) => true,
    }
}

/// One page of a file's history as tables.
pub struct LoadedVersions {
    pub versions: Vec<Version>,
    /// Revisions that could not be read (the file had another format then,
    /// say) and why, so the dialog can say so instead of skipping silently.
    pub unreadable: Vec<(CommitInfo, String)>,
    /// Whether older commits exist beyond this page.
    pub more: bool,
}

/// Committed versions `skip..skip + n` of `path`, newest first.
pub fn load_versions(
    path: &Path,
    skip: usize,
    n: usize,
    cap: u64,
) -> anyhow::Result<LoadedVersions> {
    let root = super::repo_root(path)
        .ok_or_else(|| anyhow::anyhow!("{} is not in a Git repository", path.display()))?;
    let rel = super::relative_path(path, &root)
        .ok_or_else(|| anyhow::anyhow!("{} is outside its repository", path.display()))?;
    let mut revs = file_history(&root, &rel, skip, n + 1);
    let more = revs.len() > n;
    revs.truncate(n);
    let mut versions = Vec::new();
    let mut unreadable = Vec::new();
    for r in revs {
        let commit = CommitInfo {
            sha: r.sha.clone(),
            subject: r.subject,
            author: r.author,
            date: r.date,
            path: r.path.clone(),
        };
        match read_table_at(&root, &r.sha, &r.path, cap) {
            Ok(table) => versions.push(Version { commit, table }),
            Err(e) => unreadable.push((commit, format!("{e:#}"))),
        }
    }
    Ok(LoadedVersions {
        versions,
        unreadable,
        more,
    })
}

/// `load_versions(path, 0, depth, cap)` with the file on disk in front when
/// it differs from HEAD, for the headless surfaces.
pub fn versions_with_working_copy(
    path: &Path,
    depth: usize,
    cap: u64,
) -> anyhow::Result<LoadedVersions> {
    let mut loaded = load_versions(path, 0, depth, cap)?;
    if let Some((root, rel)) = locate(path)
        && working_copy_differs(&root, &rel, path)
    {
        let table = crate::formats::read_table_auto(path, None, cap)?;
        let commit = CommitInfo {
            subject: "(not committed yet)".into(),
            ..Default::default()
        };
        loaded.versions.insert(0, Version { commit, table });
    }
    Ok(loaded)
}

/// Root and path helpers the dialog needs once, bundled.
pub fn locate(path: &Path) -> Option<(PathBuf, String)> {
    locate_or_why(path).ok()
}

/// [`locate`], with the reason when the file is not found in a repository.
pub fn locate_or_why(path: &Path) -> Result<(PathBuf, String), String> {
    let root = super::repo_root_or_why(path)?;
    let rel = super::relative_path(path, &root).ok_or_else(|| {
        format!(
            "{} is not inside the repository at {}",
            path.display(),
            root.display()
        )
    })?;
    Ok((root, rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    /// Three commits: create, change a price, rename the file.
    fn repo() -> Option<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().ok()?;
        let root = dir.path();
        let run = |args: &[&str]| Command::new("git").arg("-C").arg(root).args(args).output();
        run(&["init", "-q"]).ok()?.status.success().then_some(())?;
        run(&["config", "user.email", "t@example.com"]).ok()?;
        run(&["config", "user.name", "Tess"]).ok()?;
        run(&["config", "commit.gpgsign", "false"]).ok()?;
        fs::write(root.join("p.csv"), "id,price\n1,10\n2,20\n").ok()?;
        run(&["add", "."]).ok()?;
        run(&["commit", "-q", "-m", "create"]).ok()?;
        fs::write(root.join("p.csv"), "id,price\n1,10\n2,25\n").ok()?;
        run(&["commit", "-qam", "raise price"]).ok()?;
        run(&["mv", "p.csv", "prices.csv"]).ok()?;
        run(&["commit", "-qm", "rename"]).ok()?;
        let file = root.join("prices.csv");
        Some((dir, file))
    }

    #[test]
    fn history_follows_renames_and_reads_old_paths() {
        let Some((_d, file)) = repo() else { return };
        let root = crate::git::repo_root(&file).unwrap();
        let rel = crate::git::relative_path(&file, &root).unwrap();
        let revs = file_history(&root, &rel, 0, 10);
        assert_eq!(revs.len(), 3);
        assert_eq!(revs[0].subject, "rename");
        assert_eq!(revs[2].path, "p.csv");
        assert_eq!(revs[1].author, "Tess");
        let old = read_table_at(&root, &revs[2].sha, &revs[2].path, u64::MAX).unwrap();
        assert_eq!(old.row_count(), 2);
    }

    #[test]
    fn load_versions_pages_and_reports_more() {
        let Some((_d, file)) = repo() else { return };
        let first = load_versions(&file, 0, 2, u64::MAX).unwrap();
        assert_eq!(first.versions.len(), 2);
        assert!(first.more);
        let rest = load_versions(&file, 2, 2, u64::MAX).unwrap();
        assert_eq!(rest.versions.len(), 1);
        assert!(!rest.more);
    }
}
