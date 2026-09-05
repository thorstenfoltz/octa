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
        has_json,
        has_yaml,
        readonly_mode,
        split_view,
        split_side_by_side,
        split_panes,
        zoom_percent,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.view")).color(colors.text_primary),
        |ui| {
            if !has_data {
                ui.weak(crate::i18n::t("menu.need_table"));
                return;
            }
            let is_table = current_view_mode == ViewMode::Table;
            let is_raw = current_view_mode == ViewMode::Raw;

            // Disable table view for notebook files (notebook view is the primary view)
            let table_enabled = !has_notebook;
            // These two can be disabled, and a disabled
            // widget shows only its disabled hover text,
            // so both variants carry the same hint.
            let table_btn = ui
                .add_enabled(
                    table_enabled,
                    egui::RadioButton::new(is_table, crate::i18n::t("view_menu.table")),
                )
                .on_hover_text(crate::i18n::t("view_menu.table_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.table_hint"));
            if table_btn.clicked() {
                action.view_mode_changed = Some(ViewMode::Table);
                ui.close();
            }
            let raw_btn = ui
                .add_enabled(
                    has_raw_content,
                    egui::RadioButton::new(is_raw, crate::i18n::t("view_menu.raw")),
                )
                .on_hover_text(crate::i18n::t("view_menu.raw_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.raw_hint"));
            if raw_btn.clicked() {
                action.view_mode_changed = Some(ViewMode::Raw);
                ui.close();
            }
            if has_markdown {
                let is_md = current_view_mode == ViewMode::Markdown;
                let md_btn = ui
                    .radio(is_md, crate::i18n::t("view_menu.markdown"))
                    .on_hover_text(crate::i18n::t("view_menu.markdown_hint"));
                if md_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Markdown);
                    ui.close();
                }
            }
            if has_notebook {
                let is_nb = current_view_mode == ViewMode::Notebook;
                let nb_btn = ui
                    .radio(is_nb, crate::i18n::t("view_menu.notebook"))
                    .on_hover_text(crate::i18n::t("view_menu.notebook_hint"));
                if nb_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Notebook);
                    ui.close();
                }
            }
            if has_epub {
                let is_epub = current_view_mode == ViewMode::EpubReader;
                let epub_btn = ui
                    .radio(is_epub, crate::i18n::t("view_menu.epub"))
                    .on_hover_text(crate::i18n::t("view_menu.epub_hint"));
                if epub_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::EpubReader);
                    ui.close();
                }
            }
            if has_map {
                let is_map = current_view_mode == ViewMode::Map;
                let map_btn = ui
                    .radio(is_map, crate::i18n::t("view_menu.map"))
                    .on_hover_text(crate::i18n::t("view_menu.map_hint"));
                if map_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Map);
                    ui.close();
                }
            }
            if has_record {
                let is_record = current_view_mode == ViewMode::Record;
                let record_btn = ui
                    .radio(is_record, crate::i18n::t("view_menu.record"))
                    .on_hover_text(crate::i18n::t("view_menu.record_hint"));
                if record_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Record);
                    ui.close();
                }
            }
            if has_json {
                let is_json_tree = current_view_mode == ViewMode::JsonTree;
                let json_btn = ui
                    .radio(is_json_tree, crate::i18n::t("view_menu.json_tree"))
                    .on_hover_text(crate::i18n::t("view_menu.json_tree_hint"));
                if json_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::JsonTree);
                    ui.close();
                }
            }
            if has_yaml {
                let is_yaml_tree = current_view_mode == ViewMode::YamlTree;
                let yaml_btn = ui
                    .radio(is_yaml_tree, crate::i18n::t("view_menu.yaml_tree"))
                    .on_hover_text(crate::i18n::t("view_menu.yaml_tree_hint"));
                if yaml_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::YamlTree);
                    ui.close();
                }
            }
            // Compare with... - always available; the click triggers a
            // file picker that loads the right side and switches the
            // active tab into Compare view.
            ui.separator();
            if ui
                .button(crate::i18n::t("view_menu.compare_with"))
                .on_hover_text(crate::i18n::t("view_menu.compare_with_hint"))
                .clicked()
            {
                action.compare_with = true;
                ui.close();
            }
            if has_source_path
                && ui
                    .button(crate::i18n::t("view_menu.compare_git"))
                    .on_hover_text(crate::i18n::t("view_menu.compare_git_hint"))
                    .clicked()
            {
                action.open_git_compare = true;
                ui.close();
            }

            // Reopen as... - re-read the file *already in this tab*
            // through a reader the user picks, for a file whose
            // extension lies about its format (a .log that is really
            // JSON). The File menu's "Open as..." is the same idea for a
            // file that is not open yet.
            if has_source_path {
                ui.separator();
                ui.menu_button(crate::i18n::t("view_menu.reopen_as"), |ui| {
                    for (key, reader) in OPEN_AS_FORMATS {
                        if ui.button(crate::i18n::t(key)).clicked() {
                            action.open_as = Some(reader);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("view_menu.reopen_as_hint"));
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
            let stacked_on = split_view && !split_side_by_side;
            let split_btn = ui
                .add_enabled(
                    is_table,
                    egui::Checkbox::new(&mut stacked_on.clone(), crate::i18n::t("view_menu.split")),
                )
                .on_hover_text(crate::i18n::t("view_menu.split_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.split_hint"));
            if split_btn.clicked() {
                action.toggle_split_view = true;
                ui.close();
            }
            let side_on = split_view && split_side_by_side;
            let split_side_btn = ui
                .add_enabled(
                    is_table,
                    egui::Checkbox::new(
                        &mut side_on.clone(),
                        crate::i18n::t("view_menu.split_side"),
                    ),
                )
                .on_hover_text(crate::i18n::t("view_menu.split_side_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.split_side_hint"));
            if split_side_btn.clicked() {
                action.toggle_split_side_by_side = true;
                ui.close();
            }

            // Band count. Plain buttons rather than a submenu of six: the
            // question a user has while looking at the split is "one more" or
            // "one fewer", not "how many in total". Both hints carry the range
            // and the "split first" precondition, since a disabled entry shows
            // only its disabled text.
            let can_add = split_view && split_panes < crate::ui::table_view::MAX_SPLIT_PANES;
            if ui
                .add_enabled(
                    is_table && can_add,
                    egui::Button::new(crate::i18n::t("view_menu.add_pane")),
                )
                .on_hover_text(crate::i18n::t("view_menu.add_pane_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.add_pane_hint"))
                .clicked()
            {
                action.add_split_pane = true;
                ui.close();
            }
            if ui
                .add_enabled(
                    is_table && split_view && split_panes > 2,
                    egui::Button::new(crate::i18n::t("view_menu.remove_pane")),
                )
                .on_hover_text(crate::i18n::t("view_menu.remove_pane_hint"))
                .on_disabled_hover_text(crate::i18n::t("view_menu.remove_pane_hint"))
                .clicked()
            {
                action.remove_split_pane = true;
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
            if zoom_percent != 100
                && ui
                    .button(crate::i18n::t("view_menu.zoom_reset"))
                    .on_hover_text(crate::i18n::t("view_menu.zoom_reset_hint"))
                    .clicked()
            {
                action.zoom_reset = true;
                ui.close();
            }
        },
    );
}
