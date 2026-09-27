//! The column navigator panel: a searchable list of the active table's
//! columns with show/hide, pin and reorder. It owns no state of its own. Every
//! toggle goes back to the same `hidden_columns` / frozen-column / column-move
//! state the Columns menu drives, so the two surfaces can never disagree.

use std::collections::HashSet;

use eframe::egui;

use octa::data::column_list::{ColumnEntry, column_entries, filter_entries, move_column};
use octa::i18n::t;
use octa::ui::settings::PanelPosition;

use super::state::OctaApp;

/// What the user asked for this frame. Read and applied by the caller; the
/// panel itself mutates nothing (interaction-struct convention).
#[derive(Default)]
pub(crate) struct ColumnNavigatorAction {
    /// Close the panel.
    pub close: bool,
    pub toggle_hidden: Option<usize>,
    pub set_frozen: Option<usize>,
    pub move_column: Option<(usize, usize)>,
    pub show_all: bool,
    pub hide_all: bool,
}

impl OctaApp {
    /// Toggle the panel open/closed. Wired to the View menu entry and the
    /// `ToggleColumnNavigator` shortcut (ships unbound).
    pub(crate) fn toggle_column_navigator(&mut self) {
        self.column_navigator_visible = !self.column_navigator_visible;
    }

    /// Render the docked column navigator. Returns early when not visible so
    /// calling it every frame is cheap.
    pub(crate) fn render_column_navigator(&mut self, ui: &mut egui::Ui) {
        if !self.column_navigator_visible {
            return;
        }
        let position = self.settings.column_navigator_position;
        let side = matches!(position, PanelPosition::Left | PanelPosition::Right);
        let available = if side {
            ui.available_width()
        } else {
            ui.available_height()
        };
        let (default_size, min_size) = octa::ui::panel_fit::clamp(available, 260.0, 160.0);
        let panel = match position {
            PanelPosition::Left => egui::Panel::left("octa_column_navigator"),
            PanelPosition::Right => egui::Panel::right("octa_column_navigator"),
            PanelPosition::Top => egui::Panel::top("octa_column_navigator"),
            PanelPosition::Bottom => egui::Panel::bottom("octa_column_navigator"),
        };
        let mut action = ColumnNavigatorAction::default();
        panel
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .show(ui, |ui| {
                // Fill both axes, or the panel snaps back to its content next
                // frame: egui's own `Panel` already fills the cross axis for
                // us, but the resize axis (width for Left/Right, height for
                // Top/Bottom) is only floored at the panel's *minimum* size
                // unless we claim the space ourselves. Setting both
                // unconditionally covers every `PanelPosition` with one line
                // pair instead of branching on orientation.
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                self.draw_column_navigator_body(ui, &mut action);
            });
        self.apply_column_navigator_action(action);
    }

    fn draw_column_navigator_body(
        &mut self,
        ui: &mut egui::Ui,
        action: &mut ColumnNavigatorAction,
    ) {
        let readonly = self.is_readonly();
        let (entries, frozen_cols) = {
            let tab = &self.tabs[self.active_tab];
            (
                column_entries(&tab.table, &tab.hidden_columns, tab.table_state.frozen_cols),
                tab.table_state.frozen_cols,
            )
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("navigator.title")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("X")
                    .on_hover_text(t("panel.close_hint"))
                    .clicked()
                {
                    action.close = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.column_navigator_query)
                    .hint_text(t("navigator.search")),
            )
            .on_hover_text(t("navigator.search_hint"));
            if ui
                .button(t("navigator.show_all"))
                .on_hover_text(t("navigator.show_all_hint"))
                .clicked()
            {
                action.show_all = true;
            }
            if ui
                .button(t("navigator.hide_all"))
                .on_hover_text(t("navigator.hide_all_hint"))
                .clicked()
            {
                action.hide_all = true;
            }
        });
        ui.separator();

        let filtered = filter_entries(&entries, &self.column_navigator_query);
        if filtered.is_empty() {
            ui.weak(t("navigator.empty"));
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &filtered {
                    draw_navigator_row(ui, entry, frozen_cols, readonly, action);
                }
            });
    }

    /// Write `action` back into the active tab's `hidden_columns`, frozen
    /// count and column order. The single place either the panel's own
    /// controls or a future caller can reach these, so they can never
    /// disagree with the Columns menu.
    fn apply_column_navigator_action(&mut self, action: ColumnNavigatorAction) {
        if action.close {
            self.column_navigator_visible = false;
        }
        if action.show_all {
            self.tabs[self.active_tab].hidden_columns.clear();
        }
        if action.hide_all {
            let n = self.tabs[self.active_tab].table.col_count();
            // Keep column 0 visible so the table never ends up with zero
            // visible columns.
            self.tabs[self.active_tab].hidden_columns = (1..n).collect::<HashSet<usize>>();
        }
        if let Some(idx) = action.toggle_hidden {
            let hidden = &mut self.tabs[self.active_tab].hidden_columns;
            if !hidden.insert(idx) {
                hidden.remove(&idx);
            }
        }
        if let Some(new_frozen) = action.set_frozen {
            self.tabs[self.active_tab].table_state.frozen_cols = new_frozen;
        }
        if let Some((from, to)) = action.move_column
            && !self.is_readonly()
        {
            let tab = &mut self.tabs[self.active_tab];
            let n = tab.table.col_count();
            if from < n && to < n && from != to {
                let order = move_column(n, from, to);
                tab.table.reorder_columns(&order);
                tab.table_state.widths_initialized = false;
            }
        }
    }
}

/// One row: eye toggle, drag handle, name, type, pin toggle. A free function
/// (not a method) since it needs no access to `OctaApp` beyond what is
/// already passed in.
fn draw_navigator_row(
    ui: &mut egui::Ui,
    entry: &ColumnEntry,
    frozen_cols: usize,
    readonly: bool,
    action: &mut ColumnNavigatorAction,
) {
    let row_id = ui.id().with(("octa_column_navigator_row", entry.index));
    let row = ui.horizontal(|ui| {
        let drag_id = ui.id().with(("octa_column_navigator_drag", entry.index));
        if readonly {
            ui.add_enabled(false, egui::Label::new("::").sense(egui::Sense::hover()))
                .on_disabled_hover_text(t("navigator.drag_disabled_hint"));
        } else {
            ui.dnd_drag_source(drag_id, entry.index, |ui| {
                ui.label("::");
            })
            .response
            .on_hover_text(t("navigator.drag_hint"));
        }

        let mut visible = !entry.hidden;
        if ui
            .checkbox(&mut visible, "")
            .on_hover_text(t("navigator.visible_hint"))
            .changed()
        {
            action.toggle_hidden = Some(entry.index);
        }

        ui.label(&entry.name);
        ui.weak(&entry.data_type);

        if ui
            .selectable_label(entry.frozen, t("navigator.freeze"))
            .on_hover_text(t("navigator.freeze_hint"))
            .clicked()
        {
            action.set_frozen = Some(if entry.index + 1 == frozen_cols {
                0
            } else {
                entry.index + 1
            });
        }
    });
    let row_rect = row.response.rect;

    if !readonly {
        let drop = ui.interact(row_rect, row_id, egui::Sense::hover());
        if let Some(from) = drop.dnd_release_payload::<usize>() {
            action.move_column = Some((*from, entry.index));
        }
        if drop.dnd_hover_payload::<usize>().is_some() {
            ui.painter().rect_stroke(
                row_rect,
                2.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Middle,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel must fill its dock or egui persists the body's rect and the
    /// panel snaps back to its content on the next frame. This pins the one
    /// line that prevents it, the same defect fixed for the SQL pane - but on
    /// a left/right dock the axis egui needs filled is *width*, not height
    /// (egui's own `Panel` already fills the cross axis for us; see
    /// `containers/panel.rs`'s `show_inside_dyn`, which sets the content
    /// ui's min height to the full cross-axis extent and its min width to
    /// only the panel's *minimum* size, so a body narrower than the dragged
    /// width silently reports that narrower width back and the panel snaps
    /// to it next frame).
    #[test]
    fn navigator_panel_keeps_a_dragged_width() {
        let ctx = egui::Context::default();
        let width = std::cell::Cell::new(0.0_f32);
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let resp = egui::Panel::left("octa_column_navigator")
                    .resizable(true)
                    .default_size(260.0)
                    .min_size(160.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label("one short row");
                    });
                width.set(resp.response.rect.width());
            });
            out.textures_delta.clear();
        }
        assert!(
            (width.get() - 260.0).abs() < 8.0,
            "panel settled at {}px, expected its 260px default",
            width.get()
        );
    }
}
