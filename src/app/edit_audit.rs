//! The edit audit trail panel: a docked list of every pending (unsaved) cell
//! edit in the active tab, before and after, with a jump-to-cell button and a
//! per-row revert. Pending cell edits only - `DataTable.edits` is an overlay
//! over the loaded rows, so structural changes (rows or columns added or
//! removed) never appear here.

use eframe::egui;

use octa::data::DataTable;
use octa::data::edit_audit::{AuditEntry, audit_entries};

use octa::db::write_back::edits_as_update_sql;
use octa::i18n::t;
use octa::ui::settings::PanelPosition;

use super::state::{OctaApp, TabState};

/// What the user asked for this frame. Read and applied by the caller; the
/// panel itself mutates nothing (interaction-struct convention).
#[derive(Default)]
pub(crate) struct EditAuditAction {
    /// Close the panel.
    pub close: bool,
    pub jump_to: Option<(usize, usize)>,
    pub revert: Option<(usize, usize)>,
    /// Copy every pending edit as `UPDATE` SQL, one statement per row.
    pub copy_sql: bool,
}

/// The name to put in a copied `UPDATE` statement for a table with no live
/// database origin.
///
/// A SQLite or DuckDB file knows the real name of the table that was read out
/// of it (`db_meta`), and that is the name the statements have to carry: the
/// file stem is the name of the *database*, so `customers` inside
/// `warehouse.sqlite` would otherwise export as `UPDATE "warehouse"`. Only a
/// plain file falls back to its stem, and then to the tab's own label.
fn file_backed_table_name(tab: &TabState, idx: usize) -> String {
    if let Some(meta) = &tab.table.db_meta {
        return meta.table_name.clone();
    }
    tab.table
        .source_path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .or_else(|| tab.custom_tab_label.clone())
        .unwrap_or_else(|| format!("Untitled {}", idx + 1))
}

/// Whether the active tab has a real row key to address `UPDATE`s by. `false`
/// covers both a file-backed tab (no server at all) and a live-database tab
/// whose engine or schema offered none - both get the same first-column
/// template and the same on-screen warning.
fn is_sql_export_template(tab: &TabState) -> bool {
    tab.db_origin.as_ref().is_none_or(|o| o.identity.is_none())
}

/// Revert one pending edit, and only that one.
///
/// `DataTable::set` is the whole shared edit path needed here: it pushes an
/// `UndoAction::CellEdit`, so a revert is itself undoable, and it writes
/// into the pending-edit overlay rather than behind the undo system's back.
/// Removing the key afterwards is what turns "set it back" into "there is
/// no longer a pending edit here".
///
/// **Not `apply_edit_ops`.** That helper finishes by calling
/// `DataTable::apply_edits()`, which commits EVERY pending edit into the
/// rows and empties the overlay. Reverting one row therefore made every
/// other pending edit permanent and emptied the trail, which read from the
/// outside as "revert reverts everything" and was in fact worse: the other
/// edits were not reverted, they were silently applied.
pub(crate) fn revert_one(table: &mut DataTable, row: usize, col: usize) {
    let Some(original) = table.original_value(row, col).cloned() else {
        return;
    };
    table.set(row, col, original);
    table.edits.remove(&(row, col));
}

/// Reverting is an edit, so read-only mode forbids it. Viewing the trail is
/// never gated: the whole point is to see what is pending.
pub(crate) fn revert_allowed(readonly: bool) -> bool {
    !readonly
}

impl OctaApp {
    /// Toggle the panel open/closed. Wired to the View menu entry and the
    /// `ToggleEditAudit` shortcut (ships unbound).
    pub(crate) fn toggle_edit_audit(&mut self) {
        self.edit_audit_visible = !self.edit_audit_visible;
    }

    /// Render the docked edit audit trail. Returns early when not visible so
    /// calling it every frame is cheap.
    pub(crate) fn render_edit_audit(&mut self, ui: &mut egui::Ui) {
        if !self.edit_audit_visible {
            return;
        }
        let position = self.settings.edit_audit_position;
        let side = matches!(position, PanelPosition::Left | PanelPosition::Right);
        let available = if side {
            ui.available_width()
        } else {
            ui.available_height()
        };
        let (default_size, min_size) = octa::ui::panel_fit::clamp(available, 220.0, 140.0);
        let panel = match position {
            PanelPosition::Left => egui::Panel::left("octa_edit_audit"),
            PanelPosition::Right => egui::Panel::right("octa_edit_audit"),
            PanelPosition::Top => egui::Panel::top("octa_edit_audit"),
            PanelPosition::Bottom => egui::Panel::bottom("octa_edit_audit"),
        };
        let mut action = EditAuditAction::default();
        panel
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .show(ui, |ui| {
                // Fill both axes, or the panel snaps back to its content next
                // frame: see the identical comment in `column_navigator.rs`.
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                self.draw_edit_audit_body(ui, &mut action);
            });
        let ctx = ui.ctx().clone();
        self.apply_edit_audit_action(action, &ctx);
    }

    fn draw_edit_audit_body(&mut self, ui: &mut egui::Ui, action: &mut EditAuditAction) {
        let readonly = self.is_readonly();
        let tab = &self.tabs[self.active_tab];
        let entries = audit_entries(&tab.table);
        let row_offset = tab.table.row_offset;
        let partial = tab.table.partial_note();
        let is_template = is_sql_export_template(tab);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(
                    t("edit_audit.title").replace("{count}", &entries.len().to_string()),
                )
                .strong(),
            );
            let hint_key = if is_template {
                "edit_audit.copy_sql_template_hint"
            } else {
                "edit_audit.copy_sql_hint"
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("X")
                    .on_hover_text(t("panel.close_hint"))
                    .clicked()
                {
                    action.close = true;
                }
            });
            let enabled = !entries.is_empty();
            let resp = ui.add_enabled(enabled, egui::Button::new(t("edit_audit.copy_sql")));
            let resp = if enabled {
                resp.on_hover_text(t(hint_key))
            } else {
                resp.on_disabled_hover_text(t("edit_audit.copy_sql_empty_hint"))
            };
            if resp.clicked() {
                action.copy_sql = true;
            }
        });
        if is_template && !entries.is_empty() {
            ui.weak(t("edit_audit.template_note"));
        }
        if let Some((loaded, known_total)) = partial {
            octa::ui::message::partial_note(ui, loaded, known_total);
        }
        ui.separator();

        if entries.is_empty() {
            ui.weak(t("edit_audit.empty"));
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &entries {
                    draw_edit_audit_row(ui, entry, row_offset, readonly, action);
                }
            });
    }

    /// Apply `action` back into the active tab: jump moves the selection and
    /// scrolls it into view, revert lands an undoable `CellEdit`, copy-as-SQL
    /// puts the rendered statements on the clipboard. The single place either
    /// reaches the table, matching the column navigator's interaction-struct
    /// convention. Copying is read-only by nature (it changes no data), so it
    /// is never gated by `is_readonly()`.
    fn apply_edit_audit_action(&mut self, action: EditAuditAction, ctx: &egui::Context) {
        if let Some((row, col)) = action.jump_to {
            self.jump_to_cell(row, col);
        }
        if let Some((row, col)) = action.revert
            && !self.is_readonly()
        {
            revert_one(&mut self.tabs[self.active_tab].table, row, col);
        }
        if action.copy_sql {
            self.copy_edit_audit_sql(ctx);
        }
        if action.close {
            self.edit_audit_visible = false;
        }
    }

    /// Resolve the active tab's SQL target (engine/schema/table/key) and copy
    /// the edit trail as `UPDATE` statements. A live-database tab uses its own
    /// origin; anything else (including a database tab whose engine offers no
    /// key) falls back to a Postgres-quoted template named after the file.
    fn copy_edit_audit_sql(&mut self, ctx: &egui::Context) {
        let idx = self.active_tab;
        let tab = &self.tabs[idx];
        let (engine, schema, table_name, identity) = match &tab.db_origin {
            Some(origin) => {
                let engine = self
                    .settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == origin.conn_id)
                    .map(|c| c.engine)
                    .unwrap_or_default();
                (
                    engine,
                    origin.schema.clone(),
                    origin.table.clone(),
                    origin.identity.clone(),
                )
            }
            // No live connection. A SQLite or DuckDB file still knows its own
            // table and schema, so carry those; anything else is named after
            // the file and has no schema.
            None => (
                octa::db::DbEngine::Postgres,
                tab.table
                    .db_meta
                    .as_ref()
                    .and_then(|m| m.schema.clone())
                    .unwrap_or_default(),
                file_backed_table_name(tab, idx),
                None,
            ),
        };
        match edits_as_update_sql(&tab.table, engine, &schema, &table_name, identity.as_ref()) {
            Ok(export) => {
                ctx.copy_text(export.statements.join("\n"));
                self.status_message = Some((t("edit_audit.copied"), std::time::Instant::now()));
            }
            Err(e) => {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
            }
        }
    }
}

/// One row: row number, column name, before/after, jump and revert.
fn draw_edit_audit_row(
    ui: &mut egui::Ui,
    entry: &AuditEntry,
    row_offset: usize,
    readonly: bool,
    action: &mut EditAuditAction,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{}", entry.row + 1 + row_offset));
        ui.label(&entry.column_name);
        ui.label(egui::RichText::new(format!("{} -> {}", entry.before, entry.after)).monospace());

        if ui
            .button(t("edit_audit.jump"))
            .on_hover_text(t("edit_audit.jump_hint"))
            .clicked()
        {
            action.jump_to = Some((entry.row, entry.col));
        }

        let enabled = revert_allowed(readonly);
        let revert_resp = ui.add_enabled(enabled, egui::Button::new(t("edit_audit.revert")));
        let revert_resp = if enabled {
            revert_resp.on_hover_text(t("edit_audit.revert_hint"))
        } else {
            revert_resp.on_disabled_hover_text(t("edit_audit.revert_readonly_hint"))
        };
        if revert_resp.clicked() {
            action.revert = Some((entry.row, entry.col));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::{CellValue, ColumnInfo, DataTable};

    fn edited_table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "city".into(),
            data_type: "text".into(),
        }];
        t.rows = vec![vec![CellValue::String("Aachen".into())]];
        t.edits.insert((0, 0), CellValue::String("Bonn".into()));
        t
    }

    /// Reverting one edit restores exactly that cell and leaves the rest of
    /// the overlay alone.
    #[test]
    fn revert_one_removes_only_that_edit() {
        let mut t = edited_table();
        t.edits.insert((0, 0), CellValue::String("Bonn".into()));
        revert_one(&mut t, 0, 0);
        assert!(t.edits.is_empty(), "the reverted edit is gone");
        assert_eq!(
            t.get(0, 0).map(|c| c.to_string()).as_deref(),
            Some("Aachen")
        );
    }

    /// Read-only mode blocks the revert but never the list.
    #[test]
    fn readonly_blocks_revert_but_not_viewing() {
        let t = edited_table();
        assert_eq!(octa::data::edit_audit::audit_entries(&t).len(), 1);
        assert!(!revert_allowed(true), "read-only refuses the revert");
        assert!(revert_allowed(false));
    }
}

#[cfg(test)]
mod revert_tests {
    use super::*;
    use octa::data::{CellValue, ColumnInfo};

    fn three_edits() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "a".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "b".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = vec![
            vec![
                CellValue::String("a0".into()),
                CellValue::String("b0".into()),
            ],
            vec![
                CellValue::String("a1".into()),
                CellValue::String("b1".into()),
            ],
        ];
        t.edits.insert((0, 0), CellValue::String("EDIT-00".into()));
        t.edits.insert((1, 0), CellValue::String("EDIT-10".into()));
        t.edits.insert((0, 1), CellValue::String("EDIT-01".into()));
        t
    }

    /// Reverting ONE edit reverts exactly one. Reported from real use as
    /// "it reverts everything", so this pins the count and the survivors,
    /// not just that the clicked one went.
    #[test]
    fn reverting_one_edit_leaves_the_others_alone() {
        let mut t = three_edits();
        assert_eq!(audit_entries(&t).len(), 3);

        revert_one(&mut t, 1, 0);

        let left = audit_entries(&t);
        assert_eq!(left.len(), 2, "exactly one edit went: {left:?}");
        assert_eq!(
            t.get(1, 0),
            Some(&CellValue::String("a1".into())),
            "the reverted cell is back to its original"
        );
        assert_eq!(
            t.get(0, 0),
            Some(&CellValue::String("EDIT-00".into())),
            "another edit in the same column survived"
        );
        assert_eq!(
            t.get(0, 1),
            Some(&CellValue::String("EDIT-01".into())),
            "another edit in the same row survived"
        );
    }

    /// Reverting every edit one at a time empties the trail, and does it
    /// one at a time rather than all at once on the first click.
    #[test]
    fn reverting_each_in_turn_empties_the_trail_gradually() {
        let mut t = three_edits();
        let mut expected = 3;
        while let Some(entry) = audit_entries(&t).first().cloned() {
            revert_one(&mut t, entry.row, entry.col);
            expected -= 1;
            assert_eq!(audit_entries(&t).len(), expected, "one at a time");
        }
        assert_eq!(expected, 0);
    }
}
