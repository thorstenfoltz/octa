//! The sidebar's git marks: which repositories the opened folder touches,
//! their marks, and when to ask git again.
//!
//! The library half (`octa::git::marks`) answers "what is the state of this
//! repository right now". This half decides *when* to ask, keeps the answer,
//! and makes sure the question is never asked on the frame path: every
//! collection runs on a worker thread and lands through a slot the update
//! loop drains, the same shape every other long job in the app uses.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use octa::git::marks::{
    DirMark, FileMark, MarksLookup, MarksOptions, RepoMarks, collect, repo_root_of,
};

use crate::app::state::OctaApp;

pub(crate) struct GitMarksCache {
    /// Latest marks per repository root.
    repos: HashMap<PathBuf, RepoMarks>,
    /// Directory -> its repository root (`None` = not inside any repository).
    /// Memoised because the tree asks for every directory it draws, every
    /// frame.
    dir_repo: HashMap<PathBuf, Option<PathBuf>>,
    /// Roots with a collection in flight; a second request for the same root
    /// is dropped rather than queued, so a slow repository cannot pile up.
    in_flight: HashSet<PathBuf>,
    results: Arc<Mutex<Vec<RepoMarks>>>,
    /// Options the current marks were collected with. A change in Settings
    /// is detected by comparing, so no hook into the dialog is needed.
    options: Option<MarksOptions>,
    last_tick: Instant,
    was_focused: bool,
    /// A file Octa just saved; its repository refreshes on the next tick,
    /// once the write has landed.
    saved: Option<PathBuf>,
}

impl GitMarksCache {
    pub(crate) fn new() -> Self {
        Self {
            repos: HashMap::new(),
            dir_repo: HashMap::new(),
            in_flight: HashSet::new(),
            results: Arc::new(Mutex::new(Vec::new())),
            options: None,
            last_tick: Instant::now(),
            was_focused: true,
            saved: None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.repos.is_empty() && self.dir_repo.is_empty()
    }

    /// Forget everything. Used when the marks are switched off or the folder
    /// closes; the next open starts clean.
    pub(crate) fn clear(&mut self) {
        self.repos.clear();
        self.dir_repo.clear();
        self.in_flight.clear();
        // Collections still running push into the old slot, which nothing
        // reads again: their answer describes a folder we have closed.
        self.results = Arc::new(Mutex::new(Vec::new()));
    }

    /// Note a file Octa wrote; see [`Self::saved`].
    pub(crate) fn note_saved(&mut self, path: &Path) {
        self.saved = Some(path.to_path_buf());
    }

    /// Resolve `dir`'s repository, memoised. Returns the root when there is one.
    fn probe(&mut self, dir: &Path) -> Option<PathBuf> {
        if let Some(root) = self.dir_repo.get(dir) {
            return root.clone();
        }
        let root = repo_root_of(dir);
        self.dir_repo.insert(dir.to_path_buf(), root.clone());
        root
    }

    /// Start collecting `root` on a worker unless one is already at it.
    fn refresh(&mut self, root: &Path, options: &MarksOptions, ctx: &eframe::egui::Context) {
        if !self.in_flight.insert(root.to_path_buf()) {
            return;
        }
        let root = root.to_path_buf();
        let options = options.clone();
        let slot = Arc::clone(&self.results);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let marks = collect(&root, &options);
            if let Ok(mut r) = slot.lock() {
                r.push(marks);
            }
            ctx.request_repaint();
        });
    }

    fn refresh_all(&mut self, options: &MarksOptions, ctx: &eframe::egui::Context) {
        let roots: Vec<PathBuf> = self.repos.keys().cloned().collect();
        for root in roots {
            self.refresh(&root, options, ctx);
        }
    }

    /// Directories the tree drew without a known repository: resolve them,
    /// and collect any repository seen for the first time.
    pub(crate) fn probe_and_refresh(
        &mut self,
        dirs: Vec<PathBuf>,
        options: &MarksOptions,
        ctx: &eframe::egui::Context,
    ) {
        for dir in dirs {
            if let Some(root) = self.probe(&dir)
                && !self.repos.contains_key(&root)
                && !self.in_flight.contains(&root)
            {
                // Placeholder so a second directory of the same repository in
                // the same frame does not start a second collection.
                self.repos.insert(
                    root.clone(),
                    RepoMarks {
                        root: root.clone(),
                        ..Default::default()
                    },
                );
                self.refresh(&root, options, ctx);
            }
        }
    }

    /// Take finished collections. Once per frame.
    fn drain(&mut self) {
        let done: Vec<RepoMarks> = match self.results.lock() {
            Ok(mut r) => r.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        for marks in done {
            self.in_flight.remove(&marks.root);
            self.repos.insert(marks.root.clone(), marks);
        }
    }

    fn repo_for(&self, path: &Path) -> Option<&RepoMarks> {
        let dir = path.parent()?;
        let root = self.dir_repo.get(dir)?.as_ref()?;
        self.repos.get(root)
    }
}

impl MarksLookup for GitMarksCache {
    fn file(&self, path: &Path) -> Option<FileMark> {
        self.repo_for(path)?.file(path)
    }

    fn dir(&self, path: &Path) -> Option<DirMark> {
        // A directory row is looked up through its own memo entry when it has
        // one (it was drawn expanded before), else through its parent's.
        let root = match self.dir_repo.get(path) {
            Some(r) => r.as_ref()?,
            None => self.dir_repo.get(path.parent()?)?.as_ref()?,
        };
        self.repos.get(root)?.dir(path)
    }

    fn is_probed(&self, dir: &Path) -> bool {
        self.dir_repo.contains_key(dir)
    }

    fn base_missing_note(&self, root: &Path) -> Option<String> {
        let repo_root = self.dir_repo.get(root)?.as_ref()?;
        let marks = self.repos.get(repo_root)?;
        marks.base_missing.then(|| {
            octa::i18n::t("git_marks.base_missing").replace(
                "{base}",
                &self
                    .options
                    .as_ref()
                    .map(|o| o.base.clone())
                    .unwrap_or_else(|| "master".to_string()),
            )
        })
    }
}

impl OctaApp {
    /// Once per frame: drain finished collections, then decide whether to ask
    /// git again. Never asks while the sidebar is closed or both marks are
    /// off; then the cache is emptied so a re-open starts fresh.
    pub(crate) fn tick_git_marks(&mut self, ctx: &eframe::egui::Context) {
        let options = self.settings.git_marks_options();
        let enabled = options.uncommitted || options.branch;
        if !enabled || self.directory_tree.is_none() {
            if !self.git_marks.is_empty() {
                self.git_marks.clear();
            }
            self.git_marks.options = None;
            return;
        }
        self.git_marks.drain();

        // Settings changed: what was collected no longer answers the
        // question. Drop it and collect again with the new options.
        if self.git_marks.options.as_ref() != Some(&options) {
            self.git_marks.options = Some(options.clone());
            let roots: Vec<PathBuf> = self.git_marks.repos.keys().cloned().collect();
            for root in &roots {
                self.git_marks.repos.insert(
                    root.clone(),
                    RepoMarks {
                        root: root.clone(),
                        ..Default::default()
                    },
                );
            }
            self.git_marks.refresh_all(&options, ctx);
        }

        // A save Octa made: that file's repository, on the tick after the
        // write, so git sees the file as it is now.
        if let Some(saved) = self.git_marks.saved.take()
            && let Some(dir) = saved.parent()
            && let Some(root) = self.git_marks.probe(dir)
        {
            self.git_marks.refresh(&root, &options, ctx);
        }

        let secs = self.settings.git_marks_refresh_secs;
        if secs == 0 {
            // Open and save only, as agreed: no timer, no focus refresh.
            return;
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        let regained = focused && !self.git_marks.was_focused;
        self.git_marks.was_focused = focused;
        let period = Duration::from_secs(u64::from(secs));
        if regained || self.git_marks.last_tick.elapsed() >= period {
            self.git_marks.last_tick = Instant::now();
            self.git_marks.refresh_all(&options, ctx);
        }
        // The timer must fire without the user touching anything.
        ctx.request_repaint_after(period.saturating_sub(self.git_marks.last_tick.elapsed()));
    }
}
