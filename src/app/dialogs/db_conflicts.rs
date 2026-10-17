//! The choices a write-back conflict needs (see `db::write_back::merge_plan`).
//!
//! Raised by the save itself, never opened on demand, so it has no shortcut.
//! Forced choice: Save with these choices, or Cancel (keep the edits, save
//! nothing). Values are selectable so they can be copied.

use eframe::egui;
use octa::db::write_back::{ConflictKind, Resolution, RowConflict, apply_resolutions};
use octa::i18n::t;

use crate::app::state::OctaApp;

pub(crate) struct DbConflictsState {
    pub(crate) tab_idx: usize,
    pub(crate) conflicts: Vec<RowConflict>,
    /// One per conflict, parallel to `conflicts`.
    pub(crate) choices: Vec<Resolution>,
    /// The save's new columns were added before the conflict was found
    /// (`WriteConflicts::columns_added`), so not quite nothing was saved.
    pub(crate) columns_added: bool,
}

fn cell_text(v: &octa::data::CellValue) -> String {
    match v {
        octa::data::CellValue::Null => t("db_conflicts.null"),
        other => other.to_string(),
    }
}

/// The labels and hints for one conflict's two choices.
fn choice_labels(kind: &ConflictKind) -> [(&'static str, &'static str); 2] {
    match kind {
        ConflictKind::Cells { .. } => [
            ("db_conflicts.keep_mine", "db_conflicts.keep_mine_hint"),
            ("db_conflicts.take_theirs", "db_conflicts.take_theirs_hint"),
        ],
        ConflictKind::DeletedOnServer => [
            ("db_conflicts.add_back", "db_conflicts.add_back_hint"),
            ("db_conflicts.let_go", "db_conflicts.let_go_hint"),
        ],
        ConflictKind::ChangedOnServer { .. } => [
            (
                "db_conflicts.delete_anyway",
                "db_conflicts.delete_anyway_hint",
            ),
            ("db_conflicts.keep_theirs", "db_conflicts.keep_theirs_hint"),
        ],
    }
}

pub(crate) fn render_db_conflicts_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.db_conflicts.take() else {
        return;
    };
    if st.tab_idx >= app.tabs.len() {
        return; // tab closed: nothing to resolve
    }
    let busy = app.db_write_back_job.is_some();
    let (mut save, mut cancel) = (false, false);
    let default_pos = octa::ui::dialog_chrome::center_on_first_show(ctx, egui::vec2(640.0, 480.0));
    egui::Window::new(t("db_conflicts.title"))
        .collapsible(false)
        .resizable(true)
        .default_pos(default_pos)
        .default_size(egui::vec2(640.0, 480.0))
        .show(ctx, |ui| {
            ui.label(t(if st.columns_added {
                "db_conflicts.intro_cols"
            } else {
                "db_conflicts.intro"
            }));
            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .button(t("db_conflicts.all_mine"))
                    .on_hover_text(t("db_conflicts.all_mine_hint"))
                    .clicked()
                {
                    st.choices.fill(Resolution::KeepMine);
                }
                if ui
                    .button(t("db_conflicts.all_theirs"))
                    .on_hover_text(t("db_conflicts.all_theirs_hint"))
                    .clicked()
                {
                    st.choices.fill(Resolution::TakeTheirs);
                }
            });
            ui.separator();
            egui::ScrollArea::both().max_height(360.0).show(ui, |ui| {
                for (i, conflict) in st.conflicts.iter().enumerate() {
                    let key = conflict
                        .key
                        .iter()
                        .map(cell_text)
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(t("db_conflicts.row").replace("{key}", &key))
                                .strong(),
                        )
                        .selectable(true),
                    );
                    match &conflict.kind {
                        ConflictKind::Cells { cells, .. } => {
                            egui::Grid::new(("dbc_grid", i))
                                .striped(true)
                                .show(ui, |ui| {
                                    for h in [
                                        "db_conflicts.col_column",
                                        "db_conflicts.col_original",
                                        "db_conflicts.col_theirs",
                                        "db_conflicts.col_mine",
                                    ] {
                                        ui.label(egui::RichText::new(t(h)).strong());
                                    }
                                    ui.end_row();
                                    for c in cells {
                                        for text in [
                                            c.column.clone(),
                                            cell_text(&c.base),
                                            cell_text(&c.theirs),
                                            cell_text(&c.mine),
                                        ] {
                                            ui.add(egui::Label::new(text).selectable(true));
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                        ConflictKind::DeletedOnServer => {
                            ui.label(t("db_conflicts.deleted_on_server"));
                        }
                        ConflictKind::ChangedOnServer { .. } => {
                            ui.label(t("db_conflicts.changed_on_server"));
                        }
                    }
                    let [(mine_l, mine_h), (theirs_l, theirs_h)] = choice_labels(&conflict.kind);
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.radio_value(&mut st.choices[i], Resolution::KeepMine, t(mine_l))
                            .on_hover_text(t(mine_h));
                        ui.radio_value(&mut st.choices[i], Resolution::TakeTheirs, t(theirs_l))
                            .on_hover_text(t(theirs_h));
                    });
                    ui.add_space(8.0);
                }
            });
            ui.separator();
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new(t("db_conflicts.save")))
                    .on_hover_text(t("db_conflicts.save_hint"))
                    .on_disabled_hover_text(t("dialog.dwb_busy"))
                    .clicked()
                {
                    save = true;
                }
                if ui
                    .button(t("db_conflicts.cancel"))
                    .on_hover_text(t("db_conflicts.cancel_hint"))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
    if cancel {
        return;
    }
    if !save || busy {
        app.db_conflicts = Some(st);
        return;
    }
    let tab_idx = st.tab_idx;
    let tab = &mut app.tabs[tab_idx];
    apply_resolutions(&mut tab.table, &st.conflicts, &st.choices);
    // That cleared the undo history; the recipe's marks into it go too.
    tab.recipe_after_history_cleared();
    tab.filter_dirty = true;
    match app.db_write_back_prompt(tab_idx) {
        // Straight to the worker: the user has just said what to write.
        Ok(Some(prompt)) => app.spawn_db_write_back(prompt, ctx),
        Ok(None) => {
            // Every row went to their side: the tab equals the server now.
            let tab = &mut app.tabs[tab_idx];
            tab.table.apply_edits();
            super::db_write_back::retag_db_meta(tab);
            tab.table.clear_modified();
            tab.view_hold = None;
            app.status_message = Some((t("db.wb_no_changes"), std::time::Instant::now()));
        }
        // The resolved edits stay in the tab; Save tries again.
        Err(msg) => app.status_message = Some((msg, std::time::Instant::now())),
    }
}
