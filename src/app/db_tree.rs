//! Renderer for the sidebar Databases browser. Pure UI: it reads the saved
//! connection list + cached listings and returns a [`DbTreeAction`] the
//! caller dispatches to the background workers in [`super::db_browser`]
//! (interaction-struct pattern, same as `cloud_tree`).

use std::collections::{HashMap, HashSet};

use eframe::egui;

use octa::db::{DbConnection, DbEngine};

use super::db_browser::{ConnSchema, DbListState, join_path, split_path};

const INDENT_PER_LEVEL: f32 = 14.0;

/// What the user did in the Databases tree this frame.
#[derive(Default)]
pub(crate) struct DbTreeAction {
    /// Expand/collapse a node (connection root has path `""`).
    pub(crate) toggle: Option<ConnSchema>,
    /// Open a table read-only: (conn_id, catalog, schema, table).
    pub(crate) open: Option<(String, Option<String>, String, String)>,
    /// Open the "Copy table to another connection" dialog for
    /// (conn_id, catalog, schema, table).
    pub(crate) copy: Option<(String, Option<String>, String, String)>,
    /// Show a table's metadata in a read-only tab for
    /// (conn_id, catalog, schema, table).
    pub(crate) metadata: Option<(String, Option<String>, String, String)>,
    /// Re-list a connection.
    pub(crate) refresh: Option<String>,
    /// Open Settings at the Databases section with a blank form.
    pub(crate) add_connection: bool,
    /// Hide the Databases section.
    pub(crate) close: bool,
}

/// Render the Databases section. `share` caps the list at half height when
/// another sidebar section shares the panel.
pub(crate) fn render_db_tree(
    ui: &mut egui::Ui,
    connections: &[DbConnection],
    listings: &HashMap<ConnSchema, DbListState>,
    expanded: &HashSet<ConnSchema>,
    share: bool,
) -> DbTreeAction {
    let mut action = DbTreeAction::default();
    ui.horizontal(|ui| {
        // Same header anatomy as the cloud tree: padded label so it matches
        // the buttons' box height, plain buttons, close hard right.
        egui::Frame::NONE
            .inner_margin(ui.style().spacing.button_padding)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(octa::i18n::t("db.tree_title"))
                        .text_style(egui::TextStyle::Button),
                );
            });
        if ui
            .button(octa::i18n::t("db.add_btn"))
            .on_hover_text(octa::i18n::t("db.add_btn_hint"))
            .clicked()
        {
            action.add_connection = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("×")
                .on_hover_text(octa::i18n::t("db.close_hint"))
                .clicked()
            {
                action.close = true;
            }
        });
    });

    if connections.is_empty() {
        ui.label(
            egui::RichText::new(octa::i18n::t("db.no_connections"))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
        );
        return action;
    }

    let max_height = if share {
        ui.available_height() * 0.5
    } else {
        ui.available_height()
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("db_tree_scroll")
        .max_height(max_height)
        .show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            for conn in connections {
                draw_connection(ui, listings, expanded, conn, &mut action);
            }
        });
    action
}

fn draw_connection(
    ui: &mut egui::Ui,
    listings: &HashMap<ConnSchema, DbListState>,
    expanded: &HashSet<ConnSchema>,
    conn: &DbConnection,
    action: &mut DbTreeAction,
) {
    let root_key = (conn.id.clone(), String::new());
    let is_open = expanded.contains(&root_key);
    ui.horizontal(|ui| {
        let caret = if is_open { "▼" } else { "▶" };
        let resp = ui
            .add(
                egui::Label::new(format!(
                    "{caret} {} ({})",
                    conn.name,
                    engine_short(conn.engine)
                ))
                .sense(egui::Sense::click()),
            )
            .on_hover_text(format!("{}:{}/{}", conn.host, conn.port, conn.database))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            action.toggle = Some(root_key.clone());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_open
                && ui
                    .small_button(octa::i18n::t("db.refresh"))
                    .on_hover_text(octa::i18n::t("db.refresh_hint"))
                    .clicked()
            {
                action.refresh = Some(conn.id.clone());
            }
            if conn.allow_writes {
                ui.label(
                    egui::RichText::new(octa::i18n::t("db.writes_on"))
                        .small()
                        .color(ui.visuals().warn_fg_color),
                )
                .on_hover_text(octa::i18n::t("db.allow_writes_hint"));
            }
        });
    });

    if is_open {
        // Root path is "": for a catalog engine it lists catalogs, else schemas.
        draw_level(
            ui,
            listings,
            expanded,
            conn,
            None,
            "",
            INDENT_PER_LEVEL,
            action,
        );
    }
}

/// Render the children of the node at `path` (path == "" is the connection
/// root). `catalog` carries the catalog once we are below a catalog node, so
/// table opens qualify with the full three-part name.
#[allow(clippy::too_many_arguments)]
fn draw_level(
    ui: &mut egui::Ui,
    listings: &HashMap<ConnSchema, DbListState>,
    expanded: &HashSet<ConnSchema>,
    conn: &DbConnection,
    catalog: Option<&str>,
    path: &str,
    indent: f32,
    action: &mut DbTreeAction,
) {
    // The child key for `name` under the current path.
    let child_key = |name: &str| -> String {
        let mut parts = split_path(path);
        parts.push(name);
        join_path(&parts)
    };
    match listings.get(&(conn.id.clone(), path.to_string())) {
        None | Some(DbListState::Loading) => loading_row(ui, indent),
        Some(DbListState::Error(msg)) => error_row(ui, indent, msg),
        Some(DbListState::Catalogs(cats)) => {
            for cat in cats {
                let child = child_key(cat);
                let key = (conn.id.clone(), child.clone());
                let is_open = expanded.contains(&key);
                let caret = if is_open { "▼" } else { "▶" };
                let resp = indented(ui, indent, |ui| {
                    ui.add(egui::Label::new(format!("{caret} {cat}")).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                });
                if resp.clicked() {
                    action.toggle = Some(key.clone());
                }
                if is_open {
                    draw_level(
                        ui,
                        listings,
                        expanded,
                        conn,
                        Some(cat),
                        &child,
                        indent + INDENT_PER_LEVEL,
                        action,
                    );
                }
            }
        }
        Some(DbListState::Schemas(schemas)) => {
            for schema in schemas {
                let child = child_key(schema);
                let key = (conn.id.clone(), child.clone());
                let is_open = expanded.contains(&key);
                let caret = if is_open { "▼" } else { "▶" };
                let resp = indented(ui, indent, |ui| {
                    ui.add(
                        egui::Label::new(format!("{caret} {schema}")).sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                });
                if resp.clicked() {
                    action.toggle = Some(key.clone());
                }
                if is_open {
                    draw_tables(
                        ui,
                        listings,
                        conn,
                        catalog,
                        schema,
                        &child,
                        indent + INDENT_PER_LEVEL,
                        action,
                    );
                }
            }
        }
        Some(DbListState::Tables(_)) => {}
    }
}

/// Render one expanded schema's table list. `catalog`/`schema` build the open
/// and copy actions; `path` is the schema node's key.
#[allow(clippy::too_many_arguments)]
fn draw_tables(
    ui: &mut egui::Ui,
    listings: &HashMap<ConnSchema, DbListState>,
    conn: &DbConnection,
    catalog: Option<&str>,
    schema: &str,
    path: &str,
    indent: f32,
    action: &mut DbTreeAction,
) {
    match listings.get(&(conn.id.clone(), path.to_string())) {
        None | Some(DbListState::Loading) => loading_row(ui, indent),
        Some(DbListState::Error(msg)) => error_row(ui, indent, msg),
        Some(DbListState::Catalogs(_)) | Some(DbListState::Schemas(_)) => {}
        Some(DbListState::Tables(tables)) => {
            for table in tables {
                let resp = indented(ui, indent, |ui| {
                    ui.add(egui::Label::new(table).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(octa::i18n::t("db.open_hint"))
                });
                if resp.clicked() {
                    action.open = Some((
                        conn.id.clone(),
                        catalog.map(str::to_string),
                        schema.to_string(),
                        table.clone(),
                    ));
                }
                resp.context_menu(|ui| {
                    if ui.button(octa::i18n::t("db.copy_to")).clicked() {
                        action.copy = Some((
                            conn.id.clone(),
                            catalog.map(str::to_string),
                            schema.to_string(),
                            table.clone(),
                        ));
                        ui.close();
                    }
                    if ui
                        .button(octa::i18n::t("db.show_metadata"))
                        .on_hover_text(octa::i18n::t("db.show_metadata_hint"))
                        .clicked()
                    {
                        action.metadata = Some((
                            conn.id.clone(),
                            catalog.map(str::to_string),
                            schema.to_string(),
                            table.clone(),
                        ));
                        ui.close();
                    }
                });
            }
        }
    }
}

fn loading_row(ui: &mut egui::Ui, indent: f32) {
    indented(ui, indent, |ui| {
        ui.add(egui::Spinner::new().size(12.0));
        ui.label(octa::i18n::t("db.loading"));
    });
}

fn error_row(ui: &mut egui::Ui, indent: f32, msg: &str) {
    indented(ui, indent, |ui| {
        ui.colored_label(ui.visuals().error_fg_color, msg);
    });
}

/// Run `body` inside a left-indented horizontal row, returning its value.
fn indented<R>(ui: &mut egui::Ui, indent: f32, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        body(ui)
    })
    .inner
}

/// Short engine label after a connection name (full names in the tooltip).
fn engine_short(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Postgres => "Postgres",
        DbEngine::MySql => "MySQL",
        DbEngine::Mssql => "MSSQL",
        DbEngine::Redshift => "Redshift",
        DbEngine::ClickHouse => "ClickHouse",
        DbEngine::Exasol => "Exasol",
        DbEngine::Snowflake => "Snowflake",
        DbEngine::Databricks => "Databricks",
        DbEngine::BigQuery => "BigQuery",
    }
}
