//! A throwaway Git repository for the cell-history smoke tests, included by
//! path from both suites so neither pulls in the whole `common` module.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Three commits: create `p.csv`, raise the price of id 2 from 20 to 25,
/// rename to `prices.csv`. `None` when `git` is not installed, so a test
/// using it is a no-op there like the rest of the Git tests.
pub fn git_repo_with_price_change() -> Option<(tempfile::TempDir, PathBuf)> {
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
