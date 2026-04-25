//! Drain background-loaded rows into the active table, update the status
//! message, evict front rows when memory grows too large, and request
//! repaint while loading is in-flight.

use eframe::egui;

use octa::ui;

use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn drain_background_rows(&mut self, ctx: &egui::Context) {
        let Some(buffer) = self.tabs[self.active_tab].bg_row_buffer.clone() else {
            return;
        };
        let mut drained = false;
        if let Ok(mut buf) = buffer.try_lock() {
            if !buf.is_empty() {
                self.tabs[self.active_tab].table.rows.append(&mut *buf);
                drained = true;
            }
        }
        let loading_done = self.tabs[self.active_tab]
            .bg_loading_done
            .load(std::sync::atomic::Ordering::Relaxed);
        if drained {
            self.tabs[self.active_tab].filter_dirty = true;
            let file_exhausted = self.tabs[self.active_tab]
                .bg_file_exhausted
                .load(std::sync::atomic::Ordering::Relaxed);
            if self.tabs[self.active_tab].table.total_rows.is_some() {
                let total_loaded = self.tabs[self.active_tab].table.row_offset
                    + self.tabs[self.active_tab].table.row_count();
                let total_fmt = ui::status_bar::format_number(total_loaded);
                if loading_done && file_exhausted {
                    self.status_message = Some((
                        format!("Loaded all {} rows", total_fmt),
                        std::time::Instant::now(),
                    ));
                    self.tabs[self.active_tab].table.total_rows = None;
                    self.tabs[self.active_tab].bg_can_load_more = false;
                } else if loading_done {
                    self.status_message = Some((
                        format!("Loaded {} rows (scroll down to load more)", total_fmt),
                        std::time::Instant::now(),
                    ));
                    self.tabs[self.active_tab].bg_can_load_more = true;
                } else {
                    self.status_message = Some((
                        format!("Loading... {} rows so far", total_fmt),
                        std::time::Instant::now(),
                    ));
                }
            }
            // Evict front rows if we have too many in memory.
            if self.tabs[self.active_tab].table.rows.len() > 3_000_000 {
                let evict_count = self.tabs[self.active_tab].table.rows.len() - 2_000_000;
                self.tabs[self.active_tab]
                    .table
                    .evict_front_rows(evict_count);
                self.tabs[self.active_tab].filter_dirty = true;
            }
        }
        if loading_done {
            self.tabs[self.active_tab].bg_row_buffer = None;
        }
        if !loading_done {
            ctx.request_repaint();
        }
    }
}
