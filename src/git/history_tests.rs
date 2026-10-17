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
