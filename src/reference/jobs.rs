//! Downloads that run in the background: started at launch for what is due,
//! from Settings' Download now, and when a switch is turned on. The app and
//! the Settings section share one handle.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use super::{Dataset, ReferenceSettings, due, source, store};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) running: BTreeSet<Dataset>,
    last_error: BTreeMap<Dataset, String>,
    fresh_errors: Vec<String>,
    finished: Vec<Dataset>,
    repaint: Option<egui::Context>,
}

impl State {
    fn fail(&mut self, d: Dataset, e: &anyhow::Error) {
        let msg = format!("{}: {e:#}", crate::i18n::t(&d.label_key()));
        self.last_error.insert(d, msg.clone());
        self.fresh_errors.push(msg);
    }
}

#[derive(Clone, Default)]
pub struct Jobs(pub(crate) Arc<Mutex<State>>);

impl Jobs {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_repaint(&self, ctx: egui::Context) {
        self.lock().repaint = Some(ctx);
    }

    /// Start a refresh of `d`; false when it is already running or the data
    /// folder cannot be found (that error is reported like a failed download).
    pub fn start(&self, d: Dataset, countries: Vec<String>) -> bool {
        match store::dir() {
            Ok(dir) => self.start_in(
                dir,
                std::env::var(super::download::BASE_ENV).ok(),
                d,
                countries,
            ),
            Err(e) => {
                self.lock().fail(d, &e);
                false
            }
        }
    }

    pub fn start_in(
        &self,
        dir: PathBuf,
        base: Option<String>,
        d: Dataset,
        countries: Vec<String>,
    ) -> bool {
        if !self.lock().running.insert(d) {
            return false;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let result = source(d).refresh_with(&dir, &countries, base.as_deref());
            let mut s = me.lock();
            s.running.remove(&d);
            match result {
                Ok(_) => {
                    s.last_error.remove(&d);
                    s.finished.push(d);
                }
                Err(e) => s.fail(d, &e),
            }
            if let Some(ctx) = &s.repaint {
                ctx.request_repaint();
            }
        });
        true
    }

    /// Start every switched-on dataset whose data is due.
    pub fn start_due(&self, r: &ReferenceSettings) -> Vec<Dataset> {
        let Ok(dir) = store::dir() else {
            return Vec::new();
        };
        let now = SystemTime::now();
        Dataset::ALL
            .into_iter()
            .filter(|d| r.get(*d).enabled && due(*d, &dir, r.get(*d), now))
            .filter(|d| self.start(*d, Vec::new()))
            .collect()
    }

    pub fn running(&self, d: Dataset) -> bool {
        self.lock().running.contains(&d)
    }

    pub fn busy(&self) -> bool {
        !self.lock().running.is_empty()
    }

    pub fn last_error(&self, d: Dataset) -> Option<String> {
        self.lock().last_error.get(&d).cloned()
    }

    /// New errors since the last call, each "<name>: <error>".
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().fresh_errors)
    }

    pub fn take_finished(&self) -> Vec<Dataset> {
        std::mem::take(&mut self.lock().finished)
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
