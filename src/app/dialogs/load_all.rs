//! **Load whole table...**: count first, ask, then download every remaining
//! row into the tab.
//!
//! A live-database tab holds the pages it has loaded and a capped file holds
//! `initial_load_rows`; every analysis then answers about those rows only.
//! This is the explicit way out: one `COUNT(*)` so the user knows what they
//! are asking for, then the rest of the table.
//!
//! The download reuses the paging protocol the tab already has
//! (`bg_row_buffer` + the two flags, merged by `drain_background_rows`), so
//! write-back identities, the status line and the end-of-table handling come
//! for free. A file re-reads with the cap lifted, which is
//! `reload_without_row_cap`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;

use super::super::state::OctaApp;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

/// What the count worker hands back: `Ok(Some(n))` rows in the source,
/// `Ok(None)` a file whose reader cannot say without reading it all (CSV),
/// `Err` the server's refusal.
pub(crate) type CountSlot = Arc<Mutex<Option<Result<Option<usize>, String>>>>;

pub(crate) struct LoadAllState {
    pub(crate) tab_idx: usize,
    pub(crate) count: CountSlot,
    /// The finished count, moved out of the slot once it arrives.
    pub(crate) counted: Option<Result<Option<usize>, String>>,
}

/// Rows per round trip for the download. The user's page size is tuned for
/// scrolling (theirs is 100); a full download in pages that small is
/// thousands of round trips. Capped by the connector's own row cap, above
/// which a page would come back short and read as the end.
fn download_page(settings: &octa::ui::settings::AppSettings) -> usize {
    settings
        .db_page_size()
        .max(10_000)
        .min(octa::formats::initial_load_rows())
}

/// First cell of a one-row result as a count.
fn read_count(t: &octa::data::DataTable) -> Option<usize> {
    let col = t
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("n"))
        .unwrap_or(0);
    t.get(0, col)?.to_string().trim().parse::<usize>().ok()
}

/// The row count Load whole table confirms: the table's, or the filtered
/// result's when the tab is filtered on the server (it is what will load).
fn count_view_sql(
    conn: &octa::db::DbConnection,
    origin: &crate::app::state::DbOrigin,
    view: Option<&octa::db::pushdown::view::ServerView>,
) -> String {
    match view.and_then(|v| v.where_sql(conn.engine)) {
        Some(w) => {
            let mut src = crate::app::db_view::origin_source(conn, origin);
            src.filter = Some(w);
            format!("SELECT COUNT(*) AS n FROM {}", src.from_sql())
        }
        None => octa::db::count_sql(
            conn.engine,
            origin.catalog.as_deref(),
            &origin.schema,
            &origin.table,
        ),
    }
}

impl OctaApp {
    /// Entry from the Data menu, the tab menu, the shortcut and the fallback
    /// note. A no-op on a tab that already holds every row.
    pub(crate) fn open_load_all(&mut self, ctx: &egui::Context) {
        let tab_idx = self.active_tab;
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        if !tab.source_has_more() || tab.loading_all {
            return;
        }
        let slot: CountSlot = Arc::new(Mutex::new(None));
        match tab.db_origin.clone() {
            None => {
                if let Ok(mut g) = slot.lock() {
                    *g = Some(Ok(tab.table.known_total()));
                }
            }
            Some(origin) => {
                let Some(conn) = self.find_db_conn(&origin.conn_id) else {
                    return;
                };
                let settings = self.settings.clone();
                let cache = self.db_conn_cache.clone();
                let ctx = ctx.clone();
                let out = slot.clone();
                let sql = count_view_sql(&conn, &origin, tab.server_view.as_ref());
                std::thread::spawn(move || {
                    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                    let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                    let res = cache
                        .with_conn(&conn, secret.as_deref(), ssh.as_deref(), |c| c.query(&sql))
                        .map_err(|e| format!("{e:#}"))
                        .and_then(|t| {
                            read_count(&t)
                                .map(Some)
                                .ok_or_else(|| "the row count came back empty".to_string())
                        });
                    if let Ok(mut g) = out.lock() {
                        *g = Some(res);
                    }
                    ctx.request_repaint();
                });
            }
        }
        self.load_all = Some(LoadAllState {
            tab_idx,
            count: slot,
            counted: None,
        });
    }

    /// Download every row the tab does not hold yet.
    fn start_load_all(&mut self, ctx: &egui::Context, tab_idx: usize) {
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        if tab.view_task.is_some() {
            return;
        }
        let Some(origin) = tab.db_origin.clone() else {
            // A file: re-read it with the row cap lifted.
            self.active_tab = tab_idx;
            self.reload_without_row_cap();
            return;
        };
        let Some(conn) = self.find_db_conn(&origin.conn_id) else {
            return;
        };
        let skip = tab.table.row_offset + tab.table.row_count();
        let view = tab.server_view.clone();
        let page = download_page(&self.settings);
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let pending = self.db_browser.pending_open.clone();
        let label = format!("{} @ {}", origin.table, conn.name);
        let (load_finished, cancel_slot, cancelled) =
            self.begin_db_load(format!("{} {label}", t("loadall.loading")));

        let src = crate::app::db_view::origin_source(&conn, &origin);
        let identity = origin.identity.clone();
        // ponytail: the order at the start; a column moved during the
        // download lands the rest misplaced.
        let columns = crate::app::db_view::column_names(&self.tabs[tab_idx]);
        let tab = &mut self.tabs[tab_idx];
        let buffer = Arc::new(Mutex::new(Vec::<Vec<octa::data::CellValue>>::new()));
        let done = Arc::new(AtomicBool::new(false));
        let exhausted = Arc::new(AtomicBool::new(false));
        tab.bg_row_buffer = Some(buffer.clone());
        tab.bg_loading_done = done.clone();
        tab.bg_file_exhausted = exhausted.clone();
        tab.bg_can_load_more = false;
        tab.loading_all = true;

        let base = octa::db::select_all_sql(
            conn.engine,
            origin.catalog.as_deref(),
            &origin.schema,
            &origin.table,
        );
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _done = crate::app::flag_guard::FlagOnDrop::new(done, true);
            let _load = crate::app::flag_guard::FlagOnDrop::new(load_finished, true);
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
            let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
            // Outside the closure on purpose: `with_conn` reconnects and runs
            // the closure again once when a cached connection fails, and that
            // retry must resume after the rows already handed to the tab, not
            // fetch them a second time.
            let mut offset = skip;
            let read = cache.with_conn(&conn, secret.as_deref(), ssh.as_deref(), |c| {
                if let Ok(mut slot) = cancel_slot.lock() {
                    *slot = c.cancel_handle();
                }
                // `fetch_batches` always starts at row 0; this starts where
                // the tab stops. Same LIMIT/OFFSET ceiling as the scroll path.
                loop {
                    // `with_conn` retries once; after a Cancel that retry must
                    // not run the next page.
                    if cancelled.load(Ordering::Relaxed) {
                        anyhow::bail!("{}", t("db.load_cancelled"));
                    }
                    let sql = match &view {
                        Some(v) => crate::app::db_view::view_page_sql(
                            &src,
                            identity.as_ref(),
                            v,
                            page,
                            offset,
                        ),
                        None => octa::db::paged_sql(conn.engine, &base, page, offset),
                    };
                    let batch = c.query(&sql)?;
                    let n = batch.rows.len();
                    let rows = crate::app::db_view::rows_in_column_order(batch, &columns);
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend(rows);
                    }
                    repaint.request_repaint();
                    if n < page {
                        return Ok(());
                    }
                    offset += n;
                }
            });
            match read {
                // Only a finished read may say "that was the whole table".
                Ok(()) => exhausted.store(true, Ordering::Relaxed),
                Err(e) => {
                    if let Ok(mut p) = pending.lock() {
                        p.push(crate::app::db_browser::DbOpenResult::Failed(format!(
                            "{} {label}: {e:#}",
                            t("db.page_failed")
                        )));
                    }
                }
            }
            repaint.request_repaint();
        });
    }
}

pub(crate) fn render_load_all_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(st) = app.load_all.as_mut() else {
        return;
    };
    // The tab could have been closed while this was up.
    let Some(tab) = app.tabs.get(st.tab_idx) else {
        app.load_all = None;
        return;
    };
    if st.counted.is_none() {
        st.counted = st.count.lock().ok().and_then(|mut g| g.take());
    }
    let loaded = tab.table.row_offset + tab.table.row_count();
    // A database sort or filter running on this tab is another read too: the
    // download would page the view it is about to replace.
    let busy_elsewhere = app.db_load_job.is_some() || tab.view_task.is_some();

    let dialog_id = egui::Id::new("octa_load_all_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;
    let mut confirm = false;
    let mut cancel = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 220.0));
    let window = egui::Window::new("octa_load_all")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(220.0)
            .min_width(320.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("octa_load_all_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("loadall.title")).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let fmt = octa::ui::status_bar::format_number;
            match &st.counted {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t("loadall.counting"));
                    });
                }
                Some(Ok(Some(n))) => {
                    ui.label(
                        t("loadall.body")
                            .replace("{n}", &fmt(*n))
                            .replace("{loaded}", &fmt(loaded)),
                    );
                }
                Some(Ok(None)) => {
                    ui.label(t("loadall.body_unknown"));
                }
                Some(Err(e)) => {
                    let colour = ui.visuals().error_fg_color;
                    octa::ui::message::selectable_message(ui, colour, e);
                }
            }
            ui.add_space(12.0);
            octa::ui::control_row::control_row(ui, |ui| {
                let ready = matches!(st.counted, Some(Ok(_))) && !busy_elsewhere;
                let b = ui.add_enabled(ready, egui::Button::new(t("loadall.confirm")));
                if b.clicked() {
                    confirm = true;
                }
                if ready {
                    b.on_hover_text(t("loadall.confirm_hint"));
                } else {
                    b.on_disabled_hover_text(if busy_elsewhere {
                        t("loadall.busy_hint")
                    } else {
                        t("loadall.confirm_disabled_hint")
                    });
                }
                if ui
                    .button(t("common.cancel"))
                    .on_hover_text(t("loadall.cancel_hint"))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if confirm {
        let tab_idx = st.tab_idx;
        app.load_all = None;
        app.start_load_all(ctx, tab_idx);
    } else if cancel || chrome_close {
        app.load_all = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::{CellValue, ColumnInfo, DataTable};

    fn one_cell(name: &str, v: CellValue) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: name.into(),
            data_type: "Int64".into(),
        }];
        t.rows = vec![vec![v]];
        t
    }

    /// Snowflake hands the alias back as `N`; some drivers return the count
    /// as text.
    #[test]
    fn the_count_is_read_whatever_the_case_or_type() {
        assert_eq!(read_count(&one_cell("N", CellValue::Int(42))), Some(42));
        assert_eq!(
            read_count(&one_cell("n", CellValue::String("7".into()))),
            Some(7)
        );
        assert_eq!(read_count(&DataTable::empty()), None);
    }

    #[test]
    fn the_count_and_the_pages_follow_the_view() {
        use octa::db::pushdown::view::{ServerView, ViewFilter};
        let conn = crate::app::db_view::tests::conn();
        let origin = crate::app::db_view::tests::db_tab().db_origin.unwrap();
        assert_eq!(
            super::count_view_sql(&conn, &origin, None),
            octa::db::count_sql(conn.engine, None, "public", "t")
        );
        let view = ServerView {
            order: vec![],
            filters: vec![ViewFilter::values("name", ["a".to_string()])],
            derived: Vec::new(),
        };
        let sql = super::count_view_sql(&conn, &origin, Some(&view));
        assert!(
            sql.starts_with("SELECT COUNT(*) AS n FROM (SELECT * FROM"),
            "{sql}"
        );
        assert!(sql.contains("WHERE"), "{sql}");
    }
}
