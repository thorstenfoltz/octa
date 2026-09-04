use egui::{RichText, Ui};

use crate::data::SearchMode;

mod analyse_menu;
mod columns_menu;
mod data_menu;
mod edit_menu;
mod file_menu;
mod help_menu;
mod menu_button;
mod search_menu;
mod types;
mod view_menu;

pub use types::{AskControls, ParseScope, SearchControls, ToolbarAction, ToolbarCtx};

/// Formats offered by **File -> Open as...** and **View -> Reopen as...**:
/// `(i18n label key, reader name as registered in `FormatRegistry::new`)`.
///
/// These are the text-shaped readers, the ones worth forcing a file through when
/// its extension is misleading. Kept in step with the registry by
/// `open_as_tests::every_open_as_reader_name_resolves`.
/// Thickness of the toolbar's horizontal scrollbar, and the strip it lives in.
/// Same treatment as the tab bar (`app::tabs`): egui's default *floating*
/// scrollbar is painted **over** the content, which lays it across the bottom of
/// the File / Edit / ... buttons. A solid bar in its own strip below the row
/// keeps the buttons clean, at the cost of `SCROLL_BAR_STRIP` panel height.
const SCROLL_BAR_WIDTH: f32 = 6.0;
const SCROLL_BAR_INNER_MARGIN: f32 = 2.0;
/// Height the toolbar's scrollbar strip claims. Callers add this to the toolbar
/// panel height (see `app::toolbar_handler`).
pub const SCROLL_BAR_STRIP: f32 = SCROLL_BAR_WIDTH + SCROLL_BAR_INNER_MARGIN;

const OPEN_AS_FORMATS: &[(&str, &str)] = &[
    ("open_as.json", "JSON"),
    ("open_as.jsonl", "JSON Lines"),
    ("open_as.csv", "CSV"),
    ("open_as.tsv", "TSV"),
    ("open_as.yaml", "YAML"),
    ("open_as.toml", "TOML"),
    ("open_as.xml", "XML"),
    ("open_as.markdown", "Markdown"),
    ("open_as.text", "Text"),
    ("open_as.sql_dump", "SQL dump"),
];

/// Draw the toolbar and report what the user chose.
///
/// Takes two bundles rather than the 53 positional parameters it used to: a
/// `Copy` [`ToolbarCtx`] of everything the menus read, and a
/// [`SearchControls`] holding the search bar's `&mut` state. That is what
/// retired the `#[allow(clippy::too_many_arguments)]` this function carried,
/// and it means a new toolbar input is a named field rather than one more
/// `bool` in a positional list of a dozen others.
pub fn draw_toolbar(ui: &mut Ui, cx: ToolbarCtx<'_>, search: SearchControls<'_>) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    // Destructured into locals with the original names so the bar's own body
    // below reads as it always did; `cx` stays whole (it is `Copy`) and is
    // handed to each menu.
    let ToolbarCtx {
        colors,
        logo_texture,
        show_window_controls,
        // The bar itself is hidden without a table; the menus stay visible.
        has_data,
        ..
    } = cx;
    let SearchControls {
        text: search_text,
        mode: search_mode,
        case_sensitive: search_case_sensitive,
        whole_word: search_whole_word,
        scope_col: search_scope_col,
        column_names,
        ask,
        history: search_history,
        result_mode: search_result_mode,
        highlight_active: search_highlight_active,
        match_count: search_match_count,
        match_current: search_match_current,
        focus_requested: search_focus_requested,
        show_replace_bar,
        replace_text,
        bookmarks,
    } = search;

    // Custom title bar: the toolbar background doubles as the window's drag
    // handle. Without system decorations this is the only way to move the
    // window (left-drag) or toggle maximise (double-click), matching the
    // standard title-bar gestures. We register this interaction FIRST, over
    // the whole toolbar rect, so the menus / search box / window-control
    // buttons added afterwards sit "on top" (egui breaks hit-test ties in
    // favour of the last-registered widget) and keep their own clicks - only
    // the leftover empty background drags the window.
    if show_window_controls {
        let title_rect = ui.max_rect();
        let drag = ui.interact(
            title_rect,
            ui.id().with("custom_title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag.double_clicked() {
            let is_max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
        } else if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    // Top-level menus go through `top_menu_button` (defined above), which
    // brings back the hover-switch behaviour egui 0.31's MenuRoot used to
    // provide and that egui 0.34's MenuButton dropped. Plain `ui.horizontal`
    // is enough here - we do *not* wrap in `egui::MenuBar`, because the
    // helper handles the menu/submenu plumbing itself.
    ui.horizontal(|ui| {
        // Pin a uniform row height for the whole toolbar. egui centres each
        // item against `interact_size.y` (default 18); the search ComboBoxes /
        // TextEdit are taller, so without this the short menu buttons and the
        // search widgets sit on different baselines and the search row looks
        // dropped. A 24px band matches the ComboBox height so menus, the mode
        // combo, Recent, Filter and the rest all share one centre line.
        ui.spacing_mut().interact_size.y = 24.0;
        // A plain (vertical) mouse wheel only reaches a horizontal-only scroll
        // area when this is set - egui defaults it to false, which would force
        // the user to hold Shift. Same override the tab bar needs.
        ui.style_mut().always_scroll_the_only_direction = true;
        // Solid (not floating) scrollbar, so it gets its own strip under the row
        // instead of being painted across the menu buttons.
        let mut scroll_style = egui::style::ScrollStyle::solid();
        scroll_style.bar_inner_margin = SCROLL_BAR_INNER_MARGIN;
        scroll_style.bar_outer_margin = 0.0;
        scroll_style.bar_width = SCROLL_BAR_WIDTH;
        ui.style_mut().spacing.scroll = scroll_style;

        // Reserve the window buttons' width up front so they stay pinned to the
        // right edge; everything else scrolls inside what is left. Without this
        // a narrow window (small laptop, or a locale with long menu labels)
        // pushes the close button off-screen with no way to reach it.
        let reserved = if show_window_controls {
            3.0 * 28.0 + 3.0 * ui.spacing().item_spacing.x
        } else {
            0.0
        };
        let scroll_width = (ui.available_width() - reserved).max(120.0);
        egui::ScrollArea::horizontal()
            .id_salt("toolbar_scroll")
            .max_width(scroll_width)
            .auto_shrink([false, false])
            // Wheel and scrollbar only: the empty toolbar background is the
            // window's drag handle (see above), and drag-to-scroll would
            // swallow that gesture.
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Never,
                ..egui::scroll_area::ScrollSource::ALL
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);

                    // App logo + title. The logo is wrapped as a clickable widget so the
                    // hidden easter-egg counter (seven clicks within ~1.5 s) can trigger.
                    if let Some(tex) = logo_texture {
                        let img =
                            egui::Image::new(egui::load::SizedTexture::new(tex.id(), [20.0, 20.0]))
                                .sense(egui::Sense::click());
                        let resp = ui.add(img);
                        if resp.clicked() {
                            action.logo_clicked = true;
                        }
                    }
                    ui.label(
                        RichText::new("Octa")
                            .strong()
                            .size(15.0)
                            .color(colors.accent),
                    );

                    ui.add_space(8.0);

                    // The eight top-level menus, one file each. Each takes the same
                    // `ToolbarCtx` and writes into the same `ToolbarAction`; the order of
                    // these calls is the order they appear on the bar.

                    file_menu::file_menu(ui, cx, &mut action);

                    // Every menu stays visible even before a table is open (the SQL panel
                    // and the Assistant work with attached servers alone); menus whose
                    // entries all need a table show a short note instead.
                    edit_menu::edit_menu(ui, cx, &mut action);
                    columns_menu::columns_menu(ui, cx, &mut action);
                    data_menu::data_menu(ui, cx, &mut action);
                    view_menu::view_menu(ui, cx, &mut action);
                    search_menu::search_menu(ui, cx, &mut action);
                    analyse_menu::analyse_menu(ui, cx, &mut action);
                    help_menu::help_menu(ui, cx, &mut action);

                    if has_data {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);

                        // Search box with mode selector
                        ui.label(RichText::new("Search:").color(colors.text_secondary));
                        let old_mode = *search_mode;
                        egui::ComboBox::from_id_salt("search_mode")
                            .width(75.0)
                            .selected_text(search_mode.label_t())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    search_mode,
                                    SearchMode::Plain,
                                    SearchMode::Plain.label_t(),
                                );
                                ui.selectable_value(
                                    search_mode,
                                    SearchMode::Wildcard,
                                    SearchMode::Wildcard.label_t(),
                                );
                                ui.selectable_value(
                                    search_mode,
                                    SearchMode::Regex,
                                    SearchMode::Regex.label_t(),
                                );
                            });
                        if *search_mode != old_mode {
                            action.search_changed = true;
                        }
                        // In Ask mode this same box is the question box, so it
                        // says so and gets room for a sentence.
                        let asking = *ask.mode && ask.enabled;
                        let ask_placeholder = crate::i18n::t("search.ask_placeholder");
                        let hint: &str = if asking {
                            &ask_placeholder
                        } else {
                            match *search_mode {
                                SearchMode::Plain => "Filter rows...",
                                SearchMode::Wildcard => "e.g. foo*bar, item?",
                                SearchMode::Regex => "e.g. ^\\d{3}-",
                            }
                        };
                        let search_id = ui.id().with("toolbar_search");
                        let response = ui.add(
                            egui::TextEdit::singleline(search_text)
                                .id(search_id)
                                .desired_width(if asking { 340.0 } else { 200.0 })
                                .hint_text(hint),
                        );
                        // A half-typed question is not a filter: while asking,
                        // typing must not narrow the table to nothing.
                        if response.changed() && !asking {
                            action.search_changed = true;
                        }
                        // Record a completed query when the box loses focus.
                        if response.lost_focus() && !search_text.is_empty() {
                            action.commit_search_history = true;
                        }
                        if search_focus_requested {
                            response.request_focus();
                        }

                        // Case-sensitive (`Aa`) and whole-word toggles.
                        if ui
                            .selectable_label(*search_case_sensitive, "Aa")
                            .on_hover_text(crate::i18n::t("search.case_sensitive"))
                            .clicked()
                        {
                            *search_case_sensitive = !*search_case_sensitive;
                            action.search_changed = true;
                        }
                        if ui
                            .selectable_label(*search_whole_word, "W")
                            .on_hover_text(crate::i18n::t("search.whole_word"))
                            .clicked()
                        {
                            *search_whole_word = !*search_whole_word;
                            action.search_changed = true;
                        }

                        // Scope selector: whole table or a single column. A visible chip so
                        // the user always knows what the search covers.
                        let scope_label = match *search_scope_col {
                            None => crate::i18n::t("search.scope_all"),
                            Some(c) => column_names
                                .get(c)
                                .cloned()
                                .unwrap_or_else(|| crate::i18n::t("search.scope_all")),
                        };
                        egui::ComboBox::from_id_salt("search_scope")
                            .width(110.0)
                            .selected_text(scope_label)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        search_scope_col.is_none(),
                                        crate::i18n::t("search.scope_all"),
                                    )
                                    .clicked()
                                {
                                    *search_scope_col = None;
                                    action.search_changed = true;
                                }
                                for (c, name) in column_names.iter().enumerate() {
                                    if ui
                                        .selectable_label(*search_scope_col == Some(c), name)
                                        .clicked()
                                    {
                                        *search_scope_col = Some(c);
                                        action.search_changed = true;
                                    }
                                }
                            })
                            .response
                            .on_hover_text(crate::i18n::t("search.scope_hint"));

                        // Ask: turn a plain-language sentence into filters via
                        // the chosen assistant. Which profile answers is always
                        // visible, never implicit.
                        let ask = ask;
                        let ask_resp = ui
                            .add_enabled_ui(ask.enabled, |ui| {
                                ui.selectable_label(*ask.mode, crate::i18n::t("search.ask"))
                            })
                            .inner;
                        let ask_resp = if ask.enabled {
                            let profile_name = ask
                                .profiles
                                .iter()
                                .find(|(id, _)| id == ask.profile_id)
                                .map(|(_, name)| name.clone())
                                .unwrap_or_default();
                            ask_resp.on_hover_text(
                                crate::i18n::t("search.ask_hint")
                                    .replace("{profile}", &profile_name),
                            )
                        } else {
                            // A disabled control must say why, not repeat its label.
                            ask_resp
                                .on_disabled_hover_text(crate::i18n::t("search.ask_needs_profile"))
                        };
                        if ask_resp.clicked() {
                            *ask.mode = !*ask.mode;
                            // Switching modes changes what the box means, so it
                            // starts empty either way, and any filter the old
                            // text was applying is dropped. Focus jumps to the
                            // box so there is somewhere obvious to type.
                            if !search_text.is_empty() {
                                search_text.clear();
                                action.search_changed = true;
                            }
                            if *ask.mode {
                                response.request_focus();
                            }
                        }
                        if *ask.mode && ask.enabled {
                            let selected = ask
                                .profiles
                                .iter()
                                .find(|(id, _)| id == ask.profile_id)
                                .map(|(_, name)| name.clone())
                                .unwrap_or_default();
                            egui::ComboBox::from_id_salt("search_ask_profile")
                                .width(130.0)
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (id, name) in ask.profiles {
                                        ui.selectable_value(ask.profile_id, id.clone(), name);
                                    }
                                })
                                .response
                                .on_hover_text(crate::i18n::t("search.ask_profile_hint"));
                        }

                        // Recent-queries dropdown. Picking one fills the search box.
                        if !search_history.is_empty() {
                            ui.menu_button(crate::i18n::t("search.history_btn"), |ui| {
                                ui.set_min_width(160.0);
                                ui.label(
                                    RichText::new(crate::i18n::t("search.history"))
                                        .size(10.0)
                                        .color(colors.text_secondary),
                                );
                                for entry in search_history {
                                    if ui.button(entry).clicked() {
                                        *search_text = entry.clone();
                                        action.search_changed = true;
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(crate::i18n::t("search.history_hint"));
                        }

                        // Bookmarks dropdown: jump to a named row/cell, delete one, or add a
                        // bookmark at the current selection. Session-only per tab.
                        ui.menu_button(crate::i18n::t("bookmarks.title"), |ui| {
                            ui.set_min_width(180.0);
                            if bookmarks.is_empty() {
                                ui.label(
                                    RichText::new(crate::i18n::t("bookmarks.empty"))
                                        .size(10.0)
                                        .color(colors.text_secondary),
                                );
                            } else {
                                for (i, (name, row, col)) in bookmarks.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        let pos = match col {
                                            Some(c) => format!("R{}:C{}", row + 1, c + 1),
                                            None => format!("R{}", row + 1),
                                        };
                                        if ui.button(format!("{name}  ({pos})")).clicked() {
                                            action.jump_bookmark = Some(i);
                                            ui.close();
                                        }
                                        if ui
                                            .small_button("x")
                                            .on_hover_text(crate::i18n::t("bookmarks.delete"))
                                            .clicked()
                                        {
                                            action.delete_bookmark = Some(i);
                                            ui.close();
                                        }
                                    });
                                }
                            }
                            ui.separator();
                            if ui
                                .button(crate::i18n::t("bookmarks.add"))
                                .on_hover_text(crate::i18n::t("bookmarks.add_hint"))
                                .clicked()
                            {
                                action.add_bookmark = true;
                                ui.close();
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("bookmarks.title"));

                        // With Ask on, Enter sends the sentence to the assistant
                        // rather than stepping through matches.
                        if *ask.mode && ask.enabled && !search_text.is_empty() {
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if enter && (response.lost_focus() || response.has_focus()) {
                                action.ask_submitted = true;
                                response.request_focus();
                            }
                        }
                        // Enter / Shift+Enter while the search box is focused step through
                        // matches (highlight mode only). Re-grab focus so repeated presses
                        // keep navigating instead of dropping focus after the first Enter.
                        if !*ask.mode && search_highlight_active && !search_text.is_empty() {
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if enter && (response.lost_focus() || response.has_focus()) {
                                if ui.input(|i| i.modifiers.shift) {
                                    action.find_prev = true;
                                } else {
                                    action.find_next = true;
                                }
                                response.request_focus();
                            }
                        }

                        // Filter / Highlight behaviour toggle. Switches the active search
                        // display mode for the session; the table honours it, text/tree
                        // views always highlight regardless.
                        if ui
                            .button(search_result_mode.label_t())
                            .on_hover_text(crate::i18n::t("search.mode_toggle_hint"))
                            .clicked()
                        {
                            *search_result_mode = match *search_result_mode {
                                crate::data::SearchResultMode::Filter => {
                                    crate::data::SearchResultMode::Highlight
                                }
                                crate::data::SearchResultMode::Highlight => {
                                    crate::data::SearchResultMode::Filter
                                }
                            };
                            action.search_result_mode_changed = true;
                        }

                        // Match count + next/previous controls, shown only when matches are
                        // highlighted in place (so the user can step through them).
                        if search_highlight_active && !search_text.is_empty() {
                            let count_label = if search_match_count == 0 {
                                crate::i18n::t("search.no_matches")
                            } else {
                                format!("{} / {}", search_match_current, search_match_count)
                            };
                            ui.label(RichText::new(count_label).color(colors.text_secondary));
                            let has_matches = search_match_count > 0;
                            if ui
                                .add_enabled(has_matches, egui::Button::new("<"))
                                .on_hover_text(crate::i18n::t("search.prev_match"))
                                .clicked()
                            {
                                action.find_prev = true;
                            }
                            if ui
                                .add_enabled(has_matches, egui::Button::new(">"))
                                .on_hover_text(crate::i18n::t("search.next_match"))
                                .clicked()
                            {
                                action.find_next = true;
                            }
                        }

                        if show_replace_bar {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);
                            ui.label(RichText::new("Replace:").color(colors.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(replace_text)
                                    .desired_width(160.0)
                                    .hint_text("Replace with..."),
                            );
                            let has_search = !search_text.is_empty();
                            if ui
                                .add_enabled(has_search, egui::Button::new("Next"))
                                .clicked()
                            {
                                action.replace_next = true;
                            }
                            if ui
                                .add_enabled(has_search, egui::Button::new("All"))
                                .clicked()
                            {
                                action.replace_all = true;
                            }
                        }
                    }
                });
            });

        // Window controls - pinned to the far right of the same toolbar.
        // Only rendered when the user opted into a custom title bar
        // (Settings -> File-Specific -> "Custom title bar"); `main.rs`
        // strips system decorations in that case so these buttons are
        // the only way to close / minimize / maximize the window.
        // `right_to_left` lays them out in visual order `[_] [□] [x]`
        // matching the desktop convention.
        if show_window_controls {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_size = egui::vec2(28.0, 24.0);
                let ctx = ui.ctx().clone();
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("x").size(15.0).strong())
                            .min_size(btn_size),
                    )
                    .on_hover_text("Close")
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("\u{25A1}").size(13.0))
                            .selected(is_max)
                            .min_size(btn_size),
                    )
                    .on_hover_text(if is_max { "Restore" } else { "Maximise" })
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("_").size(15.0).strong())
                            .min_size(btn_size),
                    )
                    .on_hover_text("Minimise")
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        }
    });

    action
}
