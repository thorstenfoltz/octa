//! Opening a saved API endpoint as a tab.
//!
//! The same shape as `DbOpenResult::MetadataReady`: a worker thread produces a
//! whole `DataTable`, pushes it onto a slot, and the UI thread drains the slot
//! once per frame and builds a detached tab from it. No path, no reader, no
//! format registry - `FormatReader` takes a `&Path`, and an endpoint is not one.

use std::sync::{Arc, Mutex};

use octa::api::{ApiConnection, client};
use octa::i18n::t;
use octa::ui::settings::api_secrets;

use super::state::OctaApp;

/// Which endpoint a tab came from, so Refresh knows what to re-run. The
/// two-field mirror of `CloudOrigin`.
#[derive(Debug, Clone)]
pub(crate) struct ApiOrigin {
    pub(crate) conn_id: String,
    pub(crate) path: Option<String>,
}

/// A finished fetch, waiting for the UI thread.
pub(crate) enum ApiOpenResult {
    Ready {
        table: Box<octa::data::DataTable>,
        label: String,
        origin: ApiOrigin,
        /// Set when the read stopped on a limit rather than the last page.
        note: Option<String>,
    },
    Failed(String),
}

pub(crate) type ApiOpenSlot = Arc<Mutex<Vec<ApiOpenResult>>>;

impl OctaApp {
    /// Fetch `conn` on a worker thread and open the result as a tab.
    pub(crate) fn start_api_fetch(
        &mut self,
        conn: ApiConnection,
        path: Option<String>,
        ctx: &egui::Context,
    ) {
        let secret = api_secrets::get_api_secret(&conn.id, &self.settings);
        let slot = self.api_pending_open.clone();
        let ctx = ctx.clone();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let label = conn.tab_label(path.as_deref());
        let origin = ApiOrigin {
            conn_id: conn.id.clone(),
            path: path.clone(),
        };
        self.status_message = Some((
            t("api.fetching").replace("{name}", &conn.name),
            std::time::Instant::now(),
        ));

        std::thread::spawn(move || {
            let opts = client::FetchOptions {
                path,
                max_rows: None,
                max_pages: None,
            };
            let item = match client::fetch_table(&conn, secret.as_deref(), &opts, &cancel) {
                Ok(out) => {
                    let note = out.capped.then(|| {
                        t("api.capped")
                            .replace("{pages}", &out.pages.to_string())
                            .replace("{rows}", &out.table.row_count().to_string())
                    });
                    ApiOpenResult::Ready {
                        table: Box::new(out.table),
                        label,
                        origin,
                        note,
                    }
                }
                // `{e:#}` not `{e}`: anyhow's plain Display shows only the
                // outermost context, which would drop the server's own reason.
                Err(e) => ApiOpenResult::Failed(
                    t("api.fetch_failed").replace("{error}", &format!("{e:#}")),
                ),
            };
            if let Ok(mut q) = slot.lock() {
                q.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Place any finished fetch. Called once per frame from the update loop.
    pub(crate) fn drain_api_pending_open(&mut self) {
        let drained: Vec<ApiOpenResult> = {
            let Ok(mut q) = self.api_pending_open.lock() else {
                return;
            };
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };
        for item in drained {
            match item {
                ApiOpenResult::Ready {
                    table,
                    label,
                    origin,
                    note,
                } => {
                    let mut new_tab =
                        super::state::TabState::new(self.settings.default_search_mode);
                    new_tab.table = *table;
                    new_tab.custom_tab_label = Some(label);
                    self.take_reload_slot(&super::refresh::api_source(
                        &origin.conn_id,
                        origin.path.as_deref(),
                    ));
                    new_tab.api_origin = Some(origin);
                    self.push_result_tab(new_tab);
                    // A partial read says so: a truncated table that looks
                    // complete is the failure mode worth shouting about.
                    if let Some(note) = note {
                        self.status_message = Some((note, std::time::Instant::now()));
                    }
                }
                ApiOpenResult::Failed(msg) => {
                    self.status_message = Some((msg, std::time::Instant::now()));
                }
            }
        }
    }
}
