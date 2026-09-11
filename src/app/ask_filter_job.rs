//! The "Ask" path: send the search box's sentence to a chat profile and turn
//! the answer into ordinary filters on the active tab.
//!
//! One request, no tools, no agent loop (see `chat::ask_filter`). The network
//! call runs on a worker thread and the update loop drains the slot, the same
//! shape every other network job in the app uses.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::app::chat::ask_filter::{self, AskResult};
use crate::app::chat::providers;
use crate::app::state::{AskFilterJob, OctaApp};

impl OctaApp {
    /// Fire one request for the active tab's search text. Refuses politely
    /// rather than silently when there is nothing to ask or nobody to ask.
    pub(crate) fn start_ask_filter(&mut self, ctx: &eframe::egui::Context) {
        if self.ask_filter_job.is_some() {
            return; // one at a time; the spinner is already up
        }
        let tab_idx = self.active_tab;
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        let question = tab.search_text.trim().to_string();
        if question.is_empty() || tab.table.col_count() == 0 {
            return;
        }
        let profile_id = tab.search_ask_profile.clone();
        let Some(profile) = self
            .settings
            .chat_profiles
            .iter()
            .find(|p| p.id == profile_id)
            .or_else(|| self.settings.chat_profiles.first())
            .cloned()
        else {
            self.status_message = Some((
                octa::i18n::t("search.ask_needs_profile"),
                std::time::Instant::now(),
            ));
            return;
        };

        let api_key =
            crate::app::chat_panel::profile_api_key(&profile, &self.settings).unwrap_or_default();
        let cfg = providers::config_for_profile(
            &profile,
            &match profile.kind {
                octa::ui::settings::ChatProviderKind::Ollama => {
                    self.settings.chat_ollama_url.clone()
                }
                _ => self.settings.chat_base_url.clone(),
            },
            api_key,
            if self.settings.chat_max_tokens_unlimited {
                None
            } else {
                Some(self.settings.chat_max_tokens)
            },
        );
        let provider_kind = profile.kind;

        // Snapshot the schema, not the table: the request only needs column
        // names, types and a row count.
        let columns = tab.table.columns.clone();
        let row_count = tab.table.row_count();

        let slot: Arc<Mutex<Option<Result<AskResult, String>>>> = Arc::new(Mutex::new(None));
        self.ask_filter_job = Some(AskFilterJob {
            tab_idx,
            result: Arc::clone(&slot),
        });
        let ctx = ctx.clone();
        // See `ask_sql_job`: the Ask boxes spend tokens the session meter was
        // never told about.
        let session = std::sync::Arc::clone(&self.chat.session);
        std::thread::spawn(move || {
            let provider = providers::make_provider(provider_kind);
            let cancel = AtomicBool::new(false);
            let mut usage = (0u32, 0u32);
            let outcome = ask_filter::ask(
                provider.as_ref(),
                &cfg,
                &columns,
                row_count,
                &question,
                &cancel,
                &mut usage,
            );
            if let Ok(mut s) = session.lock() {
                s.input_tokens = s.input_tokens.saturating_add(usage.0);
                s.output_tokens = s.output_tokens.saturating_add(usage.1);
            }
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Poll the in-flight request. Called once per frame from the update loop.
    pub(crate) fn drain_ask_filter(&mut self) {
        let Some(job) = &self.ask_filter_job else {
            return;
        };
        let Some(outcome) = job.result.lock().ok().and_then(|mut g| g.take()) else {
            return;
        };
        let tab_idx = job.tab_idx;
        self.ask_filter_job = None;

        match outcome {
            Ok(result) => self.apply_ask_result(tab_idx, result),
            Err(e) => {
                // Nothing is applied on failure: a half-understood sentence
                // must not half-filter the table.
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("search.ask_failed")),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Apply a parsed answer to the tab that asked. Every condition lands as
    /// an ordinary, editable filter, so a wrong guess is visible and
    /// repairable rather than magic.
    fn apply_ask_result(&mut self, tab_idx: usize, result: AskResult) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        let applied = result.predicates.len() + result.values.len();
        tab.predicate_filters = result.predicates;
        for (col, allowed) in result.values {
            tab.column_filters
                .insert(col, allowed.into_iter().collect());
        }
        if let Some((col, ascending)) = result.sort {
            tab.table.sort_rows_by_columns(&[(col, ascending)]);
        }
        // The sentence was a question, not a text search: clear the box so the
        // text matcher does not also hide rows.
        tab.search_text.clear();
        tab.search_nav.reset();
        tab.filter_dirty = true;
        self.status_message = Some((
            octa::i18n::t("search.ask_applied").replace("{n}", &applied.to_string()),
            std::time::Instant::now(),
        ));
    }
}
