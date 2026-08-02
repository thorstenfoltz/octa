//! Copy / move / delete a cloud object or folder, from the cloud sidebar's
//! context menu.
//!
//! All three share one dialog because they share one decision: which object,
//! and (for copy/move) where to. The work itself is
//! [`octa::cloud::ops`], the same code the MCP tools and `octa --cloud-copy`
//! run, so the three surfaces cannot drift apart.
//!
//! The transfer runs on a worker thread (network, and credential resolution
//! may shell out to a cloud CLI); the dialog polls a result slot per frame,
//! like the database copy dialog next door.

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::cloud::{self, TransferReport};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::cloud_browser::CloudSelection;
use super::super::cloud_tree::CloudObjOp;
use super::super::state::OctaApp;

/// The folder a key sits in, with its trailing `/` ("a/b/c.csv" -> "a/b/").
/// The starting point for a batch destination, which has to be a folder.
fn parent_prefix(key: &str) -> String {
    match key.trim_end_matches('/').rfind('/') {
        Some(i) => key[..=i].to_string(),
        None => String::new(),
    }
}

/// Shared slot the worker writes its outcome into.
type TransferSlot = Arc<Mutex<Option<Result<TransferReport, String>>>>;

pub(crate) struct CloudTransferState {
    pub(crate) size: DialogSize,
    pub(crate) op: CloudObjOp,
    /// What the operation applies to: one object, one folder, or the whole
    /// batch the user had selected when they right-clicked.
    pub(crate) srcs: Vec<CloudSelection>,
    /// Human-readable source: the URL for one, a count for several.
    pub(crate) src_label: String,
    /// Destination connection (copy / move only).
    pub(crate) tgt_conn_id: String,
    /// Destination key, pre-filled with the source's.
    pub(crate) tgt_key: String,
    pub(crate) job: Option<TransferSlot>,
    pub(crate) result_msg: Option<(bool, String)>,
}

impl CloudTransferState {
    /// True when the single source is a folder (a batch is always files:
    /// folders are never part of the tree's selection).
    fn is_folder(&self) -> bool {
        self.srcs.len() == 1 && cloud::is_prefix(&self.srcs[0].key)
    }

    /// True when several objects are being handled at once, which forces the
    /// destination to be a folder.
    fn is_batch(&self) -> bool {
        self.srcs.len() > 1
    }
}

impl OctaApp {
    /// Entry from the cloud tree's context menu. `srcs` is one object, one
    /// folder, or the whole selection the user had highlighted.
    pub(crate) fn open_cloud_transfer(&mut self, srcs: Vec<CloudSelection>, op: CloudObjOp) {
        let Some(first) = srcs.first().cloned() else {
            return;
        };
        let Some(conn) = self.find_cloud_conn(&first.conn_id) else {
            return;
        };
        // An account-level connection has no fixed bucket: the tree qualifies
        // its keys as `<bucket>/<key>`, so the label (and later the provider)
        // has to be derived through `bind_bucket` rather than from `conn`
        // directly, whose `bucket` is empty.
        let (bound, sub) = super::super::cloud_browser::bind_bucket(&conn, &first.key);
        let src_label = if srcs.len() > 1 {
            format!("{} {}", srcs.len(), t("dialog.selected"))
        } else {
            format!("{}://{}/{}", bound.kind.scheme(), bound.bucket, sub)
        };
        // Prefill the destination: the source key for a single object, its
        // parent folder for a batch (which must land in a folder anyway).
        let tgt_key = if srcs.len() > 1 {
            parent_prefix(&first.key)
        } else {
            first.key.clone()
        };
        // Default target is the connection the sources came from: "copy this
        // next to itself" is the common case, and the combo makes another one
        // one click away.
        self.cloud_transfer_dialog = Some(CloudTransferState {
            size: DialogSize::Normal,
            op,
            tgt_conn_id: first.conn_id.clone(),
            tgt_key,
            srcs,
            src_label,
            job: None,
            result_msg: None,
        });
    }

    /// The write gate, checked before anything is spawned: the connection's own
    /// `allow_writes`. Same rule as saving back to a cloud tab and as the
    /// assistant's cloud writes, so one switch per connection governs every
    /// surface.
    fn cloud_op_blocked(&self, conn_id: &str) -> Option<String> {
        match self.find_cloud_conn(conn_id) {
            Some(c) if c.allow_writes => None,
            Some(c) => Some(t("cloud.conn_writes_off").replace("{name}", &c.name)),
            None => Some(t("cloud.open_failed")),
        }
    }

    fn spawn_cloud_transfer(&self, st: &mut CloudTransferState, ctx: &egui::Context) {
        let Some(first) = st.srcs.first().cloned() else {
            return;
        };
        // The destination is what gets written; for a delete that is the
        // sources' own connection.
        let write_conn = if st.op == CloudObjOp::Delete {
            first.conn_id.clone()
        } else {
            st.tgt_conn_id.clone()
        };
        if let Some(msg) = self.cloud_op_blocked(&write_conn) {
            st.result_msg = Some((false, msg));
            return;
        }
        let Some(tgt_conn) = self.find_cloud_conn(&write_conn) else {
            st.result_msg = Some((false, t("cloud.open_failed")));
            return;
        };
        let tgt_key_raw = st.tgt_key.trim().to_string();
        if st.op != CloudObjOp::Delete {
            if tgt_key_raw.is_empty() {
                st.result_msg = Some((false, t("dialog.clt_need_key")));
                return;
            }
            // A folder source, or several sources at once, needs a folder
            // destination: otherwise every object would be written onto the
            // same single key.
            if (st.is_folder() || st.is_batch()) && !tgt_key_raw.ends_with('/') {
                st.result_msg = Some((false, t("dialog.clt_need_folder_target")));
                return;
            }
        }

        // Resolve every source's connection here, on the UI thread, so the
        // worker only carries owned data. `bind_bucket` because an
        // account-level connection has an empty `bucket` while its keys carry
        // the bucket name: without it the provider addresses nothing.
        let mut sources: Vec<(octa::cloud::CloudConnection, String)> = Vec::new();
        for s in &st.srcs {
            let Some(conn) = self.find_cloud_conn(&s.conn_id) else {
                st.result_msg = Some((false, t("cloud.open_failed")));
                return;
            };
            sources.push(super::super::cloud_browser::bind_bucket(&conn, &s.key));
        }
        let (tgt_conn, tgt_key) = super::super::cloud_browser::bind_bucket(&tgt_conn, &tgt_key_raw);

        let slot: TransferSlot = Arc::new(Mutex::new(None));
        st.job = Some(slot.clone());
        st.result_msg = None;
        let settings = self.settings.clone();
        let op = st.op;
        let batch = st.is_batch();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<TransferReport> {
                let mut total = TransferReport {
                    server_side: true,
                    ..Default::default()
                };
                // Built once and reused for every source: a batch of fifty
                // objects should not open fifty clients.
                let dst = match op {
                    CloudObjOp::Delete => None,
                    _ => {
                        let creds =
                            octa::ui::settings::cloud_secrets::resolve_creds(&tgt_conn, &settings);
                        Some(cloud::build_provider(&tgt_conn, &creds)?)
                    }
                };

                for (src_conn, src_key) in &sources {
                    let src_creds =
                        octa::ui::settings::cloud_secrets::resolve_creds(src_conn, &settings);
                    let src = cloud::build_provider(src_conn, &src_creds)?;
                    let report = match dst.as_ref() {
                        None => cloud::ops::delete(src.as_ref(), src_key)?,
                        Some(dst) => {
                            // Same bucket of the same provider means the backend
                            // can copy server-side; anything else is streamed.
                            // Compared after binding, so two buckets of one
                            // account-level connection count as different.
                            let same_store = src_conn.kind == tgt_conn.kind
                                && src_conn.bucket == tgt_conn.bucket;
                            // Several sources keep their own names side by side
                            // under the target folder; a single one goes exactly
                            // where the user typed.
                            let dest = if batch {
                                cloud::ops::dest_in_folder(&tgt_key, src_key)
                            } else {
                                tgt_key.clone()
                            };
                            let f = if op == CloudObjOp::Move {
                                cloud::ops::move_
                            } else {
                                cloud::ops::copy
                            };
                            f(src.as_ref(), src_key, dst.as_ref(), &dest, same_store)?
                        }
                    };
                    total.objects += report.objects;
                    total.bytes += report.bytes;
                    total.server_side &= report.server_side;
                }
                Ok(total)
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }
}

pub(crate) fn render_cloud_transfer_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.cloud_transfer_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut refresh_conn: Option<String> = None;
    let mut st = app.cloud_transfer_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    if let Some(slot) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.result_msg = Some(match res {
            Ok(report) => {
                // The listing on screen is now stale on both ends.
                refresh_conn = Some(st.tgt_conn_id.clone());
                (
                    true,
                    t("dialog.clt_done").replace("{n}", &report.objects.to_string()),
                )
            }
            Err(e) => (false, e),
        });
        st.job = None;
    }
    let running = st.job.is_some();
    let deleting = st.op == CloudObjOp::Delete;

    let dialog_id = egui::Id::new("octa_cloud_transfer_dialog");
    let window = egui::Window::new("octa_cloud_transfer")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(240.0)
            .min_width(360.0)
            .min_height(180.0)
    });

    let inner = window.show(ctx, |ui| {
        let title = match st.op {
            CloudObjOp::Copy => "dialog.clt_title_copy",
            CloudObjOp::Move => "dialog.clt_title_move",
            CloudObjOp::Delete => "dialog.clt_title_delete",
        };
        egui::Panel::top("cloud_transfer_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t(title)).strong().size(16.0));
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
        egui::Panel::bottom("cloud_transfer_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                // Outcome above the buttons, on its own row: a provider error
                // is long, and inside the horizontal row below it would run off
                // the edge unwrapped.
                if let Some((ok, msg)) = &st.result_msg {
                    draw_result_message(ui, *ok, msg);
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    let go = match st.op {
                        CloudObjOp::Copy => t("cloud.copy_to"),
                        CloudObjOp::Move => t("cloud.move_to"),
                        CloudObjOp::Delete => t("common.delete"),
                    };
                    let btn = egui::Button::new(if deleting {
                        RichText::new(go).color(egui::Color32::from_rgb(0xd9, 0x53, 0x4f))
                    } else {
                        RichText::new(go)
                    });
                    if ui.add_enabled(!running, btn).clicked() {
                        run = true;
                    }
                    if running {
                        ui.spinner();
                        ui.label(t("dialog.dbc_running"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // "Cancel" means "I changed my mind"; once the copy
                        // has actually run there is nothing left to cancel, so
                        // the same button becomes "Done".
                        let label = if st.result_msg.is_some() {
                            t("common.done")
                        } else {
                            t("common.cancel")
                        };
                        if ui.button(label).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::Grid::new("cloud_transfer_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("dialog.clt_source"));
                    ui.monospace(&st.src_label);
                    ui.end_row();

                    if st.is_folder() || st.is_batch() {
                        ui.label("");
                        // A batch and a folder share the same consequence for
                        // the destination: it has to be a folder, and every
                        // object goes into it.
                        ui.label(
                            RichText::new(if st.is_batch() {
                                t("dialog.clt_need_folder_target")
                            } else {
                                t("dialog.clt_folder_note")
                            })
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                        ui.end_row();
                    }

                    if !deleting {
                        ui.label(t("dialog.clt_target_conn"));
                        let selected = app
                            .settings
                            .cloud_connections
                            .iter()
                            .find(|c| c.id == st.tgt_conn_id)
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| t("dialog.dbc_pick"));
                        egui::ComboBox::from_id_salt("cloud_transfer_target")
                            .selected_text(selected)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for c in app.settings.cloud_connections.iter() {
                                    let mut label = format!("{} ({})", c.name, c.kind.scheme());
                                    if !c.allow_writes {
                                        label.push_str(&format!("  [{}]", t("db.copy_writes_off")));
                                    }
                                    if ui.selectable_label(st.tgt_conn_id == c.id, label).clicked()
                                    {
                                        st.tgt_conn_id = c.id.clone();
                                    }
                                }
                            });
                        ui.end_row();

                        ui.label(t("dialog.clt_target_key"));
                        ui.vertical(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut st.tgt_key)
                                    .desired_width(280.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            // Where that actually lands, resolved the same way
                            // the transfer will resolve it. An account-level
                            // connection expects `<bucket>/<key>` while a
                            // bucket-scoped one does not, and the prefill comes
                            // from the source: showing the result is cheaper
                            // than explaining the rule, and needs no wording.
                            if let Some(c) = app
                                .settings
                                .cloud_connections
                                .iter()
                                .find(|c| c.id == st.tgt_conn_id)
                            {
                                let (bound, key) =
                                    super::super::cloud_browser::bind_bucket(c, st.tgt_key.trim());
                                ui.label(
                                    RichText::new(format!(
                                        "{}://{}/{key}",
                                        bound.kind.scheme(),
                                        bound.bucket
                                    ))
                                    .monospace()
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                                );
                            }
                        });
                        ui.end_row();
                    }
                });

            if deleting {
                ui.add_space(8.0);
                ui.colored_label(
                    egui::Color32::from_rgb(0xd9, 0x53, 0x4f),
                    t(if st.is_folder() {
                        "dialog.clt_delete_folder_warn"
                    } else {
                        "dialog.clt_delete_warn"
                    }),
                );
            }
        });
    });

    if let Some(r) = &inner {
        remember_dialog_rect(ctx, dialog_id, size, r.response.rect);
    }
    st.size = size;

    if run {
        app.spawn_cloud_transfer(&mut st, ctx);
    }
    if let Some(conn_id) = refresh_conn {
        // Both ends can have changed (a move empties the source folder), and a
        // batch can span connections, so refresh every one involved once.
        let mut seen: Vec<String> = vec![conn_id];
        for s in &st.srcs {
            if !seen.contains(&s.conn_id) {
                seen.push(s.conn_id.clone());
            }
        }
        for id in seen {
            app.refresh_cloud_conn(ctx, id);
        }
    }
    if !close {
        app.cloud_transfer_dialog = Some(st);
    }
}
