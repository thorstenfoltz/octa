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
    pub(super) db_connections: &'a [(String, String)],
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
                    let label = egui::RichText::new(&row.sql_name).strong();
                    if ui.selectable_label(selected, label).clicked() {
                        action.select_inspector = Some(Some(target.clone()));
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
                        for (id, name) in db_connections {
                            if ui.button(name).clicked() {
                                action.attach_db_connection = Some(id.clone());
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("sql.attach_db_connection_hint"));
                }
            });
        });
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
