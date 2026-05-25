//! Translate keyboard input into state mutations on [`OctaApp`]. All
//! user-configurable bindings live in `self.settings.shortcuts`; the fixed
//! bindings (Ctrl+1..9 tab jump, Escape closing the replace bar) are
//! hard-coded here because they aren't customizable.

use eframe::egui;

use octa::data::ViewMode;
use octa::ui::shortcuts::ShortcutAction as SA;

use super::state::{OctaApp, TabState};
use crate::view_modes;

impl OctaApp {
    pub(crate) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let shortcuts = self.settings.shortcuts.clone();
        let action_fired = |a: SA| ctx.input(|i| shortcuts.triggered(a, i));

        if action_fired(SA::NewFile) {
            let mut new_tab = TabState::new(self.settings.default_search_mode);
            new_tab.view_mode = ViewMode::Raw;
            new_tab.raw_content = Some(String::new());
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
        if action_fired(SA::OpenFile) {
            self.open_file();
        }
        if action_fired(SA::SaveFile) {
            if self.tabs[self.active_tab].table.source_path.is_some() {
                self.save_file();
            } else if self.tabs[self.active_tab].table.col_count() > 0
                || self.tabs[self.active_tab].raw_content_modified
            {
                self.save_file_as();
            }
        }
        if action_fired(SA::FocusSearch) {
            self.search_focus_requested = true;
        }
        if action_fired(SA::ToggleFindReplace) {
            self.tabs[self.active_tab].show_replace_bar =
                !self.tabs[self.active_tab].show_replace_bar;
            self.search_focus_requested = true;
        }
        if self.tabs[self.active_tab].show_replace_bar
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.tabs[self.active_tab].show_replace_bar = false;
        }
        if action_fired(SA::QuitApp) {
            if self.tabs[self.active_tab].is_modified() && !self.confirmed_close {
                self.show_close_confirm = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if action_fired(SA::CloseTab) {
            if self.tabs[self.active_tab].is_modified() {
                self.pending_close_tab = Some(self.active_tab);
                self.show_close_confirm = true;
            } else {
                self.close_tab(self.active_tab);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
        if action_fired(SA::NextTab) {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        if action_fired(SA::PrevTab) {
            if self.active_tab == 0 {
                self.active_tab = self.tabs.len() - 1;
            } else {
                self.active_tab -= 1;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        // Ctrl+1..9: jump to tab by number (not user-configurable)
        let ctrl_held = ctx.input(|i| i.modifiers.command);
        for n in 1..=9u8 {
            let key = match n {
                1 => egui::Key::Num1,
                2 => egui::Key::Num2,
                3 => egui::Key::Num3,
                4 => egui::Key::Num4,
                5 => egui::Key::Num5,
                6 => egui::Key::Num6,
                7 => egui::Key::Num7,
                8 => egui::Key::Num8,
                9 => egui::Key::Num9,
                _ => unreachable!(),
            };
            if ctrl_held && ctx.input(|i| i.key_pressed(key)) {
                let idx = (n as usize) - 1;
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                        self.tabs[self.active_tab].title_display(),
                    ));
                }
            }
        }
        // Only select all table rows when no TextEdit has focus — otherwise
        // Ctrl+A should scope to the text editor (SQL, raw, search bars, etc.)
        // and leave the table alone.
        let text_edit_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some();
        if action_fired(SA::SelectAllRows)
            && !text_edit_focused
            && !self.tabs[self.active_tab].show_column_inspector
            && self.tabs[self.active_tab].table.col_count() > 0
            && self.tabs[self.active_tab].table.row_count() > 0
        {
            self.tabs[self.active_tab].table_state.selected_rows.clear();
            self.tabs[self.active_tab].table_state.selected_cols.clear();
            for r in 0..self.tabs[self.active_tab].table.row_count() {
                self.tabs[self.active_tab]
                    .table_state
                    .selected_rows
                    .insert(r);
            }
        }
        // Copy / Cut / Paste — handled inside `render_central_panel` so SQL
        // editor, raw text editor, search bar, and any other earlier-rendered
        // TextEdits consume the clipboard events first. This avoids the
        // "Ctrl+V into the SQL editor also pastes into the table" bug.

        if action_fired(SA::ExportSqlResult)
            && self.tabs[self.active_tab]
                .sql_result
                .as_ref()
                .is_some_and(|t| t.col_count() > 0)
        {
            self.export_sql_result();
        }

        // ZoomIn also accepts Ctrl+Equals in addition to the user's binding —
        // on US layouts Ctrl++ is typed as Ctrl+= by the keyboard driver.
        let zoom_equals_fallback = shortcuts.combo(SA::ZoomIn).key == Some(egui::Key::Plus)
            && ctx.input(|i| {
                i.modifiers.command
                    && !i.modifiers.alt
                    && !i.modifiers.shift
                    && i.key_pressed(egui::Key::Equals)
            });
        if action_fired(SA::ZoomIn) || zoom_equals_fallback {
            self.zoom_percent = (self.zoom_percent + 5).min(500);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action_fired(SA::ZoomOut) {
            self.zoom_percent = self.zoom_percent.saturating_sub(5).max(25);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action_fired(SA::ZoomReset) {
            self.zoom_percent = 100;
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }

        let lower_fired = action_fired(SA::LowercaseSelection);
        let upper_fired = action_fired(SA::UppercaseSelection);
        if lower_fired || upper_fired {
            let op = if upper_fired {
                view_modes::text_ops::CaseOp::Upper
            } else {
                view_modes::text_ops::CaseOp::Lower
            };
            // Consume the key press so built-in TextEdit bindings (e.g. egui's
            // Ctrl+U = delete-to-start-of-line, which ignores Alt) don't also
            // fire on the same event.
            let combo = self.settings.shortcuts.combo(if upper_fired {
                SA::UppercaseSelection
            } else {
                SA::LowercaseSelection
            });
            if let Some(key) = combo.key {
                let modifiers = egui::Modifiers {
                    alt: combo.alt,
                    ctrl: combo.ctrl,
                    shift: combo.shift,
                    mac_cmd: false,
                    command: combo.ctrl,
                };
                ctx.input_mut(|i| i.consume_key(modifiers, key));
            }
            let sql_id = view_modes::sql_editor_id();
            let raw_id = egui::Id::new("raw_text_editor");
            let focused = ctx.memory(|m| m.focused());
            if focused == Some(sql_id) {
                let tab = &mut self.tabs[self.active_tab];
                view_modes::text_ops::apply_case_to_selection(ctx, sql_id, &mut tab.sql_query, op);
            } else if focused == Some(raw_id) {
                let tab = &mut self.tabs[self.active_tab];
                if let Some(ref mut content) = tab.raw_content
                    && view_modes::text_ops::apply_case_to_selection(ctx, raw_id, content, op)
                {
                    tab.raw_content_modified = true;
                }
            } else if lower_fired {
                self.transform_selected_cells(str::to_lowercase);
            } else {
                self.transform_selected_cells(str::to_uppercase);
            }
        }
        if action_fired(SA::SaveFileAs)
            && (self.tabs[self.active_tab].table.col_count() > 0
                || self.tabs[self.active_tab].raw_content_modified)
        {
            self.save_file_as();
        }
        if action_fired(SA::ReloadFile) && self.tabs[self.active_tab].table.source_path.is_some() {
            if self.tabs[self.active_tab].is_modified() {
                self.show_reload_confirm = true;
            } else {
                self.reload_active_file();
            }
        }
        if action_fired(SA::GoToCell) {
            self.nav_focus_requested = true;
        }
        if action_fired(SA::EditCell)
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            let tab = &mut self.tabs[self.active_tab];
            let binary_mode = self.settings.binary_display_mode;
            if let Some((r, c)) = tab.table_state.selected_cell {
                let text = tab
                    .table
                    .get(r, c)
                    .map(|v| v.display_with_binary_mode(binary_mode))
                    .unwrap_or_default();
                tab.table_state.begin_edit(r, c, text);
            }
        }
        if action_fired(SA::DuplicateRow) && !self.is_readonly() {
            self.duplicate_selected_rows();
        }
        if action_fired(SA::DeleteRow) && !self.is_readonly() {
            self.delete_selected_rows();
        }
        if action_fired(SA::InsertRowBelow)
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            let tab = &mut self.tabs[self.active_tab];
            let insert_at = tab
                .table_state
                .selected_cell
                .map(|(r, _)| r + 1)
                .unwrap_or(tab.table.row_count());
            tab.table.insert_row(insert_at);
            tab.filter_dirty = true;
        }
        if action_fired(SA::ToggleSqlPanel)
            && self.tabs[self.active_tab].view_mode == ViewMode::Table
        {
            self.tabs[self.active_tab].sql_panel_open = !self.tabs[self.active_tab].sql_panel_open;
        }

        // Undo / Redo / Mark / OpenSettings / OpenDocumentation — gated on no
        // TextEdit being focused so Ctrl+Z inside the SQL editor / raw editor
        // / search bar undoes *text*, not the table, and so the F-key dialog
        // shortcuts don't pop a window out from under the user mid-typing.
        if !text_edit_focused {
            if action_fired(SA::Undo) && !self.is_readonly() {
                self.do_undo();
            }
            if action_fired(SA::Redo) && !self.is_readonly() {
                self.do_redo();
            }
            if action_fired(SA::Mark) && !self.is_readonly() {
                let color = self.settings.default_mark_color;
                self.mark_selection_default(color);
            }
            if action_fired(SA::OpenSettings) {
                self.settings_dialog.open(&self.settings);
            }
            if action_fired(SA::OpenDocumentation) {
                self.show_documentation_dialog = true;
            }
            if action_fired(SA::OpenColumnInspector) {
                let tab = &mut self.tabs[self.active_tab];
                tab.show_column_inspector = true;
            }
            if action_fired(SA::OpenColumnFilter) {
                self.open_column_filter_dialog(None);
            }
            if action_fired(SA::CycleViewMode) {
                let tab = &mut self.tabs[self.active_tab];
                let modes = tab.available_view_modes();
                if modes.len() > 1 {
                    let current_idx = modes.iter().position(|m| *m == tab.view_mode).unwrap_or(0);
                    let next_idx = (current_idx + 1) % modes.len();
                    tab.view_mode = modes[next_idx];
                }
            }
            if action_fired(SA::ToggleReadOnly) {
                self.toggle_readonly();
            }
            if action_fired(SA::FitAllColumns) {
                // Cannot measure font widths without a Ui; queue the request
                // and let `draw_table` fulfil it on the next frame.
                self.tabs[self.active_tab]
                    .table_state
                    .fit_all_columns_requested = true;
            }
            if action_fired(SA::ReopenLastClosedTab) {
                self.reopen_last_closed_tab(ctx);
            }
            if action_fired(SA::CompareSelectedTabs) {
                // Needs exactly one Ctrl-click-selected tab. Multiple
                // selections are ambiguous (no defined left/right); zero
                // means the user pressed the key without staging anything.
                if self.tab_multi_selection.len() == 1
                    && let Some(&target) = self.tab_multi_selection.iter().next()
                {
                    self.begin_compare_with_tab(target);
                }
            }
            if action_fired(SA::ColumnValueFrequency) {
                let tab = &mut self.tabs[self.active_tab];
                if tab.table.col_count() > 0 {
                    // Prefer the column of the selected cell; fall back to the
                    // first column. Lets the shortcut be useful even without
                    // a click, mirroring how Ctrl+I (inspector) works.
                    let col = tab
                        .table_state
                        .selected_cell
                        .map(|(_, c)| c)
                        .filter(|&c| c < tab.table.col_count())
                        .unwrap_or(0);
                    tab.value_frequency_col = Some(col);
                    tab.value_frequency_size = octa::ui::settings::DialogSize::default();
                }
            }
            if action_fired(SA::ExportSchema) {
                super::dialogs::schema_export::open(self);
            }
            if action_fired(SA::MultiSearch) {
                self.toggle_multi_search();
            }
            if action_fired(SA::OpenChart) {
                self.open_chart_tab();
            }
            if action_fired(SA::FindDuplicates) {
                let tab = &mut self.tabs[self.active_tab];
                if tab.table.col_count() > 0 {
                    // Seed key from selection so the common one-column dedupe
                    // is two keys away (Ctrl+Shift+D, then Apply). Matches the
                    // toolbar-handler seeding path.
                    tab.find_duplicates_key_cols.clear();
                    if !tab.table_state.selected_cols.is_empty() {
                        for &c in &tab.table_state.selected_cols {
                            if c < tab.table.col_count() {
                                tab.find_duplicates_key_cols.insert(c);
                            }
                        }
                    } else if let Some((_, c)) = tab.table_state.selected_cell
                        && c < tab.table.col_count()
                    {
                        tab.find_duplicates_key_cols.insert(c);
                    }
                    tab.show_find_duplicates = true;
                }
            }
        }

        // --- Handle close request ---
        if ctx.input(|i| i.viewport().close_requested())
            && self.tabs[self.active_tab].is_modified()
            && !self.confirmed_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_close_confirm = true;
        }
    }

    /// Single chokepoint for "is the app currently locked". Every editing
    /// path that gates on read-only mode should call this rather than
    /// reading `self.readonly_mode` directly, so the gate stays auditable.
    pub(crate) fn is_readonly(&self) -> bool {
        self.readonly_mode
    }

    /// Flip the read-only mode flag. Queues a notice modal unless the user
    /// previously checked "Don't show this again".
    pub(crate) fn toggle_readonly(&mut self) {
        self.readonly_mode = !self.readonly_mode;
        if self.settings.show_readonly_notice {
            self.pending_readonly_notice = Some(super::state::ReadOnlyNotice {
                is_active: self.readonly_mode,
                suppress_future: false,
            });
        }
    }
}
