//! The workspace pane: attached sources, their schema trees, and the
//! inspector that describes the selected node.
//!
//! Split out of `view_modes/sql.rs` (1,555 lines). Code moved unchanged.

use super::*;

/// Read-only workspace data threaded from the panel into the workspace
/// section renderers (bundled to keep the functions under the arg limit).
pub(super) struct WorkspaceData<'a> {
    pub(super) tables: &'a [WorkspaceRow],
    pub(super) attachments: &'a [WorkspaceAttachment],
    pub(super) db_connections: &'a [super::DbAttachEntry],
    pub(super) cloud_connections: &'a [(String, String)],
}

pub(super) fn render_workspace_section(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    data: &WorkspaceData<'_>,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    inspector_entry: Option<&crate::app::state::InspectorCacheEntry>,
    action: &mut SqlAction,
) {
    let extras = data.tables.iter().filter(|t| !t.is_active).count();
    let attached = data.attachments.len();
    let summary = if extras == 0 && attached == 0 {
        octa::i18n::t("sql.ws_only_data")
    } else {
        format!(
            "{} ({} {}, {} {})",
            octa::i18n::t("sql.workspace"),
            extras,
            octa::i18n::t("sql.extra_tables"),
            attached,
            octa::i18n::t("sql.attached_dbs"),
        )
    };
    // `CollapsingHeader` paints its own triangle via egui's drawing primitives,
    // so the glyph always renders even when the bundled font lacks the
    // geometric-shapes block (where `\u{25be}` / `\u{25b8}` live).
    let resp = egui::CollapsingHeader::new(egui::RichText::new(summary).strong())
        .id_salt("sql_workspace_section")
        .default_open(tab.sql_workspace_open)
        .show(ui, |ui| {
            // Two independent Resize widgets stacked vertically. Each gets its
            // own bottom-edge handle, so the user can grow the tree without
            // touching the inspector or the editor, and vice versa. The
            // editor's existing top-split handle stays independent of both.
            egui::Resize::default()
                .id_salt("sql_workspace_tree_resize")
                .resizable([false, true])
                .min_height(80.0)
                .default_height(140.0)
                .show(ui, |ui| {
                    render_workspace_list(ui, tab, data, inspector_selection, action);
                });
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(2.0);
            egui::Resize::default()
                .id_salt("sql_workspace_inspector_resize")
                .resizable([false, true])
                .min_height(120.0)
                .default_height(240.0)
                .show(ui, |ui| {
                    if data.attachments.is_empty() {
                        render_workspace_inspector(
                            ui,
                            inspector_selection,
                            inspector_entry,
                            action,
                        );
                    } else {
                        // With servers attached, the spare width next to the
                        // Inspector carries a cheat-sheet of the attachments:
                        // the alias to type (connection names get sanitised,
                        // which is not obvious) and a ready-made query per
                        // connection.
                        ui.columns(2, |cols| {
                            render_workspace_inspector(
                                &mut cols[0],
                                inspector_selection,
                                inspector_entry,
                                action,
                            );
                            render_attachment_info(&mut cols[1], data.attachments, action);
                        });
                    }
                });
        });
    tab.sql_workspace_open = resp.openness > 0.5;
    ui.add_space(2.0);
    ui.separator();
}

fn render_workspace_list(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    data: &WorkspaceData<'_>,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    action: &mut SqlAction,
) {
    let WorkspaceData {
        tables,
        attachments,
        db_connections,
        cloud_connections,
    } = *data;
    let weak = ui.visuals().weak_text_color();
    egui::ScrollArea::vertical()
        .id_salt("sql_workspace_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in tables {
                let target = crate::app::sql_panel::InspectorTarget::RegisteredTable {
                    sql_name: row.sql_name.clone(),
                };
                let selected = inspector_selection == Some(&target);
                ui.horizontal(|ui| {
                    // Rename in place: the name exists to be typed in a FROM
                    // clause, and a dialog for one word is more chrome than
                    // the edit. The draft lives in egui's temp memory, so the
                    // per-frame row list needs no state of its own.
                    let rename_id = ui.id().with(("ws_rename", &row.sql_name));
                    let edit_id = rename_id.with("edit");
                    if let Some(mut buf) = ui.data(|d| d.get_temp::<String>(rename_id)) {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut buf)
                                .id(edit_id)
                                .desired_width(160.0),
                        );
                        // Focus the frame the box first appears (a focus
                        // request made while it did not exist yet is dropped
                        // at the end of that pass). `lost_focus` guards the
                        // frame the user clicks away, or the box would grab
                        // the focus straight back.
                        if !resp.has_focus() && !resp.lost_focus() {
                            resp.request_focus();
                        }
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if enter {
                            action.rename_table = Some((row.sql_name.clone(), buf.clone()));
                        }
                        let cancelled = !enter
                            && (resp.lost_focus()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape)));
                        if enter || cancelled {
                            ui.data_mut(|d| d.remove::<String>(rename_id));
                        } else {
                            ui.data_mut(|d| d.insert_temp(rename_id, buf));
                        }
                    } else {
                        let label = egui::RichText::new(&row.sql_name).strong();
                        let mut resp = ui.selectable_label(selected, label);
                        if !row.is_active {
                            resp = resp.on_hover_text(octa::i18n::t("sql.table_row_hint"));
                            if resp.double_clicked() {
                                let name = row.sql_name.clone();
                                ui.data_mut(|d| d.insert_temp(rename_id, name));
                            }
                        }
                        if resp.clicked() {
                            action.select_inspector = Some(Some(target.clone()));
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "({} {})",
                            row.row_count,
                            octa::i18n::t("sql.rows")
                        ))
                        .small()
                        .color(weak),
                    )
                    .on_hover_text(row.origin.clone());
                    if row.is_active {
                        if ui
                            .small_button(octa::i18n::t("sql.refresh"))
                            .on_hover_text(octa::i18n::t("sql.refresh_hint"))
                            .clicked()
                        {
                            action.refresh_active = true;
                        }
                    } else if ui
                        .small_button("\u{00d7}")
                        .on_hover_text(octa::i18n::t("sql.remove_table_hint"))
                        .clicked()
                    {
                        action.remove_table = Some(row.sql_name.clone());
                    }
                });
            }
            for att in attachments {
                let alias_key = att.alias.clone();
                let alias_open = tab.sql_workspace_tree_expanded.contains(&alias_key);
                ui.horizontal(|ui| {
                    let tri_resp = collapsing_triangle(ui, alias_open)
                        .on_hover_text(octa::i18n::t("sql.toggle_attached_hint"));
                    let label_resp = ui.add(
                        egui::Label::new(egui::RichText::new(&att.alias).strong())
                            .sense(egui::Sense::click()),
                    );
                    if tri_resp.clicked() || label_resp.clicked() {
                        action.toggle_tree_key = Some(alias_key.clone());
                    }
                    ui.label(
                        egui::RichText::new(format!("[{}]", att.kind_label))
                            .small()
                            .color(weak),
                    );
                    if !att.native {
                        ui.label(
                            egui::RichText::new(octa::i18n::t("sql.fallback"))
                                .small()
                                .color(weak)
                                .italics(),
                        )
                        .on_hover_text(octa::i18n::t("sql.fallback_hint"));
                    }
                    ui.label(
                        egui::RichText::new(format!("| {} tbl", att.table_count))
                            .small()
                            .color(weak),
                    )
                    .on_hover_text(att.source.clone());
                    if ui
                        .small_button(octa::i18n::t("sql.detach"))
                        .on_hover_text(octa::i18n::t("sql.detach_hint"))
                        .clicked()
                    {
                        action.detach_alias = Some(att.alias.clone());
                    }
                });
                if alias_open {
                    render_attached_tree(ui, tab, att, inspector_selection, action);
                }
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("sql.add_table"))
                    .on_hover_text(octa::i18n::t("sql.add_table_hint"))
                    .clicked()
                {
                    action.add_tables = true;
                }
                if ui
                    .button(octa::i18n::t("sql.attach_db"))
                    .on_hover_text(octa::i18n::t("sql.attach_db_hint"))
                    .clicked()
                {
                    action.attach_db = true;
                }
                // Saved live-database connections (Settings -> Databases).
                if !db_connections.is_empty() {
                    ui.menu_button(octa::i18n::t("sql.attach_db_connection"), |ui| {
                        for entry in db_connections {
                            match &entry.drill {
                                // Native ATTACH: nothing is imported, so one
                                // click takes the whole server.
                                None => {
                                    if ui.button(&entry.name).clicked() {
                                        action.attach_db_connection =
                                            Some((entry.id.clone(), Default::default()));
                                        ui.close();
                                    }
                                }
                                // Import engine: every table it takes is
                                // fetched, so the entry opens into the tree
                                // and the user picks how much to import.
                                Some(drill) => {
                                    ui.menu_button(&entry.name, |ui| {
                                        drill_menu(ui, entry, drill, &mut Vec::new(), action);
                                    })
                                    .response
                                    .on_hover_text(octa::i18n::t("sql.attach_drill_hint"));
                                }
                            }
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("sql.attach_db_connection_hint"));
                }
                // Saved cloud connections (Settings -> Cloud storage).
                if !cloud_connections.is_empty() {
                    ui.menu_button(octa::i18n::t("sql.attach_cloud"), |ui| {
                        for (id, name) in cloud_connections {
                            if ui.button(name).clicked() {
                                action.attach_cloud_connection = Some(id.clone());
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("sql.attach_cloud_hint"));
                }
            });
        });
}

/// One level of the attach drill-down: the catalogs, schemas or tables under
/// `parts`, plus an entry that attaches everything below it. Asking for a
/// level is a network call, so it is started when the submenu first opens and
/// read from the sidebar's shared cache afterwards.
///
/// Attaching an import connection copies every table it covers, and a single
/// Databricks catalog can hold hundreds, so the menu goes all the way down to
/// the one table the user is after.
fn drill_menu(
    ui: &mut egui::Ui,
    entry: &super::DbAttachEntry,
    drill: &super::DrillMenu,
    parts: &mut Vec<String>,
    action: &mut SqlAction,
) {
    use super::NodeListing;
    // Depth whose children are tables rather than another level to open.
    let leaf_depth = if drill.has_catalogs { 2 } else { 1 };
    let Some(listing) = drill.nodes.get(parts) else {
        action.list_db_node = Some((entry.id.clone(), parts.clone()));
        ui.label(octa::i18n::t("sql.loading"));
        return;
    };
    match listing {
        NodeListing::Loading => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(octa::i18n::t("sql.loading"));
            });
            ui.ctx().request_repaint();
        }
        NodeListing::Failed(e) => {
            octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
        }
        NodeListing::Ready(items) if items.is_empty() => {
            ui.label(octa::i18n::t("sql.attach_nothing_here"));
        }
        NodeListing::Ready(items) => {
            // "Everything here" is offered at every level a server can answer.
            // A three-level engine cannot be enumerated without a catalog, so
            // its root is the one place that gets no such entry.
            if !(drill.has_catalogs && parts.is_empty()) {
                if ui
                    .button(octa::i18n::t("sql.attach_all_here"))
                    .on_hover_text(octa::i18n::t("sql.attach_all_here_hint"))
                    .clicked()
                {
                    action.attach_db_connection =
                        Some((entry.id.clone(), scope_for(drill, parts, None)));
                    ui.close();
                }
                ui.separator();
            }
            // Solid scrollbars: a floating one paints over the labels.
            ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
            ui.allocate_ui(level_box_size(ui, items), |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in items {
                        if parts.len() == leaf_depth {
                            if ui
                                .button(item)
                                .on_hover_text(octa::i18n::t("sql.attach_table_hint"))
                                .clicked()
                            {
                                action.attach_db_connection =
                                    Some((entry.id.clone(), scope_for(drill, parts, Some(item))));
                                ui.close();
                            }
                        } else {
                            parts.push(item.clone());
                            ui.menu_button(item, |ui| drill_menu(ui, entry, drill, parts, action))
                                .response
                                .on_hover_text(octa::i18n::t("sql.attach_drill_hint"));
                            parts.pop();
                        }
                    }
                });
            });
        }
    }
}

/// Size for one menu level's scrolling list, measured from the entries
/// themselves.
///
/// A menu popup is an auto-sized [`egui::Area`]: it measures itself the first
/// frame it opens and only ever grows when its content's `min_size` grows. A
/// `ScrollArea` never grows - it shrinks into whatever space it is handed,
/// down to its own 64px floor. A submenu therefore opened at the size of the
/// "Loading..." label the listing had not replaced yet, and stayed there,
/// showing two entries however many the server sent. Measuring the box from
/// the entries is what makes the content grow, so the popup grows with it.
fn level_box_size(ui: &egui::Ui, items: &[String]) -> egui::Vec2 {
    /// Taller than this and the popup would run off a laptop screen.
    const MAX_HEIGHT: f32 = 360.0;
    let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
    let full = items.len() as f32 * row;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let painter = ui.painter();
    let widest = items.iter().fold(0.0_f32, |w, item| {
        let galley = painter.layout_no_wrap(item.clone(), font.clone(), egui::Color32::PLACEHOLDER);
        w.max(galley.size().x)
    });
    // Room for the button padding, a submenu arrow, and the bar when the list
    // is long enough to scroll.
    let bar = if full > MAX_HEIGHT {
        ui.spacing().scroll.allocated_width()
    } else {
        0.0
    };
    egui::vec2(
        widest + ui.spacing().button_padding.x * 2.0 + 24.0 + bar,
        full.min(MAX_HEIGHT),
    )
}

/// Turn a node path (plus the table clicked inside it, if any) into the scope
/// the workspace imports: `[catalog, ] schema, table`.
fn scope_for(
    drill: &super::DrillMenu,
    parts: &[String],
    leaf: Option<&String>,
) -> octa::sql::AttachScope {
    let mut walk = parts.iter().chain(leaf).cloned();
    octa::sql::AttachScope {
        catalog: drill.has_catalogs.then(|| walk.next()).flatten(),
        schema: walk.next(),
        table: walk.next(),
    }
}

/// Paint a small collapsing triangle as a clickable widget. Replaces the
/// unicode `\u{25b8}` / `\u{25be}` glyphs the workspace tree used to draw -
/// the bundled font doesn't ship the geometric-shapes block, so users saw
/// tofu squares instead of arrows. Drawing the triangle directly via
/// `egui::Painter` is font-independent.
fn collapsing_triangle(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let size = egui::vec2(12.0, ui.spacing().interact_size.y.min(16.0));
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let stroke = ui.style().interact(&resp).fg_stroke;
        let center = rect.center();
        let r = 4.0;
        let points = if open {
            // Down-pointing triangle (▾)
            vec![
                center + egui::vec2(-r, -r * 0.5),
                center + egui::vec2(r, -r * 0.5),
                center + egui::vec2(0.0, r * 0.8),
            ]
        } else {
            // Right-pointing triangle (▸)
            vec![
                center + egui::vec2(-r * 0.5, -r),
                center + egui::vec2(-r * 0.5, r),
                center + egui::vec2(r * 0.8, 0.0),
            ]
        };
        ui.painter()
            .add(egui::Shape::convex_polygon(points, stroke.color, stroke));
    }
    resp
}

fn render_attached_tree(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    att: &WorkspaceAttachment,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    if att.schemas.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("(no tables visible - fallback attachment)")
                    .small()
                    .color(weak)
                    .italics(),
            );
        });
        return;
    }
    for schema in &att.schemas {
        let schema_key = format!("{}::{}", att.alias, schema.schema);
        let schema_open = tab.sql_workspace_tree_expanded.contains(&schema_key);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            let tri_resp = collapsing_triangle(ui, schema_open);
            let label_resp = ui.add(
                egui::Label::new(format!("{} ({})", schema.schema, schema.tables.len()))
                    .sense(egui::Sense::click()),
            );
            if tri_resp.clicked() || label_resp.clicked() {
                action.toggle_tree_key = Some(schema_key.clone());
            }
        });
        if schema_open {
            for t in &schema.tables {
                let target = crate::app::sql_panel::InspectorTarget::AttachedTable {
                    alias: att.alias.clone(),
                    schema: t.schema.clone(),
                    table: t.table.clone(),
                };
                let selected = inspector_selection == Some(&target);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    let label = egui::RichText::new(&t.table);
                    if ui.selectable_label(selected, label).clicked() {
                        action.select_inspector = Some(Some(target.clone()));
                    }
                    if let Some(n) = t.row_count {
                        ui.label(
                            egui::RichText::new(format!("({n} rows)"))
                                .small()
                                .color(weak),
                        );
                    }
                });
            }
        }
    }
}

/// The attachments cheat-sheet next to the Inspector: what each attached
/// connection is called in SQL (the sanitised alias - "Post-Test" becomes
/// `post_test`, which users cannot guess), where it points, and a one-click
/// example query per connection.
fn render_attachment_info(
    ui: &mut egui::Ui,
    attachments: &[WorkspaceAttachment],
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    let strong = ui.visuals().strong_text_color();
    ui.label(
        egui::RichText::new(octa::i18n::t("sql.att_info"))
            .strong()
            .color(strong),
    );
    ui.label(
        egui::RichText::new(octa::i18n::t("sql.att_info_note"))
            .weak()
            .size(10.0),
    );
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("sql_att_info_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for att in attachments {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&att.alias)
                            .strong()
                            .monospace()
                            .color(strong),
                    );
                    ui.label(
                        egui::RichText::new(format!("[{}]", att.kind_label))
                            .small()
                            .color(weak),
                    );
                });
                ui.label(egui::RichText::new(&att.source).small().color(weak));
                // A ready-made query against the first table, so the alias
                // never has to be typed by hand.
                if let Some(schema) = att.schemas.first()
                    && let Some(t0) = schema.tables.first()
                {
                    let qualified = format!("{}.{}.{}", att.alias, t0.schema, t0.table);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{qualified}, ..."))
                                .small()
                                .monospace()
                                .color(weak),
                        );
                        if ui
                            .small_button(octa::i18n::t("sql.insert"))
                            .on_hover_text(format!(
                                "{} `SELECT * FROM {qualified} LIMIT 100;`",
                                octa::i18n::t("sql.insert_hint")
                            ))
                            .clicked()
                        {
                            action.insert_qualified = Some(qualified.clone());
                        }
                    });
                }
                ui.add_space(6.0);
            }
        });
}

fn render_workspace_inspector(
    ui: &mut egui::Ui,
    inspector_selection: Option<&crate::app::sql_panel::InspectorTarget>,
    inspector_entry: Option<&crate::app::state::InspectorCacheEntry>,
    action: &mut SqlAction,
) {
    let weak = ui.visuals().weak_text_color();
    let strong = ui.visuals().strong_text_color();
    let target = match inspector_selection {
        Some(t) => t,
        None => {
            ui.label(
                egui::RichText::new(octa::i18n::t("sql.inspector"))
                    .strong()
                    .color(strong),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new(octa::i18n::t("sql.inspector_empty")).weak());
            return;
        }
    };
    let qualified = target.qualified_sql();
    // Top header bar - qualified name + clear-selection button. Docked so it
    // stays visible no matter how short the inspector pane is.
    egui::Panel::top("sql_inspector_header")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&qualified)
                        .strong()
                        .monospace()
                        .color(strong),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("\u{00d7}")
                        .on_hover_text(octa::i18n::t("sql.clear_inspector_hint"))
                        .clicked()
                    {
                        action.select_inspector = Some(None);
                    }
                });
            });
            ui.separator();
        });
    // Bottom action bar - Copy / Insert / Run. Docked so it never scrolls
    // out of view even when the column list is long.
    egui::Panel::bottom("sql_inspector_actions")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .small_button(octa::i18n::t("sql.copy_name"))
                    .on_hover_text(octa::i18n::t("sql.copy_name_hint"))
                    .clicked()
                {
                    action.copy_qualified = Some(qualified.clone());
                }
                if ui
                    .small_button(octa::i18n::t("sql.insert"))
                    .on_hover_text(format!(
                        "{} `SELECT * FROM {qualified} LIMIT 100;`",
                        octa::i18n::t("sql.insert_hint")
                    ))
                    .clicked()
                {
                    action.insert_qualified = Some(qualified.clone());
                }
                if ui
                    .small_button(octa::i18n::t("sql.run_table"))
                    .on_hover_text(format!(
                        "{} `SELECT * FROM {qualified} LIMIT 100`",
                        octa::i18n::t("sql.run_table_hint")
                    ))
                    .clicked()
                {
                    action.run_qualified = Some(qualified.clone());
                }
            });
            ui.add_space(2.0);
        });
    // Central body - columns grid + sample table inside a scroll area that
    // fills whatever vertical room is between the header and the action bar.
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let entry = match inspector_entry {
                Some(e) => e,
                None => {
                    ui.label(egui::RichText::new(octa::i18n::t("sql.loading")).weak());
                    return;
                }
            };
            let inspection = match &entry.result {
                Ok(i) => i,
                Err(msg) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 80, 80),
                        format!("Error: {msg}"),
                    );
                    return;
                }
            };
            let row_count_str = inspection
                .row_count
                .map(format_number)
                .unwrap_or_else(|| "?".to_string());
            ui.label(
                egui::RichText::new(format!(
                    "{} column{} | {} row{}",
                    inspection.columns.len(),
                    if inspection.columns.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    row_count_str,
                    if inspection.row_count == Some(1) {
                        ""
                    } else {
                        "s"
                    },
                ))
                .small()
                .color(weak),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("sql_inspector_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("sql_inspector_columns")
                        .num_columns(2)
                        .spacing(egui::vec2(10.0, 2.0))
                        .show(ui, |ui| {
                            for col in &inspection.columns {
                                ui.label(egui::RichText::new(&col.name).monospace());
                                ui.label(
                                    egui::RichText::new(&col.data_type)
                                        .monospace()
                                        .small()
                                        .color(weak),
                                );
                                ui.end_row();
                            }
                        });
                    if !inspection.sample_rows.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "Sample (first {}):",
                                inspection.sample_rows.len()
                            ))
                            .small()
                            .color(weak),
                        );
                        ui.add_space(2.0);
                        use egui_extras::{Column, TableBuilder};
                        let mut builder = TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
                        for _ in &inspection.columns {
                            builder = builder.column(Column::auto().at_least(60.0).resizable(true));
                        }
                        builder
                            .header(18.0, |mut header| {
                                for col in &inspection.columns {
                                    header.col(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&col.name).small().monospace(),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Extend),
                                        );
                                    });
                                }
                            })
                            .body(|mut body| {
                                for row in &inspection.sample_rows {
                                    body.row(16.0, |mut r| {
                                        for cell in row {
                                            r.col(|ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(cell)
                                                            .small()
                                                            .monospace(),
                                                    )
                                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                                );
                                            });
                                        }
                                    });
                                }
                            });
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Height of the tallest popup egui has open, or 0.0 when none is.
    fn open_popup_height(ctx: &egui::Context) -> f32 {
        let layers = ctx.memory(|m| m.areas().visible_layer_ids());
        layers
            .iter()
            .filter(|l| l.order != egui::Order::Background)
            .filter_map(|l| ctx.read_response(l.id.with("move")))
            .fold(0.0_f32, |h, r| h.max(r.rect.height()))
    }

    /// A submenu that opens while its listing is still loading, then gets 200
    /// tables. Driven headlessly near the bottom edge of a short window, the
    /// SQL panel's own geometry.
    ///
    /// The regression, and the reason the first two attempts at this menu
    /// shipped broken: a menu popup is an auto-sized `Area` that caches its
    /// size the first frame it opens, and only ever grows when its content's
    /// `min_size` grows. A `ScrollArea` never grows - it shrinks into whatever
    /// space it is given, down to its 64px floor. So the popup froze at the
    /// size of the "Loading..." label it opened with, and showed two entries
    /// forever after. Allocating a box measured from the entries is what makes
    /// the content grow, so the popup grows with it.
    #[test]
    fn a_level_that_opened_while_loading_grows_when_the_listing_lands() {
        let entry = super::super::DbAttachEntry {
            id: "c".to_string(),
            name: "warehouse".to_string(),
            drill: None,
        };
        let loading = super::super::DrillMenu {
            has_catalogs: false,
            nodes: std::collections::HashMap::new(),
        };
        let ready = super::super::DrillMenu {
            has_catalogs: false,
            nodes: std::collections::HashMap::from([(
                Vec::new(),
                super::super::NodeListing::Ready(
                    (0..200).map(|i| format!("orders_{i:03}")).collect(),
                ),
            )]),
        };
        let ctx = egui::Context::default();
        let pass = |drill: &super::super::DrillMenu, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 300.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let mut action = SqlAction::default();
                ui.add_space(255.0);
                ui.menu_button("attach", |ui| {
                    ui.menu_button("warehouse", |ui| {
                        drill_menu(ui, &entry, drill, &mut Vec::new(), &mut action);
                    });
                });
            });
            out.textures_delta.clear();
        };
        let click = |pressed: bool| egui::Event::PointerButton {
            pos: egui::pos2(20.0, 270.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        // Open the menu, and the connection's submenu inside it, while the
        // listing is still on its way.
        pass(&loading, vec![]);
        pass(
            &loading,
            vec![
                egui::Event::PointerMoved(egui::pos2(20.0, 270.0)),
                click(true),
            ],
        );
        pass(&loading, vec![click(false)]);
        for y in [240.0_f32, 235.0, 245.0, 250.0, 230.0] {
            pass(
                &loading,
                vec![egui::Event::PointerMoved(egui::pos2(30.0, y))],
            );
            pass(&loading, vec![]);
        }
        assert!(
            open_popup_height(&ctx) > 0.0,
            "the submenu should be open while the listing runs"
        );
        // The listing lands.
        for _ in 0..10 {
            pass(&ready, vec![]);
        }
        let grown = open_popup_height(&ctx);
        assert!(
            grown > 200.0,
            "the popup must grow to the list it now holds, got {grown}"
        );
        for _ in 0..30 {
            pass(&ready, vec![]);
        }
        let settled = open_popup_height(&ctx);
        assert!(
            (settled - grown).abs() < 1.0,
            "the popup shrank while it sat open: {grown} -> {settled}"
        );
    }
}
