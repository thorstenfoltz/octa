//! The **View** menu: view modes, reopen-as, zoom, read-only and panel toggles.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use crate::data::ViewMode;

use super::OPEN_AS_FORMATS;
use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::i18n::t;
use crate::ui::menu_entry::{disabled_tip, menu_entry};

pub(super) fn view_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        has_source_path,
        current_view_mode,
        has_raw_content,
        has_markdown,
        has_notebook,
        has_epub,
        has_map,
        has_record,
        has_timeline,
        has_json,
        has_yaml,
        readonly_mode,
        show_invisibles,
        split_view,
        split_side_by_side,
        split_panes,
        selected_tabs,
        in_tile,
        zoom_percent,
        following,
        follow_block,
        auto_refresh_block,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.view")).color(colors.text_primary),
        |ui| {
            let is_table = current_view_mode == ViewMode::Table;
            let is_raw = current_view_mode == ViewMode::Raw;
            let no_table = (!has_data).then(|| t("greyed.need_table"));
            let radio = |ui: &mut Ui, on: bool, label: &str, hint: &str, why: Option<String>| {
                let resp = ui.add_enabled(why.is_none(), egui::RadioButton::new(on, t(label)));
                match why {
                    None => resp.on_hover_text(t(hint)).clicked(),
                    Some(why) => {
                        resp.on_disabled_hover_text(disabled_tip(&t(hint), &why));
                        false
                    }
                }
            };
            let check = |ui: &mut Ui, on: bool, label: &str, hint: &str, why: Option<String>| {
                let resp = ui.add_enabled(
                    why.is_none(),
                    egui::Checkbox::new(&mut on.clone(), t(label)),
                );
                match why {
                    None => resp.on_hover_text(t(hint)).clicked(),
                    Some(why) => {
                        resp.on_disabled_hover_text(disabled_tip(&t(hint), &why));
                        false
                    }
                }
            };
            // The view's own reason when the tab lacks it, after "no table".
            let view_why =
                |has: bool, key: &str| no_table.clone().or_else(|| (!has).then(|| t(key)));

            // A notebook has no Table view (`available_view_modes`): the
            // Notebook view shows its cells, the JSON tree its structure.
            if radio(
                ui,
                is_table,
                "view_menu.table",
                "view_menu.table_hint",
                view_why(!has_notebook, "greyed.view_table_notebook"),
            ) {
                action.view_mode_changed = Some(ViewMode::Table);
                ui.close();
            }
            if radio(
                ui,
                is_raw,
                "view_menu.raw",
                "view_menu.raw_hint",
                // A text-only tab (File -> New file) has no table but does have Raw.
                if !has_data && !has_raw_content {
                    no_table.clone()
                } else {
                    (!has_raw_content).then(|| t("greyed.need_text"))
                },
            ) {
                action.view_mode_changed = Some(ViewMode::Raw);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::Markdown,
                "view_menu.markdown",
                "view_menu.markdown_hint",
                view_why(has_markdown, "greyed.view_markdown"),
            ) {
                action.view_mode_changed = Some(ViewMode::Markdown);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::Notebook,
                "view_menu.notebook",
                "view_menu.notebook_hint",
                view_why(has_notebook, "greyed.view_notebook"),
            ) {
                action.view_mode_changed = Some(ViewMode::Notebook);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::EpubReader,
                "view_menu.epub",
                "view_menu.epub_hint",
                view_why(has_epub, "greyed.view_epub"),
            ) {
                action.view_mode_changed = Some(ViewMode::EpubReader);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::Map,
                "view_menu.map",
                "view_menu.map_hint",
                view_why(has_map(), "greyed.view_map"),
            ) {
                action.view_mode_changed = Some(ViewMode::Map);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::Record,
                "view_menu.record",
                "view_menu.record_hint",
                view_why(has_record, "greyed.view_record"),
            ) {
                action.view_mode_changed = Some(ViewMode::Record);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::Timeline,
                "view_menu.timeline",
                "view_menu.timeline_hint",
                view_why(has_timeline, "greyed.view_timeline"),
            ) {
                action.view_mode_changed = Some(ViewMode::Timeline);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::JsonTree,
                "view_menu.json_tree",
                "view_menu.json_tree_hint",
                view_why(has_json, "greyed.view_json"),
            ) {
                action.view_mode_changed = Some(ViewMode::JsonTree);
                ui.close();
            }
            if radio(
                ui,
                current_view_mode == ViewMode::YamlTree,
                "view_menu.yaml_tree",
                "view_menu.yaml_tree_hint",
                view_why(has_yaml, "greyed.view_yaml"),
            ) {
                action.view_mode_changed = Some(ViewMode::YamlTree);
                ui.close();
            }
            // Compare with... - always available; the click triggers a
            // file picker that loads the right side and switches the
            // active tab into Compare view.
            ui.separator();
            let why_file = (!has_source_path).then(|| t("greyed.need_file"));
            if menu_entry(
                ui,
                &t("view_menu.compare_with"),
                &t("view_menu.compare_with_hint"),
                no_table.as_deref(),
            ) {
                action.compare_with = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &t("view_menu.compare_git"),
                &t("view_menu.compare_git_hint"),
                why_file.as_deref(),
            ) {
                action.open_git_compare = true;
                ui.close();
            }

            // Reopen as... - re-read the file *already in this tab*
            // through a reader the user picks, for a file whose
            // extension lies about its format (a .log that is really
            // JSON). The File menu's "Open as..." is the same idea for a
            // file that is not open yet.
            ui.separator();
            let reopen_hint = t("view_menu.reopen_as_hint");
            let reopen = ui
                .add_enabled_ui(has_source_path, |ui| {
                    ui.menu_button(t("view_menu.reopen_as"), |ui| {
                        for (key, reader) in OPEN_AS_FORMATS {
                            if ui.button(t(key)).clicked() {
                                action.open_as = Some(reader);
                                ui.close();
                            }
                        }
                    })
                    .response
                })
                .inner;
            match &why_file {
                None => reopen.on_hover_text(reopen_hint),
                Some(why) => reopen.on_disabled_hover_text(disabled_tip(&reopen_hint, why)),
            };

            ui.separator();
            if check(
                ui,
                following,
                "live.follow",
                "live.follow_hint",
                follow_block.map(str::to_string),
            ) {
                action.toggle_follow = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &t("live.auto_refresh"),
                &t("live.auto_refresh_hint"),
                auto_refresh_block,
            ) {
                action.open_auto_refresh = true;
                ui.close();
            }

            ui.separator();
            // Split view only means anything in the table view; the other
            // modes render their own way. Disabled elsewhere, with the same
            // hint on both variants because a disabled widget shows only the
            // disabled text.
            //
            // One entry per orientation rather than a split toggle plus an
            // orientation toggle: each checkbox says exactly what it does, and
            // swapping orientation while split is one click, not two.
            let why_split = no_table
                .clone()
                .or_else(|| (!is_table).then(|| t("greyed.need_table_view")));
            let stacked_on = split_view && !split_side_by_side;
            if check(
                ui,
                stacked_on,
                "view_menu.split",
                "view_menu.split_hint",
                why_split.clone(),
            ) {
                action.toggle_split_view = true;
                ui.close();
            }
            let side_on = split_view && split_side_by_side;
            if check(
                ui,
                side_on,
                "view_menu.split_side",
                "view_menu.split_side_hint",
                why_split.clone(),
            ) {
                action.toggle_split_side_by_side = true;
                ui.close();
            }

            // Band count. Plain buttons rather than a submenu of six: the
            // question a user has while looking at the split is "one more" or
            // "one fewer", not "how many in total".
            let why_add = why_split.clone().or_else(|| {
                if !split_view {
                    Some(t("greyed.need_split"))
                } else if split_panes >= crate::ui::table_view::MAX_SPLIT_PANES {
                    Some(t("greyed.split_max"))
                } else {
                    None
                }
            });
            if menu_entry(
                ui,
                &t("view_menu.add_pane"),
                &t("view_menu.add_pane_hint"),
                why_add.as_deref(),
            ) {
                action.add_split_pane = true;
                ui.close();
            }
            let why_remove = why_split.or_else(|| {
                if !split_view {
                    Some(t("greyed.need_split"))
                } else if split_panes <= 2 {
                    Some(t("greyed.split_min"))
                } else {
                    None
                }
            });
            if menu_entry(
                ui,
                &t("view_menu.remove_pane"),
                &t("view_menu.remove_pane_hint"),
                why_remove.as_deref(),
            ) {
                action.remove_split_pane = true;
                ui.close();
            }

            // Tile tabs: show the selected tabs in the window at once. In
            // place, so no ellipsis.
            ui.separator();
            if let Some(l) = crate::ui::menu_entry::tile_submenu(
                ui,
                &t("tiles.tile"),
                &t("tiles.tile_hint"),
                (selected_tabs < 2)
                    .then(|| t("greyed.need_two_tabs"))
                    .as_deref(),
                selected_tabs,
            ) {
                action.tile = Some(l);
            }
            let not_tiled = (!in_tile).then(|| t("greyed.not_tiled"));
            if menu_entry(
                ui,
                &t("tiles.untile"),
                &t("tiles.untile_hint"),
                not_tiled.as_deref(),
            ) {
                action.untile = true;
                ui.close();
            }

            ui.separator();
            if ui
                .checkbox(
                    &mut readonly_mode.clone(),
                    crate::i18n::t("view_menu.readonly"),
                )
                .on_hover_text(crate::i18n::t("view_menu.readonly_hint"))
                .clicked()
            {
                action.toggle_readonly = true;
                ui.close();
            }
            if ui
                .checkbox(
                    &mut show_invisibles.clone(),
                    crate::i18n::t("view_menu.show_invisibles"),
                )
                .on_hover_text(crate::i18n::t("view_menu.show_invisibles_hint"))
                .clicked()
            {
                action.toggle_invisibles = true;
                ui.close();
            }

            ui.separator();
            ui.label(
                RichText::new(crate::i18n::t("view_menu.zoom"))
                    .strong()
                    .size(11.0)
                    .color(colors.text_muted),
            );
            ui.horizontal(|ui| {
                if ui.button("-").clicked() {
                    action.zoom_out = true;
                }
                ui.label(format!("{}%", zoom_percent));
                if ui.button("+").clicked() {
                    action.zoom_in = true;
                }
            });
            let why_zoom = (zoom_percent == 100).then(|| t("greyed.zoom_is_100"));
            if menu_entry(
                ui,
                &t("view_menu.zoom_reset"),
                &t("view_menu.zoom_reset_hint"),
                why_zoom.as_deref(),
            ) {
                action.zoom_reset = true;
                ui.close();
            }
        },
    );
}
