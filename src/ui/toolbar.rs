use std::collections::HashSet;

use egui::{RichText, Ui, WidgetText};

use super::shortcuts::Shortcuts;
use super::theme::{ThemeColors, ThemeMode};
use crate::data::{DataTable, MarkColor, MarkKey, SearchMode, ViewMode};

/// Top-level menu button that auto-switches on hover, restoring the
/// MS-Office-style behaviour that egui 0.31's `MenuRoot::stationary_interaction`
/// provided and that egui 0.34's `MenuButton` no longer does.
///
/// Click toggles open / closed. Hovering this button while a *different* top
/// popup is already open force-opens this one (the singleton popup state
/// replaces the previously open menu in one shot). When this menu's popup is
/// already the open one, hovering is a no-op so the popup doesn't churn.
///
/// We mirror `Popup::menu`'s setup (kind, layout, style, gap, and the
/// `MenuConfig` stack tag with `bar=false`) so submenu buttons rendered inside
/// `content` see `is_in_menu(ui) == true` and dispatch to
/// `SubMenuButton`, which carries its own hover-open logic for nested menus.
fn top_menu_button(
    ui: &mut Ui,
    label: impl Into<WidgetText>,
    content: impl FnOnce(&mut Ui),
) -> egui::Response {
    let resp = ui.add(egui::Button::new(label));
    let ctx = ui.ctx().clone();
    let popup_id = resp.id;
    let was_open = egui::Popup::is_id_open(&ctx, popup_id);
    let any_open = egui::Popup::is_any_open(&ctx);

    let set_open = if resp.clicked() {
        Some(egui::SetOpenCommand::Toggle)
    } else if !was_open && resp.hovered() && any_open {
        Some(egui::SetOpenCommand::Bool(true))
    } else {
        None
    };

    // `MenuConfig::default()` already gives `bar: false`, which is what makes
    // `is_in_menu(ui)` inside `content` return true and dispatch submenu
    // buttons to `SubMenuButton`.
    let config = egui::containers::menu::MenuConfig::default();
    egui::Popup::from_response(&resp)
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .gap(0.0)
        .open_memory(set_open)
        .info(
            egui::UiStackInfo::new(egui::UiKind::Menu)
                .with_tag_value(egui::containers::menu::MenuConfig::MENU_CONFIG_TAG, config),
        )
        .show(|ui| {
            // Never wrap menu-item labels: in non-English locales longer strings
            // (e.g. German/Russian) would otherwise break onto a second line and
            // look cramped. `Extend` lets the popup grow to the widest item
            // instead, applied once here so every top menu inherits it.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            content(ui);
        });

    resp
}

/// Which slice of the active table to feed into the "Parse in new tab"
/// modal. Set by the Edit menu submenu or the table's right-click context
/// menu; the app shell turns it into a [`PendingParseModal`] for the
/// dialog renderer to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseScope {
    /// Single cell at `(row, col)` (display-row coordinates).
    Cell { row: usize, col: usize },
    /// Whole row at display-row index `row`.
    Row { row: usize },
    /// Whole column at index `col`.
    Column { col: usize },
    /// The entire active table.
    Table,
}

#[derive(Default)]
pub struct ToolbarAction {
    pub new_file: bool,
    pub open_file: bool,
    pub open_directory: bool,
    pub close_directory: bool,
    pub open_recent: Option<String>,
    /// Right-click -> "Remove from list" on a single recent-files entry.
    pub remove_recent: Option<String>,
    /// Right-click -> "Clear all" on a recent-files entry.
    pub clear_recent: bool,
    pub save_file: bool,
    pub save_file_as: bool,
    pub toggle_theme: bool,
    pub search_changed: bool,
    /// The search box lost focus with a non-empty query: record it in the
    /// persistent search history.
    pub commit_search_history: bool,
    /// The Filter/Highlight search-behaviour toggle was flipped this frame.
    pub search_result_mode_changed: bool,
    /// Jump to the next highlight-search match (`>` button or Enter).
    pub find_next: bool,
    /// Jump to the previous highlight-search match (`<` button or Shift+Enter).
    pub find_prev: bool,
    pub add_row: bool,
    pub delete_row: bool,
    pub add_column: bool,
    pub time_calc: bool,
    pub delete_column: bool,
    pub move_row_up: bool,
    pub move_row_down: bool,
    pub move_col_left: bool,
    pub move_col_right: bool,
    pub sort_rows_asc_by: Option<usize>,
    pub sort_rows_desc_by: Option<usize>,
    /// Reorder all columns alphabetically by name (case-insensitive).
    pub sort_columns_asc: bool,
    /// Reorder all columns reverse-alphabetically by name (case-insensitive).
    pub sort_columns_desc: bool,
    /// Clear the active tab's `hidden_columns` so every column becomes
    /// visible again. Wired to Edit -> Show hidden columns.
    pub show_all_columns: bool,
    /// Open the Excel-style Column Filter dialog. Outer `Some` = the user
    /// invoked the action this frame (menu click, header context menu,
    /// status-bar chip, ...); inner `Some(col)` = preselect that column, inner
    /// `None` = no preselect (dialog opens on the first column or the
    /// previously remembered one).
    pub show_column_filter: Option<Option<usize>>,
    pub discard_edits: bool,
    pub view_mode_changed: Option<ViewMode>,
    pub show_settings: bool,
    pub show_about: bool,
    pub check_for_updates: bool,
    pub replace_next: bool,
    pub replace_all: bool,
    pub toggle_replace_bar: bool,
    pub search_focus: bool,
    pub show_documentation: bool,
    pub exit: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    pub zoom_reset: bool,
    pub toggle_sql_panel: bool,
    /// Open a Chart tab for the active table. Fired by **Analyse ->
    /// Chart** (toolbar) or the `OpenChart` shortcut. Independent from
    /// `toggle_sql_panel` so the user can have either / both / neither.
    pub open_chart_tab: bool,
    /// Open the Value Frequency column picker (no column context). Fired by
    /// **Analyse -> Value frequency...**.
    pub open_value_frequency: bool,
    /// Open a Summary tab (per-column statistics via DuckDB SUMMARIZE) for
    /// the active table. Fired by **Analyse -> Summary...**.
    pub open_describe_tab: bool,
    /// Open the Pivot / Unpivot dialog for the active table.
    /// Fired by **Analyse -> Pivot / Unpivot...**.
    pub open_pivot: bool,
    /// Copy the current selection to the clipboard as a Markdown table.
    /// Fired by **Edit -> Copy as Markdown table**.
    pub copy_as_markdown: bool,
    /// Open the per-column Number-format dialog for the selected column.
    /// Fired by **Edit -> Number format...**.
    pub open_column_format: bool,
    /// Open the Conditional formatting dialog for the active table.
    /// Fired by **Edit -> Conditional formatting...**.
    pub open_conditional_format: bool,
    /// Toggle "first row is header" for the active table.
    pub toggle_first_row_header: bool,
    /// Apply a color mark to a set of keys (cell/row/column).
    pub set_marks: Vec<(MarkKey, MarkColor)>,
    /// Clear color marks from a set of keys.
    pub clear_marks: Vec<MarkKey>,
    /// Clear every color mark on the active table. Wired to the new
    /// "Clear all marks" entry in **Edit -> Mark**; reachable even
    /// without a selection so users can wipe duplicate-row highlights
    /// without first selecting the rows.
    pub clear_all_marks: bool,
    /// Undo the last change.
    pub undo: bool,
    /// Redo the last undone change.
    pub redo: bool,
    /// Logo in the top-left was clicked. Wired to a hidden easter-egg counter
    /// in the app shell - most users never trigger it.
    pub logo_clicked: bool,
    /// Toggle session-only read-only mode (also bound to F8 by default).
    pub toggle_readonly: bool,
    /// Open the "Parse in new tab" modal pre-seeded with this scope.
    /// `None` means the menu wasn't clicked this frame.
    pub parse_in_new_tab: Option<ParseScope>,
    /// Restore the most-recently-closed tab. Wired to the Edit menu entry
    /// (the Ctrl+Shift+T shortcut is handled separately in
    /// `shortcuts_dispatch`).
    pub reopen_last_closed_tab: bool,
    /// Resize every column in the active table to its best-fit width.
    /// Wired to the Edit menu entry (the Ctrl+Shift+W shortcut is handled
    /// separately in `shortcuts_dispatch`).
    pub fit_all_columns: bool,
    /// User clicked View -> Compare with...  The app shell opens a file
    /// picker, loads the picked file as the right side, and flips the
    /// active tab into `ViewMode::Compare`.
    pub compare_with: bool,
    /// Open the **Edit -> Find duplicates...** modal for the active tab.
    /// The dialog itself lives in `app::dialogs::find_duplicates`; the
    /// toolbar just signals "user wants it open".
    pub show_find_duplicates: bool,
    /// Open the Schema Export dialog. The dialog itself lets the user
    /// switch between the seven supported targets; there's no need for
    /// the toolbar to pre-pick one. Fired by **File -> Export schema...**
    /// and the `ExportSchema` keyboard shortcut.
    pub show_schema_export: bool,
    /// Toggle the cross-tab + directory multi-search panel. Fired by
    /// **Search -> Multi-search...** and the `MultiSearch` keyboard
    /// shortcut.
    pub toggle_multi_search: bool,
    /// Toggle the in-GUI chat assistant panel. Fired by the toolbar Assistant
    /// button, **View -> Assistant panel**, and the `ToggleChatPanel` shortcut.
    pub toggle_chat_panel: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_toolbar(
    ui: &mut Ui,
    theme_mode: ThemeMode,
    search_text: &mut String,
    search_mode: &mut SearchMode,
    // Search-bar toggles: case-sensitive (`Aa`), whole-word, and the column
    // scope (`None` = whole table, `Some(col)` = one column). Edited in place.
    search_case_sensitive: &mut bool,
    search_whole_word: &mut bool,
    search_scope_col: &mut Option<usize>,
    // Column names for the scope dropdown (in table order).
    column_names: &[String],
    // Recent search queries (most-recent first) for the history dropdown.
    search_history: &[String],
    // Session search behaviour (Filter vs Highlight). Edited in place by the
    // search-bar toggle; the table respects it, text/tree views always
    // highlight.
    search_result_mode: &mut crate::data::SearchResultMode,
    // Whether matches are highlighted (rather than filtered) for the active
    // tab's view: true when `search_result_mode == Highlight` OR the view is a
    // text/tree view. Drives whether the count + next/prev controls show.
    search_highlight_active: bool,
    // Highlight-search match count and current 1-based position for the active
    // tab (computed by the view on the previous frame).
    search_match_count: usize,
    search_match_current: usize,
    search_focus_requested: bool,
    show_replace_bar: bool,
    replace_text: &mut String,
    has_data: bool,
    has_edits: bool,
    has_source_path: bool,
    selected_cell: Option<(usize, usize)>,
    selected_rows: &HashSet<usize>,
    selected_cols: &HashSet<usize>,
    selected_cells: &HashSet<(usize, usize)>,
    row_count: usize,
    col_count: usize,
    current_view_mode: ViewMode,
    has_raw_content: bool,
    has_markdown: bool,
    has_notebook: bool,
    has_epub: bool,
    has_map: bool,
    has_json: bool,
    has_yaml: bool,
    readonly_mode: bool,
    // Kept on the signature so callers don't have to know whether the
    // Analyse dropdown still reflects panel state - currently it does not
    // (just two flat buttons), but flipping that back is a one-line edit.
    _sql_panel_open: bool,
    zoom_percent: u32,
    logo_texture: Option<&egui::TextureHandle>,
    recent_files: &[String],
    directory_tree_open: bool,
    first_row_is_header: bool,
    has_hidden_columns: bool,
    can_undo: bool,
    can_redo: bool,
    can_reopen_tab: bool,
    _shortcuts: &Shortcuts,
    table: &DataTable,
    // When true, render close / max / min buttons at the right edge of
    // this toolbar. Paired with `AppSettings.use_custom_title_bar` (which
    // also strips system decorations in `main.rs`).
    show_window_controls: bool,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    let colors = ThemeColors::for_mode(theme_mode);
    let has_selected_cell = selected_cell.is_some();

    // Top-level menus go through `top_menu_button` (defined above), which
    // brings back the hover-switch behaviour egui 0.31's MenuRoot used to
    // provide and that egui 0.34's MenuButton dropped. Plain `ui.horizontal`
    // is enough here - we do *not* wrap in `egui::MenuBar`, because the
    // helper handles the menu/submenu plumbing itself.
    ui.horizontal(|ui| {
        ui.add_space(4.0);

        // App logo + title. The logo is wrapped as a clickable widget so the
        // hidden easter-egg counter (seven clicks within ~1.5 s) can trigger.
        if let Some(tex) = logo_texture {
            let img = egui::Image::new(egui::load::SizedTexture::new(tex.id(), [20.0, 20.0]))
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

        // --- File menu ---
        top_menu_button(
            ui,
            RichText::new(crate::i18n::t("menu.file")).color(colors.text_primary),
            |ui| {
                ui.set_min_width(180.0);
                if ui.button(crate::i18n::t("file_menu.new_file")).clicked() {
                    action.new_file = true;
                    ui.close();
                }
                if ui.button(crate::i18n::t("common.open")).clicked() {
                    action.open_file = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("file_menu.open_directory"))
                    .clicked()
                {
                    action.open_directory = true;
                    ui.close();
                }
                if directory_tree_open
                    && ui
                        .button(crate::i18n::t("file_menu.close_directory"))
                        .clicked()
                {
                    action.close_directory = true;
                    ui.close();
                }
                if has_data {
                    ui.separator();
                    if has_source_path && ui.button(crate::i18n::t("common.save")).clicked() {
                        action.save_file = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("common.save_as")).clicked() {
                        action.save_file_as = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("file_menu.export_schema"))
                        .clicked()
                    {
                        action.show_schema_export = true;
                        ui.close();
                    }
                }
                ui.separator();
                ui.menu_button(crate::i18n::t("menu.recent_files"), |ui| {
                    ui.set_min_width(250.0);
                    if recent_files.is_empty() {
                        ui.add_enabled(
                            false,
                            egui::Button::new(crate::i18n::t("file_menu.recent_none")),
                        );
                    } else {
                        for path in recent_files {
                            let filename = std::path::Path::new(path)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.clone());
                            let resp = ui.button(&filename).on_hover_text(path);
                            if resp.clicked() {
                                action.open_recent = Some(path.clone());
                                ui.close();
                            }
                            resp.context_menu(|ui| {
                                if ui
                                    .button(crate::i18n::t("file_menu.remove_from_list"))
                                    .clicked()
                                {
                                    action.remove_recent = Some(path.clone());
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button(crate::i18n::t("file_menu.clear_all")).clicked() {
                                    action.clear_recent = true;
                                    ui.close();
                                }
                            });
                        }
                    }
                });
                ui.separator();
                if ui.button(crate::i18n::t("file_menu.exit")).clicked() {
                    action.exit = true;
                    ui.close();
                }
            },
        );

        // --- Edit menu ---
        if has_data {
            top_menu_button(
                ui,
                RichText::new(crate::i18n::t("menu.edit")).color(colors.text_primary),
                |ui| {
                    // Edit menu entries deliberately omit shortcut suffixes -
                    // bindings are discoverable via Settings -> Shortcuts; cramming
                    // them into the menu was visually noisy.
                    if ui
                        .add_enabled(
                            can_undo,
                            egui::Button::new(crate::i18n::t("edit_menu.undo")),
                        )
                        .clicked()
                    {
                        action.undo = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            can_redo,
                            egui::Button::new(crate::i18n::t("edit_menu.redo")),
                        )
                        .clicked()
                    {
                        action.redo = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            can_reopen_tab,
                            egui::Button::new(crate::i18n::t("edit_menu.reopen_tab")),
                        )
                        .clicked()
                    {
                        action.reopen_last_closed_tab = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("edit_menu.fit_all_columns"))
                        .clicked()
                    {
                        action.fit_all_columns = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("edit_menu.copy_markdown"))
                        .on_hover_text(crate::i18n::t("edit_menu.copy_markdown_hint"))
                        .clicked()
                    {
                        action.copy_as_markdown = true;
                        ui.close();
                    }
                    ui.separator();

                    // Row operations
                    ui.label(
                        RichText::new(crate::i18n::t("edit_menu.section_rows"))
                            .strong()
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                    if ui.button(crate::i18n::t("edit_menu.insert_row")).clicked() {
                        action.add_row = true;
                        ui.close();
                    }
                    let del_row = ui.add_enabled(
                        has_selected_cell,
                        egui::Button::new(crate::i18n::t("edit_menu.delete_row")),
                    );
                    if del_row.clicked() {
                        action.delete_row = true;
                        ui.close();
                    }

                    let can_move_up = selected_cell.is_some_and(|(r, _)| r > 0);
                    let can_move_down = selected_cell.is_some_and(|(r, _)| r + 1 < row_count);

                    let up_btn = ui.add_enabled(
                        can_move_up,
                        egui::Button::new(crate::i18n::t("edit_menu.move_row_up")),
                    );
                    if up_btn.clicked() {
                        action.move_row_up = true;
                        ui.close();
                    }
                    let down_btn = ui.add_enabled(
                        can_move_down,
                        egui::Button::new(crate::i18n::t("edit_menu.move_row_down")),
                    );
                    if down_btn.clicked() {
                        action.move_row_down = true;
                        ui.close();
                    }

                    ui.separator();

                    // Column operations
                    ui.label(
                        RichText::new(crate::i18n::t("edit_menu.section_columns"))
                            .strong()
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                    if ui.button(crate::i18n::t("toolbar.insert_column")).clicked() {
                        action.add_column = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("toolbar.time_calc")).clicked() {
                        action.time_calc = true;
                        ui.close();
                    }
                    let del_col = ui.add_enabled(
                        has_selected_cell,
                        egui::Button::new(crate::i18n::t("toolbar.delete_column")),
                    );
                    if del_col.clicked() {
                        action.delete_column = true;
                        ui.close();
                    }

                    let can_move_left = selected_cell.is_some_and(|(_, c)| c > 0);
                    let can_move_right = selected_cell.is_some_and(|(_, c)| c + 1 < col_count);

                    let left_btn = ui.add_enabled(
                        can_move_left,
                        egui::Button::new(crate::i18n::t("edit_menu.move_col_left")),
                    );
                    if left_btn.clicked() {
                        action.move_col_left = true;
                        ui.close();
                    }
                    let right_btn = ui.add_enabled(
                        can_move_right,
                        egui::Button::new(crate::i18n::t("edit_menu.move_col_right")),
                    );
                    if right_btn.clicked() {
                        action.move_col_right = true;
                        ui.close();
                    }

                    let can_sort_cols = col_count > 1;
                    let sort_cols_asc = ui.add_enabled(
                        can_sort_cols,
                        egui::Button::new(crate::i18n::t("edit_menu.sort_cols_asc")),
                    );
                    if sort_cols_asc.clicked() {
                        action.sort_columns_asc = true;
                        ui.close();
                    }
                    let sort_cols_desc = ui.add_enabled(
                        can_sort_cols,
                        egui::Button::new(crate::i18n::t("edit_menu.sort_cols_desc")),
                    );
                    if sort_cols_desc.clicked() {
                        action.sort_columns_desc = true;
                        ui.close();
                    }

                    let num_fmt_btn = ui.add_enabled(
                        has_selected_cell,
                        egui::Button::new(crate::i18n::t("edit_menu.number_format")),
                    );
                    if num_fmt_btn
                        .on_hover_text(crate::i18n::t("edit_menu.number_format_hint"))
                        .clicked()
                    {
                        action.open_column_format = true;
                        ui.close();
                    }

                    if ui
                        .button(crate::i18n::t("edit_menu.conditional_format"))
                        .on_hover_text(crate::i18n::t("edit_menu.conditional_format_hint"))
                        .clicked()
                    {
                        action.open_conditional_format = true;
                        ui.close();
                    }

                    let show_all_btn = ui.add_enabled(
                        has_hidden_columns,
                        egui::Button::new(crate::i18n::t("edit_menu.show_hidden_columns")),
                    );
                    let show_all_btn = if !has_hidden_columns {
                        show_all_btn.on_disabled_hover_text(crate::i18n::t(
                            "edit_menu.show_hidden_columns_hint",
                        ))
                    } else {
                        show_all_btn
                    };
                    if show_all_btn.clicked() {
                        action.show_all_columns = true;
                        ui.close();
                    }

                    ui.separator();

                    // "Parse in new tab" submenu - opens a modal that
                    // parses the chosen scope (cell / row / column / whole
                    // table) as a user-picked format and opens the result
                    // in a new tab. Cell / Row / Column require a selected
                    // cell so we know which row+col to target; Whole table
                    // is always available.
                    ui.menu_button(crate::i18n::t("edit_menu.parse_in_new_tab"), |ui| {
                        let cell_btn = ui.add_enabled(
                            has_selected_cell,
                            egui::Button::new(crate::i18n::t("edit_menu.scope_cell")),
                        );
                        if cell_btn.clicked()
                            && let Some((row, col)) = selected_cell
                        {
                            action.parse_in_new_tab = Some(ParseScope::Cell { row, col });
                            ui.close();
                        }
                        let row_btn = ui.add_enabled(
                            has_selected_cell,
                            egui::Button::new(crate::i18n::t("edit_menu.scope_row")),
                        );
                        if row_btn.clicked()
                            && let Some((row, _)) = selected_cell
                        {
                            action.parse_in_new_tab = Some(ParseScope::Row { row });
                            ui.close();
                        }
                        let col_btn = ui.add_enabled(
                            has_selected_cell,
                            egui::Button::new(crate::i18n::t("edit_menu.scope_column")),
                        );
                        if col_btn.clicked()
                            && let Some((_, col)) = selected_cell
                        {
                            action.parse_in_new_tab = Some(ParseScope::Column { col });
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("edit_menu.scope_table")).clicked() {
                            action.parse_in_new_tab = Some(ParseScope::Table);
                            ui.close();
                        }
                    });

                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::t("edit_menu.section_sort_rows"))
                            .strong()
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                    let can_sort = selected_cell.is_some();
                    let sort_asc = ui.add_enabled(
                        can_sort,
                        egui::Button::new(crate::i18n::t("edit_menu.sort_asc")),
                    );
                    if sort_asc.clicked() {
                        if let Some((_, col)) = selected_cell {
                            action.sort_rows_asc_by = Some(col);
                        }
                        ui.close();
                    }
                    let sort_desc = ui.add_enabled(
                        can_sort,
                        egui::Button::new(crate::i18n::t("edit_menu.sort_desc")),
                    );
                    if sort_desc.clicked() {
                        if let Some((_, col)) = selected_cell {
                            action.sort_rows_desc_by = Some(col);
                        }
                        ui.close();
                    }

                    ui.separator();

                    // Mark submenu - surfaces the same colors as the right-click
                    // context menu, scoped to the current selection.
                    let mark_keys: Vec<MarkKey> = if !selected_rows.is_empty() {
                        let mut rs: Vec<usize> = selected_rows.iter().copied().collect();
                        rs.sort();
                        rs.into_iter().map(MarkKey::Row).collect()
                    } else if !selected_cols.is_empty() {
                        let mut cs: Vec<usize> = selected_cols.iter().copied().collect();
                        cs.sort();
                        cs.into_iter().map(MarkKey::Column).collect()
                    } else if !selected_cells.is_empty() {
                        let mut cs: Vec<(usize, usize)> = selected_cells.iter().copied().collect();
                        cs.sort();
                        cs.into_iter().map(|(r, c)| MarkKey::Cell(r, c)).collect()
                    } else if let Some((r, c)) = selected_cell {
                        vec![MarkKey::Cell(r, c)]
                    } else {
                        Vec::new()
                    };
                    let has_marks_keys = !mark_keys.is_empty();
                    let any_currently_marked =
                        mark_keys.iter().any(|k| table.marks.contains_key(k));
                    let table_has_any_marks = !table.marks.is_empty();
                    // The submenu opens whenever a clear path is available -
                    // either the selection has marks to color/clear, or the
                    // table has marks somewhere (so "Clear all marks" applies).
                    let menu_enabled = has_marks_keys || table_has_any_marks;
                    ui.add_enabled_ui(menu_enabled, |ui| {
                        ui.menu_button(crate::i18n::t("edit_menu.mark"), |ui| {
                            // Color buttons + scoped Clear act on the current
                            // selection; greyed when there is none so the user
                            // can still reach the always-available "Clear all
                            // marks" entry below.
                            ui.add_enabled_ui(has_marks_keys, |ui| {
                                for &color in MarkColor::ALL {
                                    let swatch = ThemeColors::mark_swatch(color);
                                    let label = color.label_t();
                                    let btn = egui::Button::new(RichText::new(label).color(swatch));
                                    if ui.add(btn).clicked() {
                                        for k in &mark_keys {
                                            action.set_marks.push((k.clone(), color));
                                        }
                                        ui.close();
                                    }
                                }
                                if any_currently_marked {
                                    ui.separator();
                                    if ui.button(crate::i18n::t("edit_menu.clear")).clicked() {
                                        for k in &mark_keys {
                                            action.clear_marks.push(k.clone());
                                        }
                                        ui.close();
                                    }
                                }
                            });
                            if table_has_any_marks {
                                ui.separator();
                                if ui
                                    .button(crate::i18n::t("edit_menu.clear_all_marks"))
                                    .clicked()
                                {
                                    action.clear_all_marks = true;
                                    ui.close();
                                }
                            }
                        });
                    });

                    ui.separator();
                    let mut header_flag = first_row_is_header;
                    if ui
                        .checkbox(
                            &mut header_flag,
                            crate::i18n::t("edit_menu.first_row_is_header"),
                        )
                        .changed()
                    {
                        action.toggle_first_row_header = true;
                        ui.close();
                    }

                    if has_edits {
                        ui.separator();
                        if ui
                            .button(crate::i18n::t("edit_menu.discard_all_edits"))
                            .clicked()
                        {
                            action.discard_edits = true;
                            ui.close();
                        }
                    }
                },
            );

            // --- View menu ---
            top_menu_button(
                ui,
                RichText::new(crate::i18n::t("menu.view")).color(colors.text_primary),
                |ui| {
                    let is_table = current_view_mode == ViewMode::Table;
                    let is_raw = current_view_mode == ViewMode::Raw;

                    // Disable table view for notebook files (notebook view is the primary view)
                    let table_enabled = !has_notebook;
                    let table_btn = ui.add_enabled(
                        table_enabled,
                        egui::RadioButton::new(is_table, crate::i18n::t("view_menu.table")),
                    );
                    if table_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::Table);
                        ui.close();
                    }
                    let raw_btn = ui.add_enabled(
                        has_raw_content,
                        egui::RadioButton::new(is_raw, crate::i18n::t("view_menu.raw")),
                    );
                    if raw_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::Raw);
                        ui.close();
                    }
                    if has_markdown {
                        let is_md = current_view_mode == ViewMode::Markdown;
                        let md_btn = ui.radio(is_md, crate::i18n::t("view_menu.markdown"));
                        if md_btn.clicked() {
                            action.view_mode_changed = Some(ViewMode::Markdown);
                            ui.close();
                        }
                    }
                    if has_notebook {
                        let is_nb = current_view_mode == ViewMode::Notebook;
                        let nb_btn = ui.radio(is_nb, crate::i18n::t("view_menu.notebook"));
                        if nb_btn.clicked() {
                            action.view_mode_changed = Some(ViewMode::Notebook);
                            ui.close();
                        }
                    }
                    if has_epub {
                        let is_epub = current_view_mode == ViewMode::EpubReader;
                        let epub_btn = ui.radio(is_epub, crate::i18n::t("view_menu.epub"));
                        if epub_btn.clicked() {
                            action.view_mode_changed = Some(ViewMode::EpubReader);
                            ui.close();
                        }
                    }
                    if has_map {
                        let is_map = current_view_mode == ViewMode::Map;
                        let map_btn = ui.radio(is_map, crate::i18n::t("view_menu.map"));
                        if map_btn.clicked() {
                            action.view_mode_changed = Some(ViewMode::Map);
                            ui.close();
                        }
                    }
                    if has_json {
                        let is_json_tree = current_view_mode == ViewMode::JsonTree;
                        let json_btn =
                            ui.radio(is_json_tree, crate::i18n::t("view_menu.json_tree"));
                        if json_btn.clicked() {
                            action.view_mode_changed = Some(ViewMode::JsonTree);
                            ui.close();
                        }
                    }
                    if has_yaml {
                        let is_yaml_tree = current_view_mode == ViewMode::YamlTree;
                        let yaml_btn =
                            ui.radio(is_yaml_tree, crate::i18n::t("view_menu.yaml_tree"));
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
                        .clicked()
                    {
                        action.compare_with = true;
                        ui.close();
                    }

                    ui.separator();
                    if ui
                        .checkbox(
                            &mut readonly_mode.clone(),
                            crate::i18n::t("view_menu.readonly"),
                        )
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
                        && ui.button(crate::i18n::t("view_menu.zoom_reset")).clicked()
                    {
                        action.zoom_reset = true;
                        ui.close();
                    }
                },
            );

            // --- Search menu ---
            top_menu_button(
                ui,
                RichText::new(crate::i18n::t("menu.search")).color(colors.text_primary),
                |ui| {
                    ui.set_min_width(180.0);
                    if ui.button(crate::i18n::t("search_menu.find")).clicked() {
                        action.search_focus = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("search_menu.find_replace"))
                        .clicked()
                    {
                        action.toggle_replace_bar = true;
                        ui.close();
                    }
                    ui.separator();
                    // Excel-style per-column value filter. Deliberately *not*
                    // suffixed with the shortcut combo (Ctrl+Shift+F by default)
                    // - same convention as the F8 read-only menu entry.
                    let filter_btn = ui.add_enabled(
                        has_data,
                        egui::Button::new(crate::i18n::t("search_menu.column_filter")),
                    );
                    if filter_btn.clicked() {
                        action.show_column_filter = Some(None);
                        ui.close();
                    }
                    let dup_btn = ui.add_enabled(
                        has_data,
                        egui::Button::new(crate::i18n::t("search_menu.find_duplicates")),
                    );
                    if dup_btn.clicked() {
                        action.show_find_duplicates = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(crate::i18n::t("search_menu.multi_search"))
                        .clicked()
                    {
                        action.toggle_multi_search = true;
                        ui.close();
                    }
                },
            );

            // --- Analyse group ---
            //
            // Always-visible dropdown labelled "Analyse". SQL / Chart / Value
            // frequency act on a table, so they're shown only on Table-view
            // tabs; the **Assistant** entry is always present (it works on any
            // open tab, including non-tabular ones) so the panel stays
            // discoverable everywhere, not just via the shortcut.
            top_menu_button(
                ui,
                RichText::new(crate::i18n::t("menu.analyse")).color(colors.text_primary),
                |ui| {
                    ui.set_min_width(120.0);
                    let table_actions = current_view_mode == ViewMode::Table;
                    if table_actions {
                        if ui.button(crate::i18n::t("analyse_menu.sql")).clicked() {
                            action.toggle_sql_panel = true;
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("analyse_menu.chart")).clicked() {
                            action.open_chart_tab = true;
                            ui.close();
                        }
                        if ui
                            .button(crate::i18n::t("analyse_menu.value_frequency"))
                            .clicked()
                        {
                            action.open_value_frequency = true;
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("analyse_menu.describe")).clicked() {
                            action.open_describe_tab = true;
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("analyse_menu.pivot")).clicked() {
                            action.open_pivot = true;
                            ui.close();
                        }
                        ui.separator();
                    }
                    if ui
                        .button(crate::i18n::t("analyse_menu.assistant"))
                        .clicked()
                    {
                        action.toggle_chat_panel = true;
                        ui.close();
                    }
                },
            );
        }

        // --- Help menu (always visible, next to Search) ---
        top_menu_button(
            ui,
            RichText::new(crate::i18n::t("menu.help")).color(colors.text_primary),
            |ui| {
                ui.set_min_width(180.0);
                if ui
                    .button(crate::i18n::t("help_menu.documentation"))
                    .clicked()
                {
                    action.show_documentation = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::i18n::t("help_menu.settings")).clicked() {
                    action.show_settings = true;
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(crate::i18n::t("help_menu.check_updates"))
                    .clicked()
                {
                    action.check_for_updates = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::i18n::t("help_menu.about")).clicked() {
                    action.show_about = true;
                    ui.close();
                }
            },
        );

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
            let hint = match *search_mode {
                SearchMode::Plain => "Filter rows...",
                SearchMode::Wildcard => "e.g. foo*bar, item?",
                SearchMode::Regex => "e.g. ^\\d{3}-",
            };
            let search_id = ui.id().with("toolbar_search");
            let response = ui.add(
                egui::TextEdit::singleline(search_text)
                    .id(search_id)
                    .desired_width(200.0)
                    .hint_text(hint),
            );
            if response.changed() {
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

            // Enter / Shift+Enter while the search box is focused step through
            // matches (highlight mode only). Re-grab focus so repeated presses
            // keep navigating instead of dropping focus after the first Enter.
            if search_highlight_active && !search_text.is_empty() {
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
