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

/// A file reached through a symlinked folder, or named by a relative
/// path, is still found in its repository.
#[cfg(unix)]
#[test]
fn relative_path_resolves_symlinks_and_relative_paths() {
    let Some((dir, file)) = temp_repo() else {
        return;
    };
    let root = repo_root(&file).unwrap();
    let outer = tempfile::tempdir().unwrap();
    let link = outer.path().join("link");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    let via_link = link.join("data.csv");
    assert_eq!(
        relative_path(&via_link, &repo_root(&via_link).unwrap()).as_deref(),
        Some("data.csv")
    );
    let cwd = std::env::current_dir().unwrap();
    let relative = pathdiff(&file, &cwd);
    assert_eq!(relative_path(&relative, &root).as_deref(), Some("data.csv"));
}

/// `path` relative to `base` (both absolute), with `..` steps.
fn pathdiff(path: &Path, base: &Path) -> PathBuf {
    let (p, b): (Vec<_>, Vec<_>) = (path.components().collect(), base.components().collect());
    let common = p.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &p[common..] {
        out.push(c);
    }
    out
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
