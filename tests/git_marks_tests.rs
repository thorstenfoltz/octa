//! Integration tests for `octa::git::marks`: one throwaway repository per
//! test, built through the `git` CLI, so what is asserted is what git
//! actually reports.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use octa::git::marks::{
    MarksOptions, Status, branch_changed, collect, parse_porcelain, repo_root_of, resolve_base,
};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repo with one commit on `master` holding `keep.csv` and `data.csv`.
/// Returns `None` when `git` is not installed, so the test skips.
fn repo() -> Option<(TempDir, PathBuf)> {
    Command::new("git").arg("--version").output().ok()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "master"]);
    git(&root, &["config", "user.email", "t@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    // The user's global commit.gpgsign would need a pinentry.
    git(&root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("keep.csv"), "a\n1\n").unwrap();
    fs::write(root.join("data.csv"), "a,b\n1,2\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    Some((dir, root))
}

fn all() -> MarksOptions {
    MarksOptions {
        uncommitted: true,
        branch: true,
        base: "master".to_string(),
    }
}

#[test]
fn every_working_tree_state_gets_its_status() {
    let Some((_dir, root)) = repo() else { return };
    fs::write(root.join("data.csv"), "a,b\n1,2\n3,4\n").unwrap(); // modified
    fs::write(root.join("new.csv"), "x\n").unwrap();
    git(&root, &["add", "new.csv"]); // added
    fs::remove_file(root.join("keep.csv")).unwrap(); // deleted
    fs::write(root.join("loose.txt"), "u\n").unwrap(); // untracked
    fs::write(root.join("ignored.txt"), "i\n").unwrap(); // ignored

    let marks = collect(&root, &all());
    let st = |name: &str| marks.file(&root.join(name)).and_then(|m| m.uncommitted);
    assert_eq!(st("data.csv"), Some(Status::Modified));
    assert_eq!(st("new.csv"), Some(Status::Added));
    assert_eq!(st("keep.csv"), Some(Status::Deleted));
    assert_eq!(st("loose.txt"), Some(Status::Untracked));
    assert_eq!(st("ignored.txt"), None, "ignored files are never marked");
    assert_eq!(st(".gitignore"), None, "a clean tracked file is not marked");
}

#[test]
fn a_rename_marks_the_new_path() {
    let Some((_dir, root)) = repo() else { return };
    git(&root, &["mv", "keep.csv", "kept.csv"]);
    let marks = collect(&root, &all());
    assert_eq!(
        marks
            .file(&root.join("kept.csv"))
            .and_then(|m| m.uncommitted),
        Some(Status::Renamed)
    );
    assert!(marks.file(&root.join("keep.csv")).is_none());
}

#[test]
fn an_untracked_directory_marks_everything_beneath_it() {
    let Some((_dir, root)) = repo() else { return };
    fs::create_dir_all(root.join("fresh/deeper")).unwrap();
    fs::write(root.join("fresh/deeper/one.csv"), "1\n").unwrap();
    let marks = collect(&root, &all());
    // git lists `fresh/` once; the files under it are not enumerated.
    assert_eq!(
        marks
            .file(&root.join("fresh/deeper/one.csv"))
            .and_then(|m| m.uncommitted),
        Some(Status::Untracked)
    );
    assert!(
        marks
            .dir(&root.join("fresh"))
            .is_some_and(|d| d.uncommitted)
    );
    assert!(
        marks
            .dir(&root.join("fresh/deeper"))
            .is_some_and(|d| d.uncommitted)
    );
    // The repository root itself aggregates everything beneath it, so the
    // topmost row of the sidebar is marked too.
    assert!(marks.dir(&root).is_some_and(|d| d.uncommitted));
}

#[test]
fn branch_commits_since_the_fork_are_branch_changed_and_nothing_else_is() {
    let Some((_dir, root)) = repo() else { return };
    git(&root, &["checkout", "-q", "-b", "feature"]);
    fs::write(root.join("data.csv"), "a,b\n9,9\n").unwrap();
    git(&root, &["commit", "-q", "-am", "change data on feature"]);
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("sub/extra.csv"), "e\n").unwrap();
    git(&root, &["add", "sub/extra.csv"]);
    git(&root, &["commit", "-q", "-m", "add extra"]);

    let marks = collect(&root, &all());
    assert_eq!(marks.base.as_deref(), Some("master"));
    assert!(!marks.base_missing);
    let br = |name: &str| {
        marks
            .file(&root.join(name))
            .is_some_and(|m| m.branch_changed)
    };
    assert!(br("data.csv"));
    assert!(br("sub/extra.csv"));
    assert!(!br("keep.csv"), "untouched on the branch");
    // Committed, so not uncommitted.
    assert_eq!(
        marks.file(&root.join("data.csv")).unwrap().uncommitted,
        None
    );
    // Folder propagation carries the branch mark too.
    assert!(
        marks
            .dir(&root.join("sub"))
            .is_some_and(|d| d.branch_changed)
    );
}

#[test]
fn on_the_base_branch_itself_nothing_is_branch_changed() {
    let Some((_dir, root)) = repo() else { return };
    assert!(branch_changed(&root, "master").is_empty());
}

#[test]
fn the_base_falls_back_to_main_and_then_reports_missing() {
    let Some((_dir, root)) = repo() else { return };
    assert_eq!(resolve_base(&root, "master").as_deref(), Some("master"));
    assert_eq!(resolve_base(&root, "trunk"), None, "no trunk, no main");
    git(&root, &["branch", "main"]);
    assert_eq!(resolve_base(&root, "trunk").as_deref(), Some("main"));

    let marks = collect(
        &root,
        &MarksOptions {
            uncommitted: true,
            branch: true,
            base: "nowhere".to_string(),
        },
    );
    assert_eq!(marks.base.as_deref(), Some("main"));
    git(&root, &["branch", "-D", "main"]);
    let marks = collect(
        &root,
        &MarksOptions {
            uncommitted: true,
            branch: true,
            base: "nowhere".to_string(),
        },
    );
    assert!(marks.base_missing);
    assert!(marks.base.is_none());
}

#[test]
fn folder_marks_cover_every_ancestor_up_to_the_root_and_no_further() {
    let Some((_dir, root)) = repo() else { return };
    fs::create_dir_all(root.join("a/b/c")).unwrap();
    fs::write(root.join("a/b/c/deep.csv"), "d\n").unwrap();
    git(&root, &["add", "a/b/c/deep.csv"]);
    let marks = collect(&root, &all());
    for d in ["a", "a/b", "a/b/c"] {
        assert!(
            marks.dir(&root.join(d)).is_some_and(|m| m.uncommitted),
            "{d}"
        );
    }
    assert!(
        marks.dir(&root.join("a/b/c/deep.csv")).is_none(),
        "a file is not a dir"
    );
    let above = root.parent().unwrap();
    assert!(marks.dir(above).is_none(), "nothing above the repo root");
}

#[test]
fn switched_off_marks_are_not_collected() {
    let Some((_dir, root)) = repo() else { return };
    fs::write(root.join("data.csv"), "a,b\n1,2\n3,4\n").unwrap();
    let marks = collect(
        &root,
        &MarksOptions {
            uncommitted: false,
            branch: true,
            base: "master".to_string(),
        },
    );
    assert!(marks.file(&root.join("data.csv")).is_none());
}

#[test]
fn repo_root_is_found_from_a_nested_directory_and_not_outside() {
    let Some((dir, root)) = repo() else { return };
    fs::create_dir_all(root.join("x/y")).unwrap();
    assert_eq!(repo_root_of(&root.join("x/y")), Some(root.clone()));
    assert_eq!(repo_root_of(&root), Some(root.clone()));
    // A separate temp dir with no .git anywhere above it.
    let outside = tempfile::tempdir().unwrap();
    assert!(repo_root_of(outside.path()).is_none());
    drop(dir);
}

#[test]
fn a_worktree_git_file_counts_as_a_root() {
    let Some((_dir, root)) = repo() else { return };
    let wt = tempfile::tempdir().unwrap();
    let wt_path = wt.path().join("wt");
    git(&root, &["worktree", "add", "-q", wt_path.to_str().unwrap()]);
    assert!(wt_path.join(".git").is_file(), "a worktree has a .git FILE");
    assert_eq!(repo_root_of(&wt_path), Some(wt_path.clone()));
}

#[test]
fn porcelain_parsing_handles_z_renames_and_dirs() {
    let root = PathBuf::from("/r");
    // -z output: `XY path\0` and for renames `R  new\0old\0`.
    let bytes = b" M a.csv\0?? dir/\0R  new.csv\0old.csv\0A  added.csv\0D  gone.csv\0";
    let (files, dirs): (HashMap<PathBuf, Status>, Vec<PathBuf>) = parse_porcelain(&root, bytes);
    assert_eq!(files.get(&root.join("a.csv")), Some(&Status::Modified));
    assert_eq!(files.get(&root.join("new.csv")), Some(&Status::Renamed));
    assert!(!files.contains_key(&root.join("old.csv")));
    assert_eq!(files.get(&root.join("added.csv")), Some(&Status::Added));
    assert_eq!(files.get(&root.join("gone.csv")), Some(&Status::Deleted));
    assert_eq!(dirs, vec![root.join("dir")]);
}

#[test]
fn a_repository_without_commits_degrades_to_no_branch_mark() {
    let Some(_) = Command::new("git").arg("--version").output().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "master"]);
    fs::write(root.join("first.csv"), "a\n1\n").unwrap();
    // HEAD is unborn: no ref to compare against, and every git call that
    // needs one fails. The uncommitted mark must still work.
    assert_eq!(resolve_base(&root, "master"), None);
    assert!(branch_changed(&root, "master").is_empty());
    let marks = collect(&root, &all());
    assert!(marks.base_missing, "no base branch exists yet");
    assert_eq!(
        marks
            .file(&root.join("first.csv"))
            .and_then(|m| m.uncommitted),
        Some(Status::Untracked)
    );
}
