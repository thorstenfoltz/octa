//! Name search shared by the sidebar's Databases and Cloud trees.
//!
//! Two halves. Typing filters what the tree has already loaded, instantly and
//! on the UI thread ([`matches`], [`RowFilter`]). "Search all" (or Enter) asks
//! the tree's owner to walk the parts nobody has opened yet on a worker; the
//! worker reports into a [`SearchSlot`], which this module draws as a flat,
//! clickable result list ([`search_results`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Ui;

use crate::i18n::t;

/// The lowercased, trimmed query, or `None` when there is nothing to filter by.
pub fn needle(query: &str) -> Option<String> {
    let q = query.trim();
    (!q.is_empty()).then(|| q.to_lowercase())
}

/// Case-insensitive substring match. `needle` comes from [`needle`].
pub fn matches(name: &str, needle: &str) -> bool {
    name.to_lowercase().contains(needle)
}

/// How one folder-like row (a schema, a catalog, a cloud folder) fares under
/// the filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowFilter {
    /// Nothing about it or below it matches.
    Hide,
    /// Shown as usual. Either no filter is active or its own name matches, and
    /// in both cases everything under it is shown unfiltered.
    Show,
    /// Its name does not match but something already loaded below it does: it
    /// is drawn open, with the filter still applied to its children.
    OpenForMatch,
}

impl RowFilter {
    /// `below` is only asked when the row's own name does not match, because
    /// it walks the cached subtree.
    pub fn of(filter: Option<&str>, name: &str, below: impl FnOnce(&str) -> bool) -> Self {
        match filter {
            None => Self::Show,
            Some(n) if matches(name, n) => Self::Show,
            Some(n) if below(n) => Self::OpenForMatch,
            Some(_) => Self::Hide,
        }
    }

    /// The filter the row's children are drawn with.
    pub fn child_filter(self, filter: Option<&str>) -> Option<&str> {
        match self {
            Self::OpenForMatch => filter,
            Self::Show | Self::Hide => None,
        }
    }
}

/// Where a deep search stands.
#[derive(Debug, Clone, Default)]
pub enum TreeSearch<T> {
    #[default]
    Idle,
    Running,
    Done {
        hits: Vec<T>,
        /// The walk stopped at its cap after this many entries.
        stopped_at: Option<usize>,
    },
    Failed(String),
}

/// A deep search's state together with the query it answers. A worker only
/// writes its result while its run is still the current one, so a slow or
/// cancelled search can never overwrite a newer one.
#[derive(Debug, Clone)]
pub struct SearchSlot<T> {
    pub query: String,
    pub state: TreeSearch<T>,
    /// Raised to stop the running worker. Each search gets its own flag, so
    /// cancelling one can never stop the next.
    stop: Arc<AtomicBool>,
}

// Written out: a derive would demand `T: Default`, which a hit type has no
// reason to be.
impl<T> Default for SearchSlot<T> {
    fn default() -> Self {
        Self {
            query: String::new(),
            state: TreeSearch::Idle,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<T> SearchSlot<T> {
    /// Start a search for `query`, stopping any search still running. The
    /// returned flag is the new worker's: it checks it and gives up once it
    /// is raised.
    pub fn start(&mut self, query: &str) -> Arc<AtomicBool> {
        self.stop.store(true, Ordering::Relaxed);
        self.stop = Arc::new(AtomicBool::new(false));
        self.query = query.to_string();
        self.state = TreeSearch::Running;
        self.stop.clone()
    }

    /// Stop the running search and drop its state. The worker quits at its
    /// next check, and whatever it would still report is ignored because the
    /// slot no longer answers its query.
    pub fn cancel(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.query.clear();
        self.state = TreeSearch::Idle;
    }

    /// Store a worker's result, unless its run was cancelled or replaced
    /// since. `run` is the flag [`Self::start`] handed that worker: the flag,
    /// not the query, names the run, so a cancelled search for "a" cannot land
    /// its late result in a fresh search for "a".
    pub fn finish(&mut self, run: &Arc<AtomicBool>, state: TreeSearch<T>) {
        if Arc::ptr_eq(&self.stop, run) && !run.load(Ordering::Relaxed) {
            self.state = state;
        }
    }

    /// Drop results that no longer answer what the search box says.
    pub fn forget_unless(&mut self, query: &str) {
        if self.query != query {
            self.cancel();
        }
    }
}

/// What the user did in the search box this frame.
#[derive(Debug, Default)]
pub struct SearchBoxAction {
    /// Enter or "Search all" with a non-empty query.
    pub search_all: bool,
}

/// The search row: a text box, a clear button and "Search all".
pub fn search_box(ui: &mut Ui, id_salt: &str, query: &mut String) -> SearchBoxAction {
    let has_query = !query.trim().is_empty();
    let mut action = SearchBoxAction::default();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let all = ui
                .add_enabled(has_query, egui::Button::new(t("treesearch.search_all")))
                .on_hover_text(t("treesearch.search_all_hint"))
                .on_disabled_hover_text(t("treesearch.hint"));
            if all.clicked() {
                action.search_all = true;
            }
            if has_query
                && ui
                    .small_button("x")
                    .on_hover_text(t("treesearch.clear_hint"))
                    .clicked()
            {
                query.clear();
            }
            let edit = ui
                .add(
                    egui::TextEdit::singleline(query)
                        .id_salt(id_salt)
                        .hint_text(t("treesearch.placeholder"))
                        .desired_width(ui.available_width()),
                )
                .on_hover_text(t("treesearch.hint"));
            if has_query && edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                action.search_all = true;
            }
        });
    });
    action
}

/// Height of the result list before it scrolls.
const RESULTS_MAX_HEIGHT: f32 = 180.0;

/// Draw a deep search's state under the search box. `row` draws one hit (and
/// reports its click through whatever the caller captured). Returns true when
/// the user pressed Cancel on a running search.
pub fn search_results<T>(
    ui: &mut Ui,
    id_salt: &str,
    search: &TreeSearch<T>,
    mut row: impl FnMut(&mut Ui, &T),
) -> bool {
    let mut cancel = false;
    match search {
        TreeSearch::Idle => {}
        TreeSearch::Running => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(t("treesearch.searching"));
                cancel = ui
                    .small_button(t("common.cancel"))
                    .on_hover_text(t("treesearch.cancel_hint"))
                    .clicked();
            });
        }
        TreeSearch::Failed(msg) => {
            // Selectable: a connection error is worth copying.
            let colour = ui.visuals().error_fg_color;
            crate::ui::message::selectable_message(ui, colour, msg);
        }
        TreeSearch::Done { hits, stopped_at } => {
            let weak = ui.visuals().weak_text_color();
            if hits.is_empty() {
                ui.label(
                    egui::RichText::new(t("treesearch.none"))
                        .small()
                        .color(weak),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        t("treesearch.matches").replace("{n}", &hits.len().to_string()),
                    )
                    .small()
                    .strong(),
                );
            }
            if let Some(n) = stopped_at {
                ui.label(
                    egui::RichText::new(t("treesearch.truncated").replace("{n}", &n.to_string()))
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
            if !hits.is_empty() {
                let row_height =
                    ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
                egui::ScrollArea::vertical()
                    .id_salt(id_salt)
                    .max_height(RESULTS_MAX_HEIGHT)
                    .auto_shrink([false, true])
                    .show_rows(ui, row_height, hits.len(), |ui, range| {
                        for hit in &hits[range] {
                            row(ui, hit);
                        }
                    });
            }
            ui.separator();
        }
    }
    cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_ignores_case_and_surrounding_space() {
        let n = needle("  Sales ").unwrap();
        assert!(matches("fact_SALES_2024", &n));
        assert!(!matches("orders", &n));
        assert_eq!(needle("   "), None);
    }

    #[test]
    fn a_row_opens_only_for_a_match_below_it() {
        let f = Some("sales");
        assert_eq!(RowFilter::of(None, "public", |_| false), RowFilter::Show);
        assert_eq!(RowFilter::of(f, "sales_mart", |_| false), RowFilter::Show);
        assert_eq!(
            RowFilter::of(f, "public", |_| true),
            RowFilter::OpenForMatch
        );
        assert_eq!(RowFilter::of(f, "public", |_| false), RowFilter::Hide);
        // A folder whose own name matches shows its contents unfiltered.
        assert_eq!(RowFilter::Show.child_filter(f), None);
        assert_eq!(RowFilter::OpenForMatch.child_filter(f), f);
    }

    fn done(hit: u8) -> TreeSearch<u8> {
        TreeSearch::Done {
            hits: vec![hit],
            stopped_at: None,
        }
    }

    #[test]
    fn a_stale_search_never_overwrites_a_newer_one() {
        let mut slot: SearchSlot<u8> = SearchSlot::default();
        let old = slot.start("old");
        let new = slot.start("new");
        slot.finish(&old, done(1));
        assert!(matches!(slot.state, TreeSearch::Running));
        slot.finish(&new, done(2));
        assert!(matches!(slot.state, TreeSearch::Done { ref hits, .. } if hits == &[2]));
        // Editing the box drops results that answer something else.
        slot.forget_unless("newer");
        assert!(matches!(slot.state, TreeSearch::Idle));
    }

    #[test]
    fn cancel_stops_the_worker_and_only_that_worker() {
        let mut slot: SearchSlot<u8> = SearchSlot::default();
        let first = slot.start("a");
        slot.cancel();
        assert!(first.load(Ordering::Relaxed));
        assert!(matches!(slot.state, TreeSearch::Idle));
        // Searching the same text again: the stopped worker's late result
        // must not land in the new run.
        let again = slot.start("a");
        slot.finish(&first, done(1));
        assert!(matches!(slot.state, TreeSearch::Running));
        assert!(!again.load(Ordering::Relaxed));
        // Starting another search stops the one before it.
        let next = slot.start("b");
        assert!(again.load(Ordering::Relaxed));
        assert!(!next.load(Ordering::Relaxed));
    }
}
