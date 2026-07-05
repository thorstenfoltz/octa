//! "Rename columns" dialog. The buffer is pre-seeded with every column of the
//! active tab, one name per line; the user appends `,newname` to the lines they
//! want to rename and leaves the rest untouched (a line with no new name keeps
//! its column). The preview shows matched / unmatched / colliding entries, and
//! Apply renames every matched column as one undoable step. Driven by
//! `OctaApp.rename_columns_state`.

use eframe::egui;
use egui::RichText;

use octa::data::rename_map::{parse_mapping, plan_renames};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::file_io::resync_db_meta_baseline;
use super::super::state::OctaApp;

pub(crate) fn render_rename_columns_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.rename_columns_state.is_none() {
        return;
    }
    if app.is_readonly() {
        app.rename_columns_state = None;
        return;
    }
    let mut state = app.rename_columns_state.take().unwrap();
    let mut close = false;
    let mut apply = false;

    // Current column names for the live preview.
    let columns: Vec<String> = app
        .tabs
        .get(app.active_tab)
        .map(|t| t.table.columns.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();

    let pairs = parse_mapping(&state.input_buf);
    let plan = plan_renames(&columns, &pairs);

    let dialog_id = egui::Id::new("octa_rename_columns_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_rename_columns")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(460.0).default_height(440.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("rename_columns_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.rename_title"))
                            .strong()
                            .size(16.0),
                    );
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

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.rename_input_hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("rename_input_scroll")
                .max_height(150.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.input_buf)
                            .desired_width(f32::INFINITY)
                            .desired_rows(8)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("column,new_name"),
                    );
                });

            ui.add_space(4.0);
            if ui
                .button(octa::i18n::t("dialog.rename_load_file"))
                .clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                if !state.input_buf.is_empty() && !state.input_buf.ends_with('\n') {
                    state.input_buf.push('\n');
                }
                state.input_buf.push_str(&text);
            }

            ui.separator();

            // Preview lists.
            egui::ScrollArea::vertical()
                .id_salt("rename_preview_scroll")
                .max_height(160.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} ({})",
                            octa::i18n::t("dialog.rename_matched"),
                            plan.matched.len()
                        ))
                        .strong(),
                    );
                    for (_, old, new) in &plan.matched {
                        ui.label(format!("{old}  ->  {new}"));
                    }
                    if !plan.unmatched.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "{} ({})",
                                octa::i18n::t("dialog.rename_unmatched"),
                                plan.unmatched.len()
                            ))
                            .strong()
                            .color(ui.visuals().warn_fg_color),
                        );
                        for old in &plan.unmatched {
                            ui.label(old);
                        }
                    }
                    if !plan.collisions.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "{} ({})",
                                octa::i18n::t("dialog.rename_collisions"),
                                plan.collisions.len()
                            ))
                            .strong()
                            .color(ui.visuals().error_fg_color),
                        );
                        for c in &plan.collisions {
                            ui.label(c);
                        }
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_apply = plan.collisions.is_empty() && !plan.matched.is_empty();
                if ui
                    .add_enabled(
                        can_apply,
                        egui::Button::new(octa::i18n::t("dialog.rename_apply")),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if !plan.collisions.is_empty() {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.rename_blocked"))
                            .size(10.0)
                            .color(ui.visuals().error_fg_color),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || apply {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if apply {
        if let Some(tab) = app.tabs.get_mut(app.active_tab) {
            let start = tab.table.undo_stack.len();
            for (index, _old, new) in &plan.matched {
                tab.table.rename_column(*index, new.clone());
            }
            tab.table.coalesce_undo_since(start);
            // Keep the DB diff-save baseline in step with the new names, the
            // same way clean-headers-on-load does, so a later save is not
            // rejected as a schema change.
            resync_db_meta_baseline(tab);
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
        }
        app.status_message = Some((
            octa::i18n::t("dialog.rename_done"),
            std::time::Instant::now(),
        ));
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.rename_columns_state = Some(state);
    }
}
