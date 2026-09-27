//! Which files and folders under a repository carry uncommitted changes, and
//! which were changed on the current branch since it forked from a base
//! branch. The folder sidebar colours rows from this.
//!
//! Shells out to `git`, like the rest of this module: no new dependency, and
//! every failure degrades to "no marks" rather than an error in a sidebar.
//! Nothing here touches egui.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One file's uncommitted state, straight from `git status --porcelain`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl Status {
    /// The letter shown after the file name.
    pub fn badge(self) -> &'static str {
        match self {
            Status::Modified => "M",
            Status::Added => "A",
            Status::Deleted => "D",
            Status::Renamed => "R",
            Status::Untracked => "U",
        }
    }

    /// i18n key of the hover text naming this state in words.
    pub fn hint_key(self) -> &'static str {
        match self {
            Status::Modified => "git_marks.modified",
            Status::Added => "git_marks.added",
            Status::Deleted => "git_marks.deleted",
            Status::Renamed => "git_marks.renamed",
            Status::Untracked => "git_marks.untracked",
        }
    }
}

/// Marks on one file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileMark {
    pub uncommitted: Option<Status>,
    /// In `base...HEAD`: committed on this branch, not on the base.
    pub branch_changed: bool,
}

/// Marks on one folder: an aggregate of everything beneath it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirMark {
    pub uncommitted: bool,
    pub branch_changed: bool,
}

/// What to collect, from Settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarksOptions {
    pub uncommitted: bool,
    pub branch: bool,
    /// Configured base branch name; `main` is tried when it is missing.
    pub base: String,
}

/// Everything the sidebar needs for one repository, keyed by absolute path.
#[derive(Clone, Debug, Default)]
pub struct RepoMarks {
    pub root: PathBuf,
    pub files: HashMap<PathBuf, FileMark>,
    /// Every ancestor (inside `root`) of a marked file. Precomputed so a
    /// folder lookup is one hash probe, not a scan of every marked file.
    pub dirs: HashMap<PathBuf, DirMark>,
    /// Whole directories git reported as untracked (`?? dir/`). Their files
    /// are not enumerated; `file()` answers for them by prefix.
    pub untracked_dirs: Vec<PathBuf>,
    /// The base branch actually compared against.
    pub base: Option<String>,
    /// Neither the configured base nor `main` exists here.
    pub base_missing: bool,
}

impl RepoMarks {
    pub fn file(&self, path: &Path) -> Option<FileMark> {
        if let Some(m) = self.files.get(path) {
            return Some(*m);
        }
        self.untracked_dirs
            .iter()
            .any(|d| path.starts_with(d))
            .then_some(FileMark {
                uncommitted: Some(Status::Untracked),
                branch_changed: false,
            })
    }

    pub fn dir(&self, path: &Path) -> Option<DirMark> {
        if let Some(m) = self.dirs.get(path) {
            return Some(*m);
        }
        self.untracked_dirs
            .iter()
            .any(|d| path.starts_with(d))
            .then_some(DirMark {
                uncommitted: true,
                branch_changed: false,
            })
    }

    /// Fill `dirs` from `files` and `untracked_dirs`: every ancestor up to
    /// and including `root` inherits the marks of what lies beneath it.
    fn build_dirs(&mut self) {
        let root = self.root.clone();
        let mut dirs: HashMap<PathBuf, DirMark> = HashMap::new();
        let mut mark_ancestors = |path: &Path, uncommitted: bool, branch: bool| {
            let mut cur = path.parent();
            while let Some(d) = cur {
                if !d.starts_with(&root) {
                    break;
                }
                let e = dirs.entry(d.to_path_buf()).or_default();
                e.uncommitted |= uncommitted;
                e.branch_changed |= branch;
                if d == root {
                    break;
                }
                cur = d.parent();
            }
        };
        for (p, m) in &self.files {
            mark_ancestors(p, m.uncommitted.is_some(), m.branch_changed);
        }
        for d in &self.untracked_dirs {
            mark_ancestors(d, true, false);
        }
        for d in &self.untracked_dirs {
            dirs.entry(d.clone()).or_default().uncommitted = true;
        }
        self.dirs = dirs;
    }
}

/// The repository root containing `dir`: the nearest ancestor (or `dir`
/// itself) with a `.git` entry. A worktree or submodule has a `.git` *file*
/// rather than a directory, so this checks for either. Pure filesystem, no
/// process, cheap enough to run inline.
pub fn repo_root_of(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

fn run_git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Parse `git status --porcelain=v1 -z` output.
///
/// Entries are `XY path\0`. With `-z` a rename is `R  new\0old\0`: the new
/// path first, then the original, which is skipped. An untracked directory
/// arrives as `?? dir/` (trailing slash) and goes into the second return
/// value instead of the map.
pub fn parse_porcelain(root: &Path, bytes: &[u8]) -> (HashMap<PathBuf, Status>, Vec<PathBuf>) {
    let mut files = HashMap::new();
    let mut dirs = Vec::new();
    let mut entries = bytes.split(|&b| b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry[0] as char, entry[1] as char);
        let rel = String::from_utf8_lossy(&entry[3..]).to_string();
        let status = match (x, y) {
            ('?', '?') => Status::Untracked,
            ('!', '!') => continue,
            ('R', _) | (_, 'R') | ('C', _) | (_, 'C') => {
                // The original path follows as its own NUL-terminated entry.
                let _original = entries.next();
                if x == 'C' || y == 'C' {
                    Status::Added
                } else {
                    Status::Renamed
                }
            }
            ('D', _) | (_, 'D') => Status::Deleted,
            ('A', _) | (_, 'A') => Status::Added,
            _ => Status::Modified,
        };
        if status == Status::Untracked && rel.ends_with('/') {
            dirs.push(root.join(rel.trim_end_matches('/')));
        } else {
            files.insert(root.join(&rel), status);
        }
    }
    (files, dirs)
}

/// Uncommitted state of every file under `root`, plus whole untracked
/// directories. Empty on any git failure.
pub fn status_marks(root: &Path) -> (HashMap<PathBuf, Status>, Vec<PathBuf>) {
    match run_git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    ) {
        Some(bytes) => parse_porcelain(root, &bytes),
        None => (HashMap::new(), Vec::new()),
    }
}

/// Files committed on the current branch that `base` does not have:
/// `git diff --name-only base...HEAD`. Empty on any git failure.
pub fn branch_changed(root: &Path, base: &str) -> HashSet<PathBuf> {
    let Some(bytes) = run_git(
        root,
        &["diff", "--name-only", "-z", &format!("{base}...HEAD")],
    ) else {
        return HashSet::new();
    };
    bytes
        .split(|&b| b == 0)
        .filter(|e| !e.is_empty())
        .map(|e| root.join(String::from_utf8_lossy(e).as_ref()))
        .collect()
}

fn ref_exists(root: &Path, name: &str) -> bool {
    run_git(root, &["rev-parse", "--verify", "--quiet", name]).is_some()
}

/// The configured base if it exists, else `main`, else `None`.
pub fn resolve_base(root: &Path, configured: &str) -> Option<String> {
    let configured = configured.trim();
    let mut candidates = Vec::new();
    if !configured.is_empty() {
        candidates.push(configured);
    }
    if configured != "main" {
        candidates.push("main");
    }
    candidates
        .into_iter()
        .find(|name| ref_exists(root, name))
        .map(str::to_string)
}

/// Everything the sidebar needs for one repository. Only the pieces switched
/// on in `options` cost a git call.
pub fn collect(root: &Path, options: &MarksOptions) -> RepoMarks {
    let mut marks = RepoMarks {
        root: root.to_path_buf(),
        ..Default::default()
    };
    if options.uncommitted {
        let (files, dirs) = status_marks(root);
        for (path, status) in files {
            marks.files.entry(path).or_default().uncommitted = Some(status);
        }
        marks.untracked_dirs = dirs;
    }
    if options.branch {
        match resolve_base(root, &options.base) {
            Some(base) => {
                for path in branch_changed(root, &base) {
                    marks.files.entry(path).or_default().branch_changed = true;
                }
                marks.base = Some(base);
            }
            None => marks.base_missing = true,
        }
    }
    marks.build_dirs();
    marks
}

/// What the tree renderer asks. Implemented by the app's cache, so the
/// renderer stays free of app types and of any git call.
pub trait MarksLookup {
    fn file(&self, path: &Path) -> Option<FileMark>;
    fn dir(&self, path: &Path) -> Option<DirMark>;
    /// Whether `dir` has had its repository resolved yet. The renderer
    /// reports unresolved directories back so the app can resolve them
    /// after the frame.
    fn is_probed(&self, dir: &Path) -> bool;
    /// Hover text for the root row when the base branch could not be found
    /// in the repository containing `root`.
    fn base_missing_note(&self, root: &Path) -> Option<String>;
}
