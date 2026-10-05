//! "Hash columns" dialog: append one column holding a hex digest of the chosen
//! columns, in the chosen order, whatever their types. The work is
//! `octa::data::transform::hash_columns`, shared with the MCP tool and the
//! CLI action.
//!
//! On a live-database tab that does not hold every row (and runs analyses on
//! the database) the database computes the column: the hash joins the tab's
//! server view and the sync reads the table again with it filled, for every
//! row present and future. Any other tab that holds only part of its source
//! is refused: the hash would cover the loaded rows only.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::transform::hash_columns::{self as engine, HashColumnsAlgo, HashColumnsSpec};
use octa::db::pushdown::hash::ServerHash;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::db_view::{SETTLE, server_conn, view_source_for};
use super::super::pushdown::{ServerTask, TaskPoll};
use super::super::state::{OctaApp, TabState};

/// Rows shown in the preview.
const PREVIEW_ROWS: usize = 5;

#[derive(Default)]
pub(crate) struct HashColumnsState {
    pub(crate) spec: HashColumnsSpec,
    /// The new column's name. Follows the picked columns until the user
    /// types one of their own.
    pub(crate) name: String,
    pub(crate) name_edited: bool,
    pub(crate) size: DialogSize,
    /// The database's preview on a server tab.
    pub(crate) server: Option<ServerPreview>,
}

/// The database's first rows for one hash: asked once the settings have
/// stood for the typing pause.
pub(crate) struct ServerPreview {
    pub(crate) key: ServerHash,
    pub(crate) since: Instant,
    pub(crate) task: Option<ServerTask<Vec<(String, String)>>>,
    pub(crate) result: Option<Result<Vec<(String, String)>, String>>,
}

impl OctaApp {
    /// Whether tab `tab` gets its hash from the database.
    fn hash_on_server(&self, tab: &TabState) -> bool {
        server_conn(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )
        .is_some()
    }

    /// Entry from **Columns -> Hash columns...** and the shortcut.
    pub(crate) fn open_hash_columns(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let partial = tab.source_has_more() && !self.hash_on_server(tab);
        if tab.table.col_count() == 0 || partial || self.is_readonly() {
            return;
        }
        self.hash_columns_dialog = Some(HashColumnsState::default());
    }

    /// The server preview's next step: start over for new settings, ask
    /// once they have settled, take the answer.
    fn step_hash_preview(&self, st: &mut HashColumnsState, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if st.spec.columns.is_empty() {
            st.server = None;
            return;
        }
        let names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        // The name is not part of the preview's SQL: renaming asks nothing.
        let key = ServerHash::of(&st.spec, &names, "h");
        let now = Instant::now();
        let p = match st.server.as_mut() {
            Some(p) if p.key == key => p,
            _ => {
                st.server = Some(ServerPreview {
                    key,
                    since: now,
                    task: None,
                    result: None,
                });
                ctx.request_repaint_after(SETTLE);
                return;
            }
        };
        if let Some(task) = &p.task {
            let answer = match task.poll() {
                TaskPoll::Pending => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                TaskPoll::Ready(rows) => Ok(rows),
                TaskPoll::Cancelled => Err(t("pushdown.cancelled")),
                TaskPoll::Failed(e) => Err(e),
            };
            p.task = None;
            p.result = Some(answer);
            return;
        }
        if p.result.is_some() {
            return;
        }
        let waited = now.saturating_duration_since(p.since);
        if waited < SETTLE {
            ctx.request_repaint_after(SETTLE - waited);
            return;
        }
        let Some(mut src) = view_source_for(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        ) else {
            return;
        };
        // The rows as the tab holds them: its filter and hash columns.
        if let Some(v) = &tab.server_view {
            src.filter = v.where_sql(src.engine());
            src.derived = v.derived.clone();
        }
        let h = p.key.clone();
        p.task = Some(self.spawn_server_task(
            src.conn.clone(),
            t("hashcols.title"),
            move |c, stop| octa::db::pushdown::hash::preview(c, &src, &h, PREVIEW_ROWS, stop),
        ));
        ctx.request_repaint();
    }
}

pub(crate) fn render_hash_columns_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.hash_columns_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let on_server = app.hash_on_server(tab);
    if on_server {
        app.step_hash_preview(&mut st, ctx);
    }
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    // Adding a database hash reads the table again, which would drop edits.
    let unsaved = on_server && tab.table.is_modified();
    let table = &tab.table;
    let col_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    // A column deleted while the dialog was open drops out of the pick.
    st.spec.columns.retain(|&c| c < col_names.len());
    if !st.name_edited {
        st.name = if st.spec.columns.is_empty() {
            String::new()
        } else {
            engine::default_name(table, &st.spec.columns)
        };
    }
    let name = st.name.trim().to_string();
    let name_taken = col_names.contains(&name);
    let ready = !st.spec.columns.is_empty() && !name.is_empty() && !name_taken && !unsaved;

    let mut close = false;
    let mut apply = false;
    let dialog_id = egui::Id::new("octa_hash_columns_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(520.0, 520.0));
    let window = egui::Window::new("octa_hash_columns")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(520.0)
            .min_width(360.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("hash_columns_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("hashcols.title")).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }

        egui::Panel::bottom("hash_columns_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_row(ui, |ui| {
                    let b = ui.add_enabled(ready, egui::Button::new(t("hashcols.apply")));
                    if b.clicked() {
                        apply = true;
                    }
                    if ready {
                        b.on_hover_text(t(if on_server {
                            "hashcols.server_apply_hint"
                        } else {
                            "hashcols.apply_hint"
                        }));
                    } else if unsaved {
                        b.on_disabled_hover_text(t("hashcols.unsaved_hint"));
                    } else {
                        b.on_disabled_hover_text(t("hashcols.apply_disabled_hint"));
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("hashcols.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    RichText::new(t("hashcols.intro"))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
                if on_server {
                    octa::ui::message::partial_note_label(ui, &t("hashcols.server_intro"));
                }
                ui.add_space(6.0);

                // The ordered pick: the order is part of the hash.
                ui.label(t("hashcols.columns"))
                    .on_hover_text(t("hashcols.columns_hint"));
                let mut action: Option<(usize, i32)> = None; // (position, -1 up / 1 down / 0 remove)
                for (pos, &c) in st.spec.columns.iter().enumerate() {
                    octa::ui::control_row::control_row(ui, |ui| {
                        ui.label(format!("{}. {}", pos + 1, col_names[c]));
                        if ui
                            .add_enabled(pos > 0, egui::Button::new("^").small())
                            .on_hover_text(t("hashcols.up_hint"))
                            .clicked()
                        {
                            action = Some((pos, -1));
                        }
                        if ui
                            .add_enabled(
                                pos + 1 < st.spec.columns.len(),
                                egui::Button::new("v").small(),
                            )
                            .on_hover_text(t("hashcols.down_hint"))
                            .clicked()
                        {
                            action = Some((pos, 1));
                        }
                        if ui
                            .small_button("x")
                            .on_hover_text(t("hashcols.remove_hint"))
                            .clicked()
                        {
                            action = Some((pos, 0));
                        }
                    });
                }
                match action {
                    Some((pos, 0)) => {
                        st.spec.columns.remove(pos);
                    }
                    Some((pos, d)) => {
                        let other = (pos as i32 + d) as usize;
                        st.spec.columns.swap(pos, other);
                    }
                    None => {}
                }
                let mut add: Option<usize> = None;
                egui::ComboBox::from_id_salt("hash_columns_add")
                    .selected_text(t("hashcols.add"))
                    .show_ui(ui, |ui| {
                        for (i, n) in col_names.iter().enumerate() {
                            if !st.spec.columns.contains(&i)
                                && ui.selectable_label(false, n).clicked()
                            {
                                add = Some(i);
                            }
                        }
                    })
                    .response
                    .on_hover_text(t("hashcols.add_hint"));
                if let Some(i) = add {
                    st.spec.columns.push(i);
                }

                ui.add_space(8.0);
                octa::ui::control_row::control_grid(ui, "hash_columns_grid", |ui| {
                    ui.label(t("hashcols.algo"))
                        .on_hover_text(t("hashcols.algo_hint"));
                    egui::ComboBox::from_id_salt("hash_columns_algo")
                        .selected_text(st.spec.algo.label())
                        .show_ui(ui, |ui| {
                            for a in HashColumnsAlgo::ALL {
                                ui.selectable_value(&mut st.spec.algo, a, a.label());
                            }
                        })
                        .response
                        .on_hover_text(t("hashcols.algo_hint"));
                    ui.end_row();

                    ui.label(t("hashcols.delimiter"))
                        .on_hover_text(t("hashcols.delimiter_hint"));
                    ui.add(egui::TextEdit::singleline(&mut st.spec.delimiter).desired_width(80.0))
                        .on_hover_text(t("hashcols.delimiter_hint"));
                    ui.end_row();

                    ui.label(t("hashcols.null_text"))
                        .on_hover_text(t("hashcols.null_text_hint"));
                    ui.add(egui::TextEdit::singleline(&mut st.spec.null_text).desired_width(80.0))
                        .on_hover_text(t("hashcols.null_text_hint"));
                    ui.end_row();

                    ui.label(t("hashcols.name"))
                        .on_hover_text(t("hashcols.name_hint"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut st.name).desired_width(220.0))
                        .on_hover_text(t("hashcols.name_hint"))
                        .changed()
                    {
                        st.name_edited = true;
                    }
                    ui.end_row();
                });
                ui.checkbox(&mut st.spec.trim, t("hashcols.trim"))
                    .on_hover_text(t("hashcols.trim_hint"));
                ui.checkbox(&mut st.spec.upper, t("hashcols.upper"))
                    .on_hover_text(t("hashcols.upper_hint"));
                if name_taken {
                    ui.colored_label(ui.visuals().warn_fg_color, t("hashcols.name_taken"));
                }

                if !st.spec.columns.is_empty() && on_server {
                    ui.add_space(8.0);
                    ui.label(t("hashcols.preview"))
                        .on_hover_text(t("hashcols.preview_server_hint"));
                    let mono = egui::FontId::monospace(11.0);
                    match st.server.as_ref().and_then(|p| p.result.as_ref()) {
                        Some(Ok(rows)) => {
                            for (input, digest) in rows {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(digest.as_str()).font(mono.clone()),
                                    )
                                    .selectable(true),
                                )
                                .on_hover_text(input.as_str());
                            }
                        }
                        Some(Err(e)) => octa::ui::message::selectable_message(
                            ui,
                            ui.visuals().error_fg_color,
                            &format!("{} {e}", t("hashcols.preview_failed")),
                        ),
                        None => {
                            octa::ui::control_row::control_row(ui, |ui| {
                                ui.spinner();
                                ui.weak(t("hashcols.preview_loading"));
                            });
                        }
                    }
                } else if !st.spec.columns.is_empty() {
                    ui.add_space(8.0);
                    ui.label(t("hashcols.preview"))
                        .on_hover_text(t("hashcols.preview_hint"));
                    let mono = egui::FontId::monospace(11.0);
                    for r in 0..table.row_count().min(PREVIEW_ROWS) {
                        let input = engine::row_input(table, r, &st.spec);
                        let digest = engine::hash_columns_row(table, r, &st.spec);
                        ui.add(
                            egui::Label::new(RichText::new(digest).font(mono.clone()))
                                .selectable(true),
                        )
                        .on_hover_text(input);
                    }
                }
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if apply && on_server && !app.is_readonly() {
        // The database fills the column: the sync reads the table again.
        let tab = &mut app.tabs[app.active_tab];
        let names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        tab.server_hashes
            .push(ServerHash::of(&st.spec, &names, &name));
        if !tab.server_hash_seen.contains(&name) {
            tab.server_hash_seen.push(name.clone());
        }
        let idx = tab.table.col_count();
        tab.table.insert_column(idx, name.clone(), "Utf8".into());
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        app.status_message = Some((
            t("hashcols.server_done").replace("{name}", &name),
            std::time::Instant::now(),
        ));
        return;
    }
    if apply && !app.is_readonly() {
        let tab = &mut app.tabs[app.active_tab];
        let values = engine::hash_columns(&tab.table, &st.spec);
        let idx = tab.table.col_count();
        let start = tab.table.undo_stack.len();
        tab.table.insert_column(idx, name.clone(), "Utf8".into());
        for (r, v) in values.into_iter().enumerate() {
            tab.table.set(r, idx, v);
        }
        tab.table.coalesce_undo_since(start);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        app.status_message = Some((
            t("hashcols.done").replace("{name}", &name),
            std::time::Instant::now(),
        ));
        return;
    }
    if !close {
        app.hash_columns_dialog = Some(st);
    }
}
