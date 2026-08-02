//! Renderer for the sidebar cloud-storage browser. Pure UI: it reads the
//! connection list + cached listings + sign-in status and returns a
//! [`CloudTreeAction`] the caller dispatches to the background workers in
//! [`super::cloud_browser`] (interaction-struct pattern, same as
//! `ui::directory_tree`).

use std::collections::{HashMap, HashSet};

use eframe::egui;

use octa::cloud::{CloudConnection, CloudKind};

use super::cloud_browser::{
    CloudSelection, CloudSort, ConnPrefix, ListState, SignInState, root_prefix, sorted_entries,
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
    /// User clicked "+ Add connection": open Settings at the Cloud section
    /// with an empty connection form.
    pub(crate) add_connection: bool,
    /// Ctrl-clicked a file: toggle it in the batch selection (do not open it).
    pub(crate) toggle_select: Option<CloudSelection>,
    /// Union every selected object (downloads them, then opens the Union dialog).
    pub(crate) union_selected: bool,
    /// "List contents as table...": recursively inventory (conn_id, prefix)
    /// into a detached tab.
    pub(crate) inventory: Option<(String, String)>,
    /// Drop the batch selection.
    pub(crate) clear_selection: bool,
    /// Copy / move / delete cloud objects: the targets plus the operation.
    /// Carries the whole batch selection when the right-clicked row is part of
    /// one, so the menu acts on what is highlighted rather than only on the row
    /// under the pointer. A key ending in `/` is a folder (always on its own,
    /// since folders are never part of the selection) and is recursive.
    pub(crate) object_op: Option<(Vec<CloudSelection>, CloudObjOp)>,
    /// "Union tables in this folder...": (conn_id, prefix, recursive). Lists
    /// the folder, downloads every readable table, opens the Union dialog.
    pub(crate) union_folder: Option<(String, String, bool)>,
    /// Replace the whole batch selection (rubber-band marquee result).
    pub(crate) set_selection: Option<HashSet<CloudSelection>>,
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
    /// Objects Ctrl-clicked for a batch action (Union).
    pub(crate) selected: &'a HashSet<CloudSelection>,
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
        // The header label has to match the buttons beside it in *box* size, not
        // just font size: a bare `Label` allocates only its text, while a Button
        // adds `button_padding` around it, so the label came out visibly smaller
        // however the text was styled. Giving it the same padding (and the same
        // Button text style, unbolded) makes all four controls one uniform row.
        egui::Frame::NONE
            .inner_margin(ui.style().spacing.button_padding)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(octa::i18n::t("cloud.connections"))
                        .text_style(egui::TextStyle::Button),
                );
            });
        // All three controls are plain `Button`s of the same text size, so they
        // line up as one row. `small_button` shrinks the padding and a
        // `menu_button` cannot be small at all, so mixing them (as this header
        // used to) gives every control a different height.
        if ui
            .button(octa::i18n::t("cloud.add_connection_btn"))
            .on_hover_text(octa::i18n::t("cloud.add_connection_btn_hint"))
            .clicked()
        {
            // Sits in the header, above the `connections.is_empty()` early
            // return below, so it is reachable precisely when there is nothing
            // to browse yet and the user needs their first connection.
            action.add_connection = true;
        }
        draw_sort_menu(ui, ctx.sort, &mut action);
        // Close sits last, hard right, away from the two it is easy to misclick.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("×")
                .on_hover_text(octa::i18n::t("cloud.close_hint"))
                .clicked()
            {
                action.close = true;
            }
        });
    });

    // Selection bar, present only while objects are Ctrl-clicked. Mirrors the
    // directory tree: a context menu alone is too easy to miss, and the count
    // is worth seeing while a selection is being built.
    if !ctx.selected.is_empty() {
        let count = ctx.selected.len();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{count} {}",
                    octa::i18n::t("union_tree.n_selected")
                ))
                .size(11.0),
            );
            if ui
                .add_enabled(
                    count >= 2,
                    egui::Button::new(octa::i18n::t("union_tree.union_btn")).small(),
                )
                .on_hover_text(octa::i18n::t("cloud.union_hint"))
                .clicked()
            {
                action.union_selected = true;
            }
            if ui
                .small_button("×")
                .on_hover_text(octa::i18n::t("union_tree.clear"))
                .clicked()
            {
                action.clear_selection = true;
            }
        });
    }

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
            // Truncate long names/keys to the panel width instead of letting
            // them force the panel wider than the widest filename (which would
            // block the user from dragging the divider narrower). Full names
            // stay reachable via the per-row hover tooltips.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

            // The visible strip of the list: what a drag has to start inside,
            // and what the auto-scroll measures its edges from.
            let viewport = ui.clip_rect();

            let mut rows: Vec<(egui::Rect, CloudSelection)> = Vec::new();
            for conn in connections {
                draw_connection(ui, ctx, conn, &mut action, &mut rows);
            }

            cloud_marquee(ui, ctx, viewport, &rows, &mut action);
            delete_key_shortcut(ui, ctx, &mut action);
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

/// Delete / Backspace on a selection asks to delete it, opening the same
/// confirmation dialog the context menu's **Delete** does. Never deletes
/// outright: the operation is irreversible without bucket versioning, so a
/// keypress must not be the last word.
///
/// `Delete` is the physical key, so this is the `Entf` key on a German layout
/// and every other layout's equivalent, with no per-layout handling. Backspace
/// is accepted too because Mac laptop keyboards have no forward-delete.
///
/// Two guards keep it from firing while the user means something else:
/// nothing may hold keyboard focus (a search box, the SQL editor, a cell being
/// edited), and the pointer has to be over the cloud list, so a selection left
/// behind in the sidebar cannot be deleted by a Delete meant for the table.
fn delete_key_shortcut(ui: &egui::Ui, ctx: &TreeCtx, action: &mut CloudTreeAction) {
    if ctx.selected.is_empty() || !ui.ui_contains_pointer() {
        return;
    }
    if ui.ctx().memory(|m| m.focused()).is_some() {
        return;
    }
    let pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if pressed {
        action.object_op = Some((ctx.selected.iter().cloned().collect(), CloudObjOp::Delete));
    }
}

/// Selection the band started from, remembered for the life of one drag so a
/// Ctrl-drag adds to it rather than to itself.
#[derive(Clone)]
struct CloudMarqueeBase(HashSet<CloudSelection>);

/// One frame of the cloud-tree rubber-band. The geometry, the click-vs-drag
/// threshold and the edge auto-scroll all live in the shared
/// [`crate::ui::directory_tree::drive_marquee`], so this tree and the local
/// file tree behave identically; only the selection type differs.
fn cloud_marquee(
    ui: &egui::Ui,
    ctx: &TreeCtx,
    viewport: egui::Rect,
    rows: &[(egui::Rect, CloudSelection)],
    action: &mut CloudTreeAction,
) {
    use crate::ui::directory_tree::drive_marquee;

    let mem_id = egui::Id::new("cloud_marquee_state");
    let base_id = egui::Id::new("cloud_marquee_base");
    let mut ctrl = false;
    let Some(frame) = drive_marquee(ui, mem_id, viewport, &mut ctrl) else {
        ui.memory_mut(|m| m.data.remove::<CloudMarqueeBase>(base_id));
        return;
    };

    // Capture the pre-drag selection once, on the frame the band appears.
    let base = ui
        .memory(|m| m.data.get_temp::<CloudMarqueeBase>(base_id))
        .unwrap_or_else(|| {
            let b = CloudMarqueeBase(if ctrl {
                ctx.selected.clone()
            } else {
                HashSet::new()
            });
            ui.memory_mut(|m| m.data.insert_temp(base_id, b.clone()));
            b
        });

    let centers: Vec<f32> = rows.iter().map(|(r, _)| r.center().y).collect();
    let mut selected = base.0;
    for i in frame.contains_row(&centers) {
        selected.insert(rows[i].1.clone());
    }
    action.set_selection = Some(selected);

    let band = frame.band_rect(viewport.x_range());
    let fill = ui.visuals().selection.bg_fill.linear_multiply(0.25);
    ui.painter().rect_filled(band, 2.0, fill);
    ui.painter().rect_stroke(
        band,
        2.0,
        egui::Stroke::new(1.0_f32, ui.visuals().selection.stroke.color),
        egui::StrokeKind::Inside,
    );
}

fn draw_connection(
    ui: &mut egui::Ui,
    ctx: &TreeCtx,
    conn: &CloudConnection,
    action: &mut CloudTreeAction,
    rows: &mut Vec<(egui::Rect, CloudSelection)>,
) {
    let root = root_prefix(conn);
    let root_key = (conn.id.clone(), root.clone());
    let is_open = ctx.expanded.contains(&root_key);
    let has_cli = ctx.cli_avail.get(&conn.kind).copied().unwrap_or(false);
    let has_secret = ctx.secret_present.get(&conn.id).copied().unwrap_or(false);
    let sign_out_armed = ctx.sign_out_confirm == Some(conn.id.as_str());
    ui.horizontal(|ui| {
        let caret = if is_open { "▼" } else { "▶" };
        let resp = ui
            .add(
                egui::Label::new(format!("{caret} {} ({})", conn.name, kind_short(conn.kind)))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(format!("{}://{}", conn.kind.scheme(), conn.bucket))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            action.toggle = Some(root_key.clone());
        }
        resp.context_menu(|ui| {
            if ui
                .button(octa::i18n::t("inventory.list_contents"))
                .on_hover_text(octa::i18n::t("inventory.list_contents_hint"))
                .clicked()
            {
                action.inventory = Some((conn.id.clone(), root.clone()));
                ui.close();
            }
            ui.separator();
            if ui
                .button(octa::i18n::t("union_tree.union_folder"))
                .on_hover_text(octa::i18n::t("union_tree.union_folder_hint"))
                .clicked()
            {
                action.union_folder = Some((conn.id.clone(), root.clone(), false));
                ui.close();
            }
            if ui
                .button(octa::i18n::t("union_tree.union_folder_recursive"))
                .on_hover_text(octa::i18n::t("union_tree.union_folder_hint"))
                .clicked()
            {
                action.union_folder = Some((conn.id.clone(), root.clone(), true));
                ui.close();
            }
        });
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
        draw_listing(ui, ctx, &conn.id, &root, 1, action, rows);
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
    rows: &mut Vec<(egui::Rect, CloudSelection)>,
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
                    let resp = indented(ui, indent, |ui| {
                        ui.add(
                            egui::Label::new(format!("{caret} {}", entry.name))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                    });
                    if resp.clicked() {
                        action.toggle = Some(key.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui
                            .button(octa::i18n::t("inventory.list_contents"))
                            .on_hover_text(octa::i18n::t("inventory.list_contents_hint"))
                            .clicked()
                        {
                            action.inventory = Some((conn_id.to_string(), entry.key.clone()));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button(octa::i18n::t("union_tree.union_folder"))
                            .on_hover_text(octa::i18n::t("union_tree.union_folder_hint"))
                            .clicked()
                        {
                            action.union_folder =
                                Some((conn_id.to_string(), entry.key.clone(), false));
                            ui.close();
                        }
                        if ui
                            .button(octa::i18n::t("union_tree.union_folder_recursive"))
                            .on_hover_text(octa::i18n::t("union_tree.union_folder_hint"))
                            .clicked()
                        {
                            action.union_folder =
                                Some((conn_id.to_string(), entry.key.clone(), true));
                            ui.close();
                        }
                        object_op_menu(
                            ui,
                            vec![CloudSelection {
                                conn_id: conn_id.to_string(),
                                key: entry.key.clone(),
                                name: entry.name.clone(),
                            }],
                            action,
                        );
                    });
                    if is_open {
                        draw_listing(ui, ctx, conn_id, &entry.key, depth + 1, action, rows);
                    }
                } else {
                    // Compact inline metadata (size + full last-modified
                    // timestamp); the hover tooltip carries the full key and
                    // exact byte count. Object stores expose only a
                    // last-modified time, not a separate creation time.
                    let meta = format_entry_meta(entry.size, entry.modified.as_ref());
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
                    let sel = CloudSelection {
                        conn_id: conn_id.to_string(),
                        key: entry.key.clone(),
                        name: entry.name.clone(),
                    };
                    let is_selected = ctx.selected.contains(&sel);
                    let resp = indented(ui, indent, |ui| {
                        let text = if is_selected {
                            // Selected rows are tinted, matching the local tree.
                            egui::RichText::new(label)
                                .background_color(ui.visuals().selection.bg_fill)
                        } else {
                            egui::RichText::new(label)
                        };
                        ui.add(egui::Label::new(text).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(tip)
                    });
                    rows.push((resp.rect, sel.clone()));
                    if resp.clicked() {
                        let mods = ui.input(|i| i.modifiers);
                        if mods.ctrl || mods.command {
                            // Ctrl-click builds a batch selection to Union; it
                            // must not also open the file.
                            action.toggle_select = Some(sel.clone());
                        } else {
                            action.open =
                                Some((conn_id.to_string(), entry.key.clone(), entry.name.clone()));
                        }
                    }
                    // Union via right-click, mirroring the directory tree:
                    // offered on a row that is part of a 2+ selection, a
                    // disabled how-to hint otherwise.
                    let count = ctx.selected.len();
                    resp.context_menu(|ui| {
                        if is_selected && count >= 2 {
                            if ui
                                .button(format!(
                                    "{} ({count})",
                                    octa::i18n::t("union_tree.selected")
                                ))
                                .on_hover_text(octa::i18n::t("cloud.union_hint"))
                                .clicked()
                            {
                                action.union_selected = true;
                                ui.close();
                            }
                        } else {
                            ui.add_enabled(
                                false,
                                egui::Button::new(octa::i18n::t("union_tree.need_two")),
                            );
                        }
                        if count > 0 && ui.button(octa::i18n::t("union_tree.clear")).clicked() {
                            action.clear_selection = true;
                            ui.close();
                        }
                        // Act on the highlighted batch when this row is in it;
                        // right-clicking an unselected row is still about that
                        // row alone.
                        let targets: Vec<CloudSelection> = if is_selected && count >= 2 {
                            ctx.selected.iter().cloned().collect()
                        } else {
                            vec![sel.clone()]
                        };
                        object_op_menu(ui, targets, action);
                    });
                }
            }
        }
    }
}

/// What to do with a cloud object or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloudObjOp {
    Copy,
    Move,
    Delete,
}

/// Copy / Move / Delete entries, shared by the folder and file context menus so
/// a folder and an object offer exactly the same three actions (the recursion
/// is implied by the trailing `/` on a folder key).
///
/// `targets` is what the action applies to: the whole batch selection when the
/// clicked row belongs to one, otherwise just that row. The count is shown in
/// the label for a batch, mirroring the Union entry, so it is obvious before
/// clicking that this is about more than the row under the pointer.
fn object_op_menu(ui: &mut egui::Ui, targets: Vec<CloudSelection>, action: &mut CloudTreeAction) {
    ui.separator();
    let n = targets.len();
    for (label, hint, op) in [
        ("cloud.copy_to", "cloud.copy_to_hint", CloudObjOp::Copy),
        ("cloud.move_to", "cloud.move_to_hint", CloudObjOp::Move),
        ("common.delete", "cloud.delete_hint", CloudObjOp::Delete),
    ] {
        let text = if n > 1 {
            format!("{} ({n})", octa::i18n::t(label))
        } else {
            octa::i18n::t(label)
        };
        if ui.button(text).on_hover_text(octa::i18n::t(hint)).clicked() {
            action.object_op = Some((targets.clone(), op));
            ui.close();
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

/// Inline metadata shown after a file's name: its size and/or the full
/// last-modified timestamp (to the second, UTC). Empty when neither is known.
fn format_entry_meta(
    size: Option<u64>,
    modified: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
    let ts = modified.map(|m| m.format("%Y-%m-%d %H:%M:%S").to_string());
    match (size, ts) {
        (Some(sz), Some(t)) => format!("  ({}, {})", human_size(sz), t),
        (Some(sz), None) => format!("  ({})", human_size(sz)),
        (None, Some(t)) => format!("  ({})", t),
        (None, None) => String::new(),
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
    use super::{format_entry_meta, human_size};
    use chrono::{TimeZone, Utc};

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn entry_meta_shows_full_timestamp() {
        let m = Utc.with_ymd_and_hms(2026, 7, 6, 14, 8, 33).unwrap();
        // Both size and time -> full timestamp to the second, not date-only.
        assert_eq!(
            format_entry_meta(Some(1024), Some(&m)),
            "  (1.0 KB, 2026-07-06 14:08:33)"
        );
        // Time only.
        assert_eq!(format_entry_meta(None, Some(&m)), "  (2026-07-06 14:08:33)");
        // Size only.
        assert_eq!(format_entry_meta(Some(512), None), "  (512 B)");
        // Neither.
        assert_eq!(format_entry_meta(None, None), "");
    }
}
