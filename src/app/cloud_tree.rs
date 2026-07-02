//! Renderer for the sidebar cloud-storage browser. Pure UI: it reads the
//! connection list + cached listings + sign-in status and returns a
//! [`CloudTreeAction`] the caller dispatches to the background workers in
//! [`super::cloud_browser`] (interaction-struct pattern, same as
//! `ui::directory_tree`).

use std::collections::{HashMap, HashSet};

use eframe::egui;

use octa::cloud::{CloudConnection, CloudKind};

use super::cloud_browser::{
    CloudSort, ConnPrefix, ListState, SignInState, root_prefix, sorted_entries,
};

const INDENT_PER_LEVEL: f32 = 14.0;

/// What the user did in the cloud tree this frame.
#[derive(Default)]
pub(crate) struct CloudTreeAction {
    /// Expand/collapse a node (connection root has prefix `""`).
    pub(crate) toggle: Option<ConnPrefix>,
    /// Open a file: (conn_id, key, name).
    pub(crate) open: Option<(String, String, String)>,
    /// Run sign-in for a connection.
    pub(crate) sign_in: Option<String>,
    /// Arm the "Sign out (clear saved keys)" confirm for a connection.
    pub(crate) sign_out_arm: Option<String>,
    /// Confirmed sign-out: clear this connection's saved secret.
    pub(crate) sign_out_yes: Option<String>,
    /// Cancel an armed sign-out confirm.
    pub(crate) sign_out_cancel: bool,
    /// Refresh a connection's listings.
    pub(crate) refresh: Option<String>,
    /// Change the file sort order.
    pub(crate) set_sort: Option<CloudSort>,
    /// Hide the cloud section.
    pub(crate) close: bool,
}

/// Shared, read-only borrows the caller assembles and threads through the
/// connection/listing renderers (bundled so the draw functions - and this
/// entry point - stay under the argument limit). `listings`/`sign_in` are
/// snapshots taken under their mutex; `cli_avail`/`secret_present` are the
/// memoised per-connection lookups (the caller does the IO, never the paint).
pub(crate) struct TreeCtx<'a> {
    pub(crate) listings: &'a HashMap<ConnPrefix, ListState>,
    pub(crate) expanded: &'a HashSet<ConnPrefix>,
    pub(crate) sign_in: &'a HashMap<String, SignInState>,
    pub(crate) cli_avail: &'a HashMap<CloudKind, bool>,
    pub(crate) secret_present: &'a HashMap<String, bool>,
    /// Connection id with an armed "Sign out" confirm, if any.
    pub(crate) sign_out_confirm: Option<&'a str>,
    /// Current file sort order.
    pub(crate) sort: CloudSort,
}

/// Render the cloud section. `share_with_dir` caps the list at half height when
/// the directory tree shares the panel.
pub(crate) fn render_cloud_tree(
    ui: &mut egui::Ui,
    connections: &[CloudConnection],
    ctx: &TreeCtx,
    share_with_dir: bool,
) -> CloudTreeAction {
    let mut action = CloudTreeAction::default();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(octa::i18n::t("cloud.connections")).strong());
        if ui
            .small_button("×")
            .on_hover_text(octa::i18n::t("cloud.close_hint"))
            .clicked()
        {
            action.close = true;
        }
        draw_sort_menu(ui, ctx.sort, &mut action);
    });

    if connections.is_empty() {
        ui.label(
            egui::RichText::new(octa::i18n::t("cloud.connect_hint"))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
        );
        return action;
    }

    let max_height = if share_with_dir {
        ui.available_height() * 0.5
    } else {
        ui.available_height()
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("cloud_tree_scroll")
        .max_height(max_height)
        .show(ui, |ui| {
            for conn in connections {
                draw_connection(ui, ctx, conn, &mut action);
            }
        });
    action
}

/// A compact "Sort" menu: pick how files are ordered in every folder.
fn draw_sort_menu(ui: &mut egui::Ui, current: CloudSort, action: &mut CloudTreeAction) {
    ui.menu_button(octa::i18n::t("cloud.sort"), |ui| {
        for (opt, key) in [
            (CloudSort::NameAsc, "cloud.sort_name_asc"),
            (CloudSort::NameDesc, "cloud.sort_name_desc"),
            (CloudSort::ModifiedNewest, "cloud.sort_newest"),
            (CloudSort::ModifiedOldest, "cloud.sort_oldest"),
            (CloudSort::SizeLargest, "cloud.sort_largest"),
            (CloudSort::SizeSmallest, "cloud.sort_smallest"),
        ] {
            if ui
                .selectable_label(current == opt, octa::i18n::t(key))
                .clicked()
            {
                action.set_sort = Some(opt);
                ui.close();
            }
        }
    })
    .response
    .on_hover_text(octa::i18n::t("cloud.sort_hint"));
}

fn draw_connection(
    ui: &mut egui::Ui,
    ctx: &TreeCtx,
    conn: &CloudConnection,
    action: &mut CloudTreeAction,
) {
    let root = root_prefix(conn);
    let root_key = (conn.id.clone(), root.clone());
    let is_open = ctx.expanded.contains(&root_key);
    let has_cli = ctx.cli_avail.get(&conn.kind).copied().unwrap_or(false);
    let has_secret = ctx.secret_present.get(&conn.id).copied().unwrap_or(false);
    let sign_out_armed = ctx.sign_out_confirm == Some(conn.id.as_str());
    ui.horizontal(|ui| {
        let caret = if is_open { "▼" } else { "▶" };
        if ui
            .add(
                egui::Label::new(format!("{caret} {} ({})", conn.name, kind_short(conn.kind)))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(format!("{}://{}", conn.kind.scheme(), conn.bucket))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            action.toggle = Some(root_key.clone());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Refresh (only meaningful once expanded).
            if is_open
                && ui
                    .small_button(octa::i18n::t("cloud.refresh"))
                    .on_hover_text(octa::i18n::t("cloud.refresh_hint"))
                    .clicked()
            {
                action.refresh = Some(conn.id.clone());
            }
            // Right-side control reflects how the connection authenticates:
            // public -> nothing to do; saved keys -> Sign out; otherwise the
            // browser Sign in (or a "needs CLI" note).
            if conn.anonymous {
                // nothing: the mode shows on the status line below.
            } else if has_secret {
                if ui
                    .add(egui::Button::new(octa::i18n::t("cloud.sign_out")).small())
                    .on_hover_text(octa::i18n::t("cloud.sign_out_hint"))
                    .clicked()
                {
                    action.sign_out_arm = Some(conn.id.clone());
                }
            } else {
                match ctx.sign_in.get(&conn.id) {
                    Some(SignInState::InProgress) => {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.label(octa::i18n::t("cloud.signing_in"));
                    }
                    _ if has_cli => {
                        if ui
                            .add(egui::Button::new(octa::i18n::t("cloud.sign_in")).small())
                            .on_hover_text(octa::i18n::t("cloud.sign_in_hint"))
                            .clicked()
                        {
                            action.sign_in = Some(conn.id.clone());
                        }
                    }
                    // CLI missing: show the reason inline (visible without
                    // hovering), with the full explanation on hover.
                    _ => {
                        ui.label(
                            egui::RichText::new(octa::i18n::t("cloud.sign_in_no_cli"))
                                .small()
                                .color(ui.visuals().warn_fg_color),
                        )
                        .on_hover_text(octa::i18n::t("cloud.sign_in_needs_cli"));
                    }
                }
            }
        });
    });

    // Status line: auth mode + reachability from the last listing.
    draw_status_line(ui, ctx, conn, has_secret);

    // Sign-out confirm (clearing saved keys is destructive, so require a
    // second explicit click - mirrors the Settings Clear-secret guard).
    if sign_out_armed {
        ui.horizontal(|ui| {
            ui.add_space(INDENT_PER_LEVEL);
            ui.label(
                egui::RichText::new(octa::i18n::t("cloud.secret_clear_confirm"))
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
            if ui
                .small_button(octa::i18n::t("cloud.secret_clear_yes"))
                .clicked()
            {
                action.sign_out_yes = Some(conn.id.clone());
            }
            if ui
                .small_button(octa::i18n::t("cloud.secret_clear_cancel"))
                .clicked()
            {
                action.sign_out_cancel = true;
            }
        });
    }

    // Sign-in failure note under the row.
    if let Some(SignInState::Failed(msg)) = ctx.sign_in.get(&conn.id) {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("{} {msg}", octa::i18n::t("cloud.sign_in_failed")),
        );
    }

    if is_open {
        draw_listing(ui, ctx, &conn.id, &root, 1, action);
    }
}

/// Small second line: how the connection authenticates, plus whether the last
/// listing reached the bucket. No network here - reachability is read from the
/// cached root listing (so it persists when the node is collapsed).
fn draw_status_line(ui: &mut egui::Ui, ctx: &TreeCtx, conn: &CloudConnection, has_secret: bool) {
    let mode = if conn.anonymous {
        octa::i18n::t("cloud.mode_public")
    } else if has_secret {
        octa::i18n::t("cloud.mode_keys")
    } else {
        octa::i18n::t("cloud.mode_signin")
    };
    ui.horizontal(|ui| {
        ui.add_space(INDENT_PER_LEVEL);
        ui.label(
            egui::RichText::new(mode)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        match ctx.listings.get(&(conn.id.clone(), root_prefix(conn))) {
            Some(ListState::Ready(_)) => {
                ui.label(
                    egui::RichText::new(octa::i18n::t("cloud.reachable"))
                        .small()
                        .color(egui::Color32::from_rgb(0x4c, 0xaf, 0x50)),
                );
            }
            Some(ListState::Error(_)) => {
                ui.label(
                    egui::RichText::new(octa::i18n::t("cloud.unreachable"))
                        .small()
                        .color(ui.visuals().error_fg_color),
                );
            }
            _ => {}
        }
    });
}

/// Render the cached listing for one node, recursing into expanded folders.
fn draw_listing(
    ui: &mut egui::Ui,
    ctx: &TreeCtx,
    conn_id: &str,
    prefix: &str,
    depth: usize,
    action: &mut CloudTreeAction,
) {
    let indent = depth as f32 * INDENT_PER_LEVEL;
    match ctx.listings.get(&(conn_id.to_string(), prefix.to_string())) {
        None | Some(ListState::Loading) => {
            indented(ui, indent, |ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(octa::i18n::t("cloud.loading"));
            });
        }
        Some(ListState::Error(msg)) => {
            indented(ui, indent, |ui| {
                ui.colored_label(ui.visuals().error_fg_color, msg);
            });
        }
        Some(ListState::Ready(entries)) => {
            if entries.is_empty() {
                indented(ui, indent, |ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("cloud.empty"))
                            .color(ui.visuals().weak_text_color()),
                    );
                });
                return;
            }
            for entry in sorted_entries(entries, ctx.sort) {
                if entry.is_prefix {
                    let key = (conn_id.to_string(), entry.key.clone());
                    let is_open = ctx.expanded.contains(&key);
                    let caret = if is_open { "▼" } else { "▶" };
                    let clicked = indented(ui, indent, |ui| {
                        ui.add(
                            egui::Label::new(format!("{caret} {}", entry.name))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    });
                    if clicked {
                        action.toggle = Some(key.clone());
                    }
                    if is_open {
                        draw_listing(ui, ctx, conn_id, &entry.key, depth + 1, action);
                    }
                } else {
                    // Compact inline metadata (size + last-modified date);
                    // the hover tooltip carries the full key, exact size, and
                    // timestamp. Object stores expose only a last-modified time,
                    // not a separate creation time.
                    let meta = match (entry.size, entry.modified.as_ref()) {
                        (Some(sz), Some(m)) => {
                            format!("  ({}, {})", human_size(sz), m.format("%Y-%m-%d"))
                        }
                        (Some(sz), None) => format!("  ({})", human_size(sz)),
                        (None, Some(m)) => format!("  ({})", m.format("%Y-%m-%d")),
                        (None, None) => String::new(),
                    };
                    let label = format!("{}{}", entry.name, meta);
                    let mut tip = entry.key.clone();
                    if let Some(sz) = entry.size {
                        tip.push_str(&format!(
                            "\n{} {} ({} bytes)",
                            octa::i18n::t("cloud.size"),
                            human_size(sz),
                            sz
                        ));
                    }
                    if let Some(m) = entry.modified.as_ref() {
                        tip.push_str(&format!(
                            "\n{} {} UTC",
                            octa::i18n::t("cloud.modified"),
                            m.format("%Y-%m-%d %H:%M:%S")
                        ));
                    }
                    let clicked = indented(ui, indent, |ui| {
                        ui.add(egui::Label::new(label).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(tip)
                            .clicked()
                    });
                    if clicked {
                        action.open =
                            Some((conn_id.to_string(), entry.key.clone(), entry.name.clone()));
                    }
                }
            }
        }
    }
}

/// Run `body` inside a left-indented horizontal row, returning its value.
fn indented<R>(ui: &mut egui::Ui, indent: f32, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        body(ui)
    })
    .inner
}

/// Short provider label shown after a connection name (no logos - those carry
/// trademark constraints; the short name distinguishes providers at a glance).
fn kind_short(kind: CloudKind) -> &'static str {
    match kind {
        CloudKind::S3 => "S3",
        CloudKind::AzureBlob => "Azure",
        CloudKind::Gcs => "GCS",
    }
}

/// Compact human-readable byte size (B/KB/MB/GB).
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
