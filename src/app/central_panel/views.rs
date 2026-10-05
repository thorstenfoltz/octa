//! View-mode dispatch: the non-table views render and return early, the
//! table view draws the grid and routes its interactions.

use eframe::egui;

use octa::data::ViewMode;
use octa::ui;

use crate::app::state::OctaApp;
use crate::view_modes;

impl OctaApp {
    /// Draw the active tab when it is not a plain table. Returns true when
    /// something was drawn, so the caller skips the table.
    pub(super) fn render_non_table_view(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> bool {
        // Empty-file easter egg: render ASCII art instead of the table.
        if self.tabs[self.active_tab].empty_file_placeholder {
            render_empty_file_placeholder(ui, self.theme_mode);
            return true;
        }

        // Non-table view modes render and return early.
        if self.tabs[self.active_tab].view_mode == ViewMode::Compare {
            let syntax_cap = self.settings.syntax_highlight_max_bytes;
            let theme_mode = self.theme_mode;
            // Read before the mutable borrow below: `is_readonly()` reads
            // the active tab. The left pane of the text diff edits the
            // working file, so it funnels through the same gate as every
            // other edit path.
            let readonly = self.is_readonly();
            let action = view_modes::render_compare_view(
                ui,
                &mut self.tabs[self.active_tab],
                theme_mode,
                syntax_cap,
                readonly,
                self.settings.tab_size,
            );
            if action.close {
                let tab = &mut self.tabs[self.active_tab];
                tab.compare_right_path = None;
                tab.compare_right_raw = None;
                tab.compare_right_table = None;
                tab.compare_error = None;
                tab.view_mode = ViewMode::Table;
            }
            return true;
        }
        // Hoisted: `is_readonly()` reads the active tab, so it cannot be
        // called while `self.tabs` is mutably borrowed in the calls below.
        let readonly = self.is_readonly();
        if self.tabs[self.active_tab].view_mode == ViewMode::Notebook {
            view_modes::render_notebook_view(
                ctx,
                ui,
                &mut self.tabs[self.active_tab],
                self.theme_mode,
                self.settings.notebook_output_layout,
                readonly,
            );
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Markdown {
            view_modes::render_markdown_view(
                ui,
                &mut self.tabs[self.active_tab],
                readonly,
                self.settings.tab_size,
            );
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::EpubReader {
            view_modes::render_epub_view(ctx, ui, &mut self.tabs[self.active_tab]);
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Map {
            view_modes::render_map_view(ctx, ui, &mut self.tabs[self.active_tab], &self.settings);
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Chart {
            let tables = view_modes::render_chart_view(
                ui,
                &mut self.tabs[self.active_tab],
                self.theme_mode,
                octa::data::chart::ChartLimits {
                    max_points: self.settings.chart_max_points,
                    max_categories: self.settings.chart_max_categories,
                },
            );
            // Forecast to table: one new tab per series.
            for (label, table) in tables {
                let mut tab = crate::app::state::TabState::new(self.settings.default_search_mode);
                tab.table = table;
                tab.custom_tab_label = Some(label);
                tab.tab_hint = Some(octa::i18n::t("chart.forecast_to_table_hint"));
                tab.filter_dirty = true;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
            }
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Timeline {
            let name = self.tabs[self.active_tab].title_display();
            if let Some((table, hints)) =
                view_modes::render_timeline_view(ui, &mut self.tabs[self.active_tab])
            {
                let mut tab = crate::app::state::TabState::new(self.settings.default_search_mode);
                tab.table = table;
                tab.table_state.header_tooltips = hints;
                tab.custom_tab_label =
                    Some(octa::i18n::t("timeline.tab_label").replace("{name}", &name));
                tab.filter_dirty = true;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
            }
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Record {
            view_modes::render_record_view(
                ui,
                &mut self.tabs[self.active_tab],
                self.theme_mode,
                readonly,
            );
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::Raw {
            self.maybe_offer_raw_perf_prompt();
            let raw_action = view_modes::render_raw_view(
                ui,
                &mut self.tabs[self.active_tab],
                self.theme_mode,
                view_modes::raw_text::RawViewOpts {
                    color_aligned_columns: self.settings.color_aligned_columns,
                    tab_size: self.settings.tab_size,
                    warn_unalign: self.settings.warn_raw_align_reload,
                    readonly,
                    syntax_highlight_max_bytes: self.settings.syntax_highlight_max_bytes,
                },
            );
            if raw_action.confirm_unalign {
                self.show_unalign_confirm = true;
            }
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::JsonTree {
            view_modes::render_json_tree_view(ui, &mut self.tabs[self.active_tab], self.theme_mode);
            return true;
        }
        if self.tabs[self.active_tab].view_mode == ViewMode::YamlTree {
            view_modes::render_yaml_tree_view(ui, &mut self.tabs[self.active_tab], self.theme_mode);
            return true;
        }
        false
    }

    pub(super) fn render_table_view(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // --- Table view ---
        // Drain pending Copy/Cut/Paste events (and remappable
        // ShortcutAction triggers) here, AFTER all earlier panels (SQL
        // editor, toolbar search, status bar nav, etc.) have had a chance
        // to consume them. This keeps clipboard interactions in TextEdits
        // local to those editors and only routes the leftover events to
        // the table.
        self.handle_table_clipboard(ctx);

        // Archive action bar: rendered when the active tab was
        // loaded as a zip/tar/tgz so users can open the selected
        // entry without leaving the table.
        self.render_archive_action_bar(ui);

        // Highlight-search next/previous jump (table view): select and
        // scroll to the current match before drawing.
        self.apply_table_search_jump();

        let readonly = self.is_readonly();
        // Why Cell history is greyed out, if it is: the generic line plus
        // git's own reason, so "it is in a repo" can be checked against it.
        let cell_history_unavailable: Option<String> =
            match self.tabs[self.active_tab].git_location_or_why() {
                Ok(_) => None,
                Err(why) => {
                    let generic = octa::i18n::t("context_menu.cell_history_disabled");
                    Some(if why.is_empty() || why == generic {
                        generic
                    } else {
                        format!("{generic}\n\n{why}")
                    })
                }
            };
        // A partial database tab's facet popup lists the server's values.
        let facet_external = crate::app::db_view::server_conn(
            &self.tabs[self.active_tab],
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )
        .is_some();
        let tab = &mut self.tabs[self.active_tab];
        tab.table_state.facet_external = facet_external;
        // Large-file mode: the vertical scrollbar addresses the file, not
        // the loaded page. Off for every other tab.
        let virtual_rows = match (&tab.large, tab.table.total_rows) {
            (Some(_), Some(total)) => Some((tab.table.row_offset, total)),
            _ => None,
        };
        tab.table_state.set_virtual_rows(virtual_rows);
        let filtered = tab.filtered_rows.clone();
        let search_matches: std::collections::HashSet<(usize, usize)> =
            tab.search_cell_matches.iter().copied().collect();
        let current_match = tab.search_cell_matches.get(tab.search_nav.current).copied();
        // The sequential 1..N row column only adds information when the
        // displayed rows are a filtered subset; otherwise it duplicates the
        // original numbers. Show it while a search, column filter, or
        // "Filter to marked" is narrowing the visible rows.
        let filter_active = !tab.search_text.is_empty()
            || !tab.column_filters.is_empty()
            || !tab.predicate_filters.is_empty()
            || tab.duplicate_filter.is_some()
            || tab.mark_filter_active;
        let show_sequential = self.settings.show_sequential_row_numbers && filter_active;
        let hidden_cols = tab.hidden_columns.clone();
        let col_number_formats = tab.column_number_formats.clone();
        let cond_format_rules = tab.conditional_format_rules.clone();
        let validation_violations = tab.validation_violations.clone();
        let outlier_cells = tab.outlier_cells.clone();
        let table_cx = ui::table_view::TableCtx {
            theme_mode: self.theme_mode,
            filtered_rows: &filtered,
            show_row_numbers: self.settings.show_row_numbers,
            show_sequential_numbers: show_sequential,
            alternating_row_colors: self.settings.alternating_row_colors,
            negative_numbers_red: self.settings.negative_numbers_red,
            highlight_edits: self.settings.highlight_edits,
            font_size: self.settings.font_size * self.zoom_percent as f32 / 100.0,
            cell_line_breaks: self.settings.cell_line_breaks,
            show_invisibles: self.settings.show_invisible_chars,
            clickable_links: self.settings.clickable_links,
            binary_display_mode: self.settings.binary_display_mode,
            welcome_logo_texture: self.welcome_logo_texture.as_ref(),
            shortcuts: &self.settings.shortcuts,
            readonly,
            cell_history_unavailable: cell_history_unavailable.as_deref(),
            column_filters: &tab.column_filters,
            hidden_columns: &hidden_cols,
            thousands_separators: self.settings.thousands_separators_in_cells,
            separator_style: self.settings.number_separator_style,
            column_number_formats: &col_number_formats,
            search_matches: &search_matches,
            current_match,
            conditional_format_rules: &cond_format_rules,
            validation_violations: &validation_violations,
            outlier_cells: &outlier_cells,
            handles_input: true,
            // Only the split view has other panes to keep in step; it
            // overrides this per pane.
            scroll_all: false,
        };
        // Split view draws the same table once per pane, so it reports
        // one interaction per band; everything else reports exactly one.
        // The welcome screen (no columns) is not a table to split, and
        // `draw_table` short-circuits to the logo before any band exists.
        let interactions = if tab.table_state.is_split() && tab.table.col_count() > 0 {
            ui::table_view::draw_table_split(ui, &mut tab.table, &mut tab.table_state, table_cx)
        } else {
            vec![ui::table_view::draw_table(
                ui,
                &mut tab.table,
                &mut tab.table_state,
                table_cx,
            )]
        };

        let mut welcome_logo_clicked = false;
        let mut welcome_logo_rect = None;
        for interaction in interactions {
            welcome_logo_clicked |= interaction.welcome_logo_clicked;
            welcome_logo_rect = welcome_logo_rect.or(interaction.welcome_logo_rect);
            self.handle_table_interaction(interaction, ctx);
        }
        if welcome_logo_clicked {
            self.register_welcome_logo_click(ctx);
        }
        // Christmas-window overlay: paint a Santa hat on top of the
        // welcome-screen logo. Lives in the binary side (alongside the
        // other easter eggs) so the library renderer stays oblivious.
        // Gated on the same deadline as the snow above, so the whole
        // decoration goes at once rather than leaving a hat behind. The
        // snow overlay runs earlier in the frame and has already started
        // the clock, so this only reads it.
        if let Some(rect) = welcome_logo_rect
            && crate::app::easter_eggs::is_christmas_window()
            && crate::app::easter_eggs::festive_intro_running(&mut self.christmas_until)
        {
            crate::app::easter_eggs::paint_santa_hat_overlay(ctx, rect);
        }
    }

    /// Surface the slow-file prompt the first time the user actually enters
    /// the raw view of a CSV/TSV above the threshold. Triggered here (not at
    /// load time) so the prompt doesn't appear for users who only ever look
    /// at the table view of a large CSV.
    fn maybe_offer_raw_perf_prompt(&mut self) {
        const RAW_PERF_PROMPT_BYTES: u64 = 10 * 1024 * 1024;
        if self.pending_raw_perf_prompt.is_some() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.raw_perf_prompt_resolved {
            return;
        }
        let Some(file_size) = tab.raw_file_size else {
            return;
        };
        let format = tab.table.format_name.as_deref();
        if !matches!(format, Some("CSV") | Some("TSV")) || file_size <= RAW_PERF_PROMPT_BYTES {
            return;
        }
        let file_name = tab
            .table
            .source_path
            .as_deref()
            .map(std::path::Path::new)
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "this file".to_string());
        self.pending_raw_perf_prompt = Some(crate::app::state::RawPerfPrompt {
            tab_idx: self.active_tab,
            file_size,
            file_name,
        });
    }
}

/// Center the easter-egg ASCII art for an empty file. Picks the accent color
/// from the active theme so the art doesn't fight the theme palette.
fn render_empty_file_placeholder(ui: &mut egui::Ui, theme_mode: ui::theme::ThemeMode) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(crate::app::easter_eggs::EMPTY_FILE_ART)
                .monospace()
                .size(14.0)
                .color(colors.accent),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(crate::app::easter_eggs::EMPTY_FILE_TAGLINE)
                .italics()
                .size(13.0)
                .color(colors.text_secondary),
        );
    });
}
