//! "New tab group" / "Tab group" dialog: a name and one of the twelve mark
//! colours. Driven by `OctaApp.tab_group_dialog`.

use eframe::egui;
use egui::RichText;

use octa::data::MarkColor;
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_line, control_row, control_text_edit};
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};
use octa::ui::theme::ThemeColors;

use super::super::state::OctaApp;
use super::super::tabs::groups::GroupId;

pub(crate) struct TabGroupDialog {
    /// The group being renamed, or `None` for a new one.
    pub editing: Option<GroupId>,
    /// The tabs a new group starts with.
    pub members: Vec<usize>,
    pub name: String,
    pub colour: MarkColor,
}

pub(crate) fn render_tab_group_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut d) = app.tab_group_dialog.take() else {
        return;
    };
    // The group being edited was closed or ungrouped meanwhile.
    if d.editing
        .is_some_and(|g| !app.tab_groups.iter().any(|x| x.id == g))
    {
        return;
    }
    let mut close = false;
    let mut submit = false;

    let dialog_id = egui::Id::new("octa_tab_group_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|m| m.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(380.0, 200.0));
    let window = egui::Window::new("octa_tab_group")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(380.0).default_pos(center)
    });
    let (title, ok, ok_hint) = if d.editing.is_some() {
        ("tabgroup.title_edit", "tabgroup.save", "tabgroup.save_hint")
    } else {
        (
            "tabgroup.title_new",
            "tabgroup.create",
            "tabgroup.create_hint",
        )
    };

    let inner = window.show(ctx, |ui| {
        if dialog_header(ui, "tab_group_header", RichText::new(t(title)), &mut size) {
            close = true;
        }
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let mut enter = false;
            control_grid(ui, "tab_group_grid", |ui| {
                ui.label(t("tabgroup.name"))
                    .on_hover_text(t("tabgroup.name_hint"));
                let resp = control_text_edit(ui, 240.0, egui::TextEdit::singleline(&mut d.name))
                    .on_hover_text(t("tabgroup.name_hint"));
                enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.end_row();

                ui.label(t("tabgroup.colour"))
                    .on_hover_text(t("tabgroup.colour_hint"));
                control_row(ui, |ui| {
                    // Small square swatches, centred in the row's height.
                    ui.spacing_mut().interact_size.y = 18.0;
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for &c in MarkColor::ALL {
                        let stroke = if c == d.colour {
                            egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                        } else {
                            egui::Stroke::NONE
                        };
                        let swatch = egui::Button::new("")
                            .fill(ThemeColors::mark_swatch(c))
                            .stroke(stroke)
                            .min_size(egui::vec2(18.0, 18.0));
                        if ui
                            .add(swatch)
                            .on_hover_text(format!(
                                "{}\n{}",
                                c.label_t(),
                                t("tabgroup.colour_hint")
                            ))
                            .clicked()
                        {
                            d.colour = c;
                        }
                    }
                });
                ui.end_row();
            });

            ui.add_space(8.0);
            let named = !d.name.trim().is_empty();
            control_line(ui, |ui| {
                if ui
                    .add_enabled(named, egui::Button::new(t(ok)))
                    .on_hover_text(t(ok_hint))
                    .on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(
                        &t(ok_hint),
                        &t("tabgroup.name_needed"),
                    ))
                    .clicked()
                    || (enter && named)
                {
                    submit = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("tabgroup.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    let done = close || submit;
    ctx.data_mut(|m| m.insert_temp(size_key, if done { DialogSize::Normal } else { size }));

    if submit {
        let name = d.name.trim().to_string();
        match d.editing {
            Some(g) => {
                if let Some(group) = app.tab_groups.iter_mut().find(|x| x.id == g) {
                    group.name = name;
                    group.colour = d.colour;
                }
            }
            None => {
                app.create_group(name, d.colour, &d.members);
            }
        }
    } else if !close {
        app.tab_group_dialog = Some(d);
    }
}
