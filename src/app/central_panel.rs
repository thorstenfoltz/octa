//! Central panel: status banner, view-mode dispatch (Notebook/Markdown/
//! Raw/JsonTree), the table renderer, and the table interaction handling
//! (column rename, type change, sort, context menu, lazy row loading).

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::data::{self, ViewMode};
use octa::formats;
use octa::ui;
use octa::ui::shortcuts::ShortcutAction as SA;

use super::file_io::load_remaining_parquet_rows;
use super::state::OctaApp;
use crate::view_modes;

impl OctaApp {
    pub(crate) fn render_central_panel(&mut self, parent_ui: &mut egui::Ui) {
        // Cloned up front: the interaction handler needs a Context, and
        // `parent_ui` is mutably borrowed by the panel closure below.
        let ctx_for_interaction = parent_ui.ctx().clone();
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        egui::CentralPanel::default().show(parent_ui, |ui| {
            // Per-theme background decoration (e.g. Manga's halftone field).
            // Painted before any content so widgets sit on top.
            ui::theme::paint_background_decoration(ui.painter(), ui.max_rect(), self.theme_mode);

            // Status message - auto-fades after `status_message_secs`, the
            // same span for every message. Confirmations and failures used to
            // differ (10s vs 60s) so an error could be read and copied; the
            // hover pause below does that job now without making every other
            // message outstay its welcome.
            let message_lifetime = self.settings.status_message_duration();
            let mut dismiss_status = false;
            if let Some((ref msg, instant)) = self.status_message
                && instant.elapsed() < message_lifetime
            {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let color = if is_success_message(msg) {
                    colors.success
                } else if msg.starts_with('\u{1f419}') {
                    // Easter-egg messages (kraken, etc.) get the accent.
                    colors.accent
                } else {
                    colors.error
                };
                // Selectable + right-click Copy: a failure message here is
                // often the whole reason a save or a connection did not work.
                let msg = msg.clone();
                let row = ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui::message::selectable_message_sized(ui, color, &msg, Some(12.0));
                    // Dismiss now, for anyone who has read it and wants the
                    // room back before the timer runs out.
                    if ui
                        .small_button("x")
                        .on_hover_text(octa::i18n::t("status_bar.dismiss_hint"))
                        .clicked()
                    {
                        dismiss_status = true;
                    }
                });
                // Pause the countdown while the pointer is over the message.
                // Implemented by pushing the stored start forward by one
                // frame rather than by tracking paused time separately, so
                // `elapsed()` simply stops growing and the ~30 places that
                // set `status_message` keep working unchanged.
                // `contains_pointer`, not `hovered`: the message itself is a
                // selectable (interactive) label, so it takes the hover from
                // the row that wraps it and the pause never fired.
                if row.response.contains_pointer()
                    && let Some((_, ref mut started)) = self.status_message
                {
                    let dt = ui.ctx().input(|i| i.stable_dt).min(0.25);
                    if let Some(pushed) =
                        started.checked_add(std::time::Duration::from_secs_f32(dt))
                    {
                        *started = pushed;
                    }
                    // Keep animating while hovered so the pause is honoured
                    // even when nothing else asks for a repaint.
                    ui.ctx().request_repaint();
                } else {
                    // egui only paints when something asks it to, so a message
                    // whose time is up stays on screen until the next unrelated
                    // event repaints the frame - which read as "the timeout in
                    // Settings does nothing". Book the frame that removes it.
                    ui.ctx()
                        .request_repaint_after(message_lifetime.saturating_sub(instant.elapsed()));
                }
                ui.add_space(4.0);
            }
            if dismiss_status {
                self.status_message = None;
            }

            // Date format-change banner. Stays visible until the user
            // dismisses it; the inference pass only sets it when the source
            // layout differs from the canonical ISO display.
            let mut dismiss_warning = false;
            let mut keep_dates = false;
            if let Some(warning) = self
                .pending_date_warning
                .as_ref()
                .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
            {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let names: Vec<String> = warning
                    .entries
                    .iter()
                    .map(|e| format!("{} ({})", e.column_name, e.source_label))
                    .collect();
                // Bounded, and the row wraps: a wide file can promote two
                // hundred columns, and spelled out in one non-wrapping row
                // that label pushes Okay and Dismiss off the screen edge.
                let summary = ui::message::elide_list(&names, ui::message::BANNER_LIST_MAX);
                let full = names.join(", ");
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            octa::i18n::t("banner.dates_detected").replace("{cols}", &summary),
                        )
                        .color(colors.warning)
                        .size(12.0),
                    )
                    .on_hover_text(&full);
                    // "Okay" accepts the date display and closes the banner;
                    // "Dismiss" reverts the promoted columns back to text.
                    if ui
                        .small_button(octa::i18n::t("banner.okay"))
                        .on_hover_text(octa::i18n::t("banner.dates_keep_tip"))
                        .clicked()
                    {
                        keep_dates = true;
                    }
                    if ui
                        .small_button(octa::i18n::t("banner.dismiss"))
                        .on_hover_text(octa::i18n::t("banner.dates_revert_tip"))
                        .clicked()
                    {
                        dismiss_warning = true;
                    }
                    ui.label(
                        egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                            .color(colors.text_muted)
                            .size(11.0),
                    );
                });
                ui.add_space(4.0);
            }
            if dismiss_warning {
                self.revert_promoted_date_columns();
            } else if keep_dates {
                self.pending_date_warning = None;
            }

            // Near-miss date banner. Lists columns that looked date-shaped but
            // had values that could not be parsed, so they stayed text. This
            // explains *why* a column the user expected to be a date is still
            // text. Dismiss-only - nothing was changed.
            let mut dismiss_date_parse = false;
            if let Some(warning) = self
                .pending_date_parse_warning
                .as_ref()
                .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
            {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let names: Vec<String> = warning
                    .entries
                    .iter()
                    .map(|e| {
                        let samples = e.samples.join(", ");
                        format!(
                            "{} (looks like {}, {} of {} values parsed; e.g. {})",
                            e.column_name, e.source_label, e.parsed, e.total, samples
                        )
                    })
                    .collect();
                // These entries are long sentences, so the cap is lower.
                let summary = ui::message::elide_list(&names, 2);
                let full = names.join("; ");
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            octa::i18n::t("banner.dates_unparsed").replace("{cols}", &summary),
                        )
                        .color(colors.warning)
                        .size(12.0),
                    )
                    .on_hover_text(&full);
                    if ui
                        .small_button(octa::i18n::t("banner.dismiss"))
                        .on_hover_text(octa::i18n::t("banner.close_tip"))
                        .clicked()
                    {
                        dismiss_date_parse = true;
                    }
                    ui.label(
                        egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                            .color(colors.text_muted)
                            .size(11.0),
                    );
                });
                ui.add_space(4.0);
            }
            if dismiss_date_parse {
                self.pending_date_parse_warning = None;
            }

            // Whitespace-trim banner. Lists the columns whose string cells had
            // leading/trailing whitespace stripped on load. Dismiss-only - the
            // trimming itself already happened.
            let mut dismiss_trim = false;
            let mut undo_trim = false;
            if let Some(warning) = self
                .pending_trim_warning
                .as_ref()
                .filter(|w| w.tab_idx == self.active_tab && !w.columns.is_empty())
            {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let summary =
                    ui::message::elide_list(&warning.columns, ui::message::BANNER_LIST_MAX);
                let full = warning.columns.join(", ");
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            octa::i18n::t("banner.trimmed")
                                .replace("{n}", &warning.columns.len().to_string())
                                .replace("{cols}", &summary),
                        )
                        .color(colors.warning)
                        .size(12.0),
                    )
                    .on_hover_text(&full);
                    // "Okay" accepts the trim and closes the banner; "Dismiss"
                    // undoes it, restoring the original leading/trailing
                    // whitespace.
                    if ui
                        .small_button(octa::i18n::t("banner.okay"))
                        .on_hover_text(octa::i18n::t("banner.trim_keep_tip"))
                        .clicked()
                    {
                        dismiss_trim = true;
                    }
                    if ui
                        .small_button(octa::i18n::t("banner.dismiss"))
                        .on_hover_text(octa::i18n::t("banner.trim_undo_tip"))
                        .clicked()
                    {
                        undo_trim = true;
                    }
                    ui.label(
                        egui::RichText::new(octa::i18n::t("banner.disable_hint"))
                            .color(colors.text_muted)
                            .size(11.0),
                    );
                });
                ui.add_space(4.0);
            }
            if undo_trim {
                self.revert_trimmed_columns();
            } else if dismiss_trim {
                self.pending_trim_warning = None;
            }

            // Number-promotion banner. Lists the text columns that were read as
            // European- or English-formatted numbers. Okay accepts, Dismiss
            // puts the original strings back.
            let mut dismiss_numbers = false;
            let mut undo_numbers = false;
            if let Some(warning) = self
                .pending_number_warning
                .as_ref()
                .filter(|w| w.tab_idx == self.active_tab && !w.entries.is_empty())
            {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let names: Vec<String> = warning
                    .entries
                    .iter()
                    .map(|e| e.column_name.clone())
                    .collect();
                let summary = ui::message::elide_list(&names, ui::message::BANNER_LIST_MAX);
                let full = names.join(", ");
                let style_label = warning.entries[0].style_label;
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            octa::i18n::t("banner.numbers_promoted")
                                .replace("{n}", &warning.entries.len().to_string())
                                .replace("{cols}", &summary)
                                .replace("{style}", style_label),
                        )
                        .color(colors.warning)
                        .size(12.0),
                    )
                    .on_hover_text(&full);
                    if ui
                        .small_button(octa::i18n::t("banner.okay"))
                        .on_hover_text(octa::i18n::t("banner.numbers_keep_tip"))
                        .clicked()
                    {
                        dismiss_numbers = true;
                    }
                    if ui
                        .small_button(octa::i18n::t("banner.dismiss"))
                        .on_hover_text(octa::i18n::t("banner.numbers_undo_tip"))
                        .clicked()
                    {
                        undo_numbers = true;
                    }
                });
                ui.add_space(4.0);
            }
            if undo_numbers {
                self.revert_promoted_number_columns();
            } else if dismiss_numbers {
                self.pending_number_warning = None;
            }

            // Per-tab notice banner (parse fallback, inventory truncation, the
            // file-internals facts). The Raw view renders this itself inside
            // its scroll area, so only the other views need it here - without
            // this, a banner set on a table-shaped tab was simply invisible.
            if self.tabs[self.active_tab].view_mode != ViewMode::Raw
                && let Some(text) = self.tabs[self.active_tab].parse_error_banner.clone()
                && crate::view_modes::raw_text::render_parse_error_banner(
                    ui,
                    &text,
                    self.theme_mode,
                )
            {
                self.tabs[self.active_tab].parse_error_banner = None;
            }

            // Comparison-filter chips (`amount greater than 1000`), applied by
            // the search bar's Ask mode. Shown so a wrong interpretation is
            // visible and removable rather than a mystery.
            if !self.tabs[self.active_tab].predicate_filters.is_empty() {
                let mut remove: Option<usize> = None;
                let labels: Vec<String> = {
                    let tab = &self.tabs[self.active_tab];
                    tab.predicate_filters
                        .iter()
                        .map(|f| f.label(&tab.table))
                        .collect()
                };
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(octa::i18n::t("search.ask_filters"))
                            .size(11.0)
                            .color(ui::theme::ThemeColors::for_mode(self.theme_mode).text_muted),
                    );
                    for (i, label) in labels.iter().enumerate() {
                        if ui
                            .small_button(format!("{label}  x"))
                            .on_hover_text(octa::i18n::t("search.ask_filter_remove"))
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    }
                    if labels.len() > 1
                        && ui
                            .small_button(octa::i18n::t("search.ask_filters_clear"))
                            .clicked()
                    {
                        remove = Some(usize::MAX);
                    }
                });
                ui.add_space(4.0);
                if let Some(i) = remove {
                    let tab = &mut self.tabs[self.active_tab];
                    if i == usize::MAX {
                        tab.predicate_filters.clear();
                    } else if i < tab.predicate_filters.len() {
                        tab.predicate_filters.remove(i);
                    }
                    tab.filter_dirty = true;
                }
            }

            // Recompute filter before drawing (toolbar actions earlier in the
            // frame may have dirtied it).
            if self.tabs[self.active_tab].filter_dirty {
                self.recompute_filter();
            }

            // Empty-file easter egg: render ASCII art instead of the table.
            if self.tabs[self.active_tab].empty_file_placeholder {
                render_empty_file_placeholder(ui, self.theme_mode);
                return;
            }

            // Non-table view modes render and return early.
            if self.tabs[self.active_tab].view_mode == ViewMode::Compare {
                let syntax_cap = self.settings.syntax_highlight_max_bytes;
                let theme_mode = self.theme_mode;
                let action = view_modes::render_compare_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    theme_mode,
                    syntax_cap,
                );
                if action.close {
                    let tab = &mut self.tabs[self.active_tab];
                    tab.compare_right_path = None;
                    tab.compare_right_raw = None;
                    tab.compare_right_table = None;
                    tab.compare_error = None;
                    tab.view_mode = ViewMode::Table;
                }
                return;
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
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Markdown {
                view_modes::render_markdown_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    readonly,
                    self.settings.tab_size,
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::EpubReader {
                view_modes::render_epub_view(ctx, ui, &mut self.tabs[self.active_tab]);
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Map {
                view_modes::render_map_view(
                    ctx,
                    ui,
                    &mut self.tabs[self.active_tab],
                    &self.settings,
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Chart {
                view_modes::render_chart_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                    octa::data::chart::ChartLimits {
                        max_points: self.settings.chart_max_points,
                        max_categories: self.settings.chart_max_categories,
                    },
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Record {
                view_modes::render_record_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                    readonly,
                );
                return;
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
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::JsonTree {
                view_modes::render_json_tree_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::YamlTree {
                view_modes::render_yaml_tree_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }

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

            let os_has_clipboard = self.os_clipboard_has_text();
            let readonly = self.is_readonly();
            let tab = &mut self.tabs[self.active_tab];
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
            let filtered_cols: std::collections::HashSet<usize> =
                tab.column_filters.keys().copied().collect();
            // The sequential 1..N row column only adds information when the
            // displayed rows are a filtered subset; otherwise it duplicates the
            // original numbers. Show it while a search, column filter, or
            // "Filter to marked" is narrowing the visible rows.
            let filter_active = !tab.search_text.is_empty()
                || !tab.column_filters.is_empty()
                || !tab.predicate_filters.is_empty()
                || tab.mark_filter_active;
            let show_sequential = self.settings.show_sequential_row_numbers && filter_active;
            let hidden_cols = tab.hidden_columns.clone();
            let col_number_formats = tab.column_number_formats.clone();
            let cond_format_rules = tab.conditional_format_rules.clone();
            let validation_violations = tab.validation_violations.clone();
            let outlier_cells = tab.outlier_cells.clone();
            let os_has_clip = tab.table_state.clipboard.is_some() || os_has_clipboard;
            let table_cx = ui::table_view::TableCtx {
                theme_mode: self.theme_mode,
                filtered_rows: &filtered,
                os_clipboard_has_content: os_has_clip,
                show_row_numbers: self.settings.show_row_numbers,
                show_sequential_numbers: show_sequential,
                alternating_row_colors: self.settings.alternating_row_colors,
                negative_numbers_red: self.settings.negative_numbers_red,
                highlight_edits: self.settings.highlight_edits,
                font_size: self.settings.font_size * self.zoom_percent as f32 / 100.0,
                cell_line_breaks: self.settings.cell_line_breaks,
                clickable_links: self.settings.clickable_links,
                binary_display_mode: self.settings.binary_display_mode,
                welcome_logo_texture: self.welcome_logo_texture.as_ref(),
                shortcuts: &self.settings.shortcuts,
                readonly,
                filtered_columns: &filtered_cols,
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
                self.handle_table_interaction(interaction, &ctx_for_interaction);
            }
            if welcome_logo_clicked {
                self.register_welcome_logo_click(ctx);
            }
            // Christmas-window overlay: paint a Santa hat on top of the
            // welcome-screen logo. Lives in the binary side (alongside the
            // other easter eggs) so the library renderer stays oblivious.
            if let Some(rect) = welcome_logo_rect
                && super::easter_eggs::is_christmas_window()
            {
                super::easter_eggs::paint_santa_hat_overlay(ctx, rect);
            }
        });
    }

    /// Route `Event::Copy` / `Event::Cut` / `Event::Paste` and the remappable
    /// `ShortcutAction::Copy/Cut/Paste` triggers to the table-level clipboard
    /// ops - but only when no text is marked and no TextEdit has focus.
    ///
    /// Subtle invariant: egui's TextEdit reads `Event::Paste` etc. without
    /// removing them from `i.events`, AND `draw_table` later in the frame
    /// also has its own paste-event picker. So we ALWAYS drain those events
    /// here (so nothing else fires on them), but only act on them when no
    /// TextEdit is focused. When the SQL editor / search bar / any other
    /// TextEdit is focused, the events have already been consumed by that
    /// editor in an earlier panel and we just throw them away.
    fn handle_table_clipboard(&mut self, ctx: &egui::Context) {
        // Marked text owns Ctrl+C, wherever it is: a chat bubble, tool output,
        // a message in a dialog, the focused text box. Stand down entirely -
        // the events stay in the queue for egui's own copy, and `do_copy`
        // never runs, so the direct OS-clipboard write cannot race the one
        // egui queues at the end of the pass. Clicking outside the text clears
        // the selection and hands the shortcut straight back to the table.
        if octa::ui::text_selection::has_active_selection(ctx) {
            return;
        }
        if self.tabs[self.active_tab].view_mode != ViewMode::Table {
            return;
        }
        if self.tabs[self.active_tab].table.col_count() == 0 {
            return;
        }

        let mut do_copy = false;
        let mut do_cut = false;
        let mut paste_text: Option<String> = None;
        let mut had_paste_event = false;

        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Copy => {
                    do_copy = true;
                    false
                }
                egui::Event::Cut => {
                    do_cut = true;
                    false
                }
                egui::Event::Paste(t) => {
                    paste_text = Some(t.clone());
                    had_paste_event = true;
                    false
                }
                _ => true,
            });
        });

        // While the Shortcuts grid is recording, Ctrl+C / X / V are a binding
        // being typed, not a clipboard command. The events are still drained
        // above so nothing later in the frame acts on them either.
        if octa::ui::shortcuts::capture_mode() {
            return;
        }

        // If any TextEdit holds focus (SQL editor, raw editor, search bar,
        // inline cell editor, dialogs, status-bar nav...), the events above
        // were already handled by that editor when it rendered. Drop them
        // and don't react further on the table side.
        let text_edit_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some()
            || ctx.egui_wants_keyboard_input();
        if text_edit_focused {
            return;
        }

        // Configurable shortcut path (e.g. user remapped Copy to Ctrl+Shift+C).
        let shortcuts = self.settings.shortcuts.clone();
        if ctx.input(|i| shortcuts.triggered(SA::Copy, i)) {
            do_copy = true;
        }
        if ctx.input(|i| shortcuts.triggered(SA::Cut, i)) {
            do_cut = true;
        }
        if ctx.input(|i| shortcuts.triggered(SA::Paste, i)) && !had_paste_event {
            paste_text = None;
            had_paste_event = true;
        }

        if do_copy {
            self.do_copy();
        }
        if do_cut {
            self.do_cut();
        }
        if had_paste_event {
            self.do_paste(paste_text);
        }
    }

    /// Revert every column that the date inference pass promoted under a
    /// non-canonical layout, restoring the source strings the user saw on
    /// disk and switching the column type back to `Utf8`. Called from the
    /// "Dismiss" button on the date-format-change banner.
    fn revert_promoted_date_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_date_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for entry in &warning.entries {
            if entry.col_idx >= tab.table.col_count() {
                continue;
            }
            for (row, original) in entry.original_values.iter().enumerate() {
                if row >= tab.table.row_count() {
                    break;
                }
                let new_cell = match original {
                    Some(s) => CellValue::String(s.clone()),
                    None => CellValue::Null,
                };
                tab.table.rows[row][entry.col_idx] = new_cell;
            }
            if let Some(col) = tab.table.columns.get_mut(entry.col_idx) {
                col.data_type = "Utf8".to_string();
            }
        }
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }

    /// Undo the load-time number promotion, putting the original strings back
    /// and returning the columns to text. Called from the "Dismiss" button on
    /// the number banner; the sibling of `revert_promoted_date_columns`.
    fn revert_promoted_number_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_number_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for entry in &warning.entries {
            if entry.col_idx >= tab.table.col_count() {
                continue;
            }
            for (row, original) in entry.original_values.iter().enumerate() {
                if row >= tab.table.row_count() {
                    break;
                }
                tab.table.rows[row][entry.col_idx] = match original {
                    Some(s) => CellValue::String(s.clone()),
                    None => CellValue::Null,
                };
            }
            if let Some(col) = tab.table.columns.get_mut(entry.col_idx) {
                col.data_type = "Utf8".to_string();
            }
        }
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
    }

    /// Undo the load-time whitespace trim, restoring the original column titles
    /// and cell whitespace recorded in the trim banner's undo log. Called from
    /// the "Dismiss" button on the whitespace-trim banner.
    fn revert_trimmed_columns(&mut self) {
        use octa::data::CellValue;
        let Some(warning) = self.pending_trim_warning.take() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(warning.tab_idx) else {
            return;
        };
        for (col_idx, title) in &warning.undo.titles {
            if let Some(col) = tab.table.columns.get_mut(*col_idx) {
                col.name = title.clone();
            }
        }
        for (col_idx, cells) in &warning.undo.cells {
            for (row_idx, value) in cells {
                if *row_idx < tab.table.row_count() && *col_idx < tab.table.col_count() {
                    tab.table.rows[*row_idx][*col_idx] = CellValue::String(value.clone());
                }
            }
        }
        // Restoring whitespace is an in-place change; re-sync the DB diff-save
        // baseline so it isn't later seen as an edit / schema change.
        super::file_io::resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        tab.table_state.invalidate_row_heights();
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
        self.pending_raw_perf_prompt = Some(super::state::RawPerfPrompt {
            tab_idx: self.active_tab,
            file_size,
            file_name,
        });
    }

    fn handle_table_interaction(
        &mut self,
        interaction: ui::table_view::TableInteraction,
        ctx: &egui::Context,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(col_idx) = interaction.header_col_clicked {
            tab.insert_col_at = Some(col_idx + 1);
            if let Some((row, _)) = tab.table_state.selected_cell {
                tab.table_state.selected_cell = Some((row, col_idx));
            }
        }

        if let Some((from, to)) = interaction.col_drag_move {
            tab.table.move_column(from, to);
            if let Some((row, col)) = tab.table_state.selected_cell {
                let new_col = if col == from {
                    to
                } else if from < to {
                    if col > from && col <= to {
                        col - 1
                    } else {
                        col
                    }
                } else if col >= to && col < from {
                    col + 1
                } else {
                    col
                };
                tab.table_state.selected_cell = Some((row, new_col));
            }
            if from < tab.table_state.col_widths.len() && to < tab.table_state.col_widths.len() {
                let w = tab.table_state.col_widths.remove(from);
                tab.table_state.col_widths.insert(to, w);
            }
            tab.filter_dirty = true;
        }

        let tab = &mut self.tabs[self.active_tab];
        if let Some((col_idx, new_name)) = interaction.rename_column
            && col_idx < tab.table.columns.len()
            && !new_name.is_empty()
        {
            tab.table.columns[col_idx].name = new_name;
            tab.table.structural_changes = true;
            tab.table_state.widths_initialized = false;
        }

        if let Some((col_idx, new_type)) = interaction.change_col_type
            && !tab.table.convert_column(col_idx, &new_type)
        {
            self.status_message = Some((
                format!("Cannot convert column to {new_type}: some values are incompatible"),
                std::time::Instant::now(),
            ));
        }

        let tab = &mut self.tabs[self.active_tab];
        let large_sort = tab.large.is_some().then_some(()).and_then(|()| {
            interaction
                .sort_rows_asc_by
                .map(|c| (c, true))
                .or_else(|| interaction.sort_rows_desc_by.map(|c| (c, false)))
        });
        if large_sort.is_none() {
            if let Some(col_idx) = interaction.sort_rows_asc_by {
                tab.table.sort_rows_by_column(col_idx, true);
                tab.filter_dirty = true;
            }
            if let Some(col_idx) = interaction.sort_rows_desc_by {
                tab.table.sort_rows_by_column(col_idx, false);
                tab.filter_dirty = true;
            }
        }

        // --- Context menu: row operations ---
        if interaction.ctx_insert_row {
            let insert_at = match tab.table_state.selected_cell {
                Some((row, _)) => row + 1,
                None => tab.table.row_count(),
            };
            tab.table.insert_row(insert_at);
            let sel_col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
            tab.table_state.selected_cell = Some((insert_at, sel_col));
            tab.table_state.editing_cell = None;
            tab.filter_dirty = true;
        }
        if interaction.ctx_delete_row
            && let Some((row, col)) = tab.table_state.selected_cell
        {
            tab.table.delete_row(row);
            tab.table_state.editing_cell = None;
            if tab.table.row_count() == 0 {
                tab.table_state.selected_cell = None;
            } else {
                let new_row = row.min(tab.table.row_count() - 1);
                tab.table_state.selected_cell = Some((new_row, col));
            }
            tab.filter_dirty = true;
        }
        if interaction.ctx_move_row_up
            && let Some((row, col)) = tab.table_state.selected_cell
            && row > 0
        {
            tab.table.move_row(row, row - 1);
            tab.table_state.selected_cell = Some((row - 1, col));
            tab.filter_dirty = true;
        }
        if interaction.ctx_move_row_down
            && let Some((row, col)) = tab.table_state.selected_cell
            && row + 1 < tab.table.row_count()
        {
            tab.table.move_row(row, row + 1);
            tab.table_state.selected_cell = Some((row + 1, col));
            tab.filter_dirty = true;
        }

        // --- Context menu: column operations ---
        if interaction.ctx_insert_column {
            tab.show_add_column_dialog = true;
            tab.new_col_name.clear();
            tab.new_col_type = "String".to_string();
            tab.new_col_formula.clear();
            tab.insert_col_at = tab.table_state.selected_cell.map(|(_, c)| c + 1);
        }
        if interaction.ctx_delete_column && tab.table.col_count() > 0 {
            self.open_delete_columns_dialog();
        }
        if interaction.ctx_move_col_left {
            let tab = &mut self.tabs[self.active_tab];
            if let Some((row, col)) = tab.table_state.selected_cell
                && col > 0
            {
                tab.table.move_column(col, col - 1);
                tab.table_state.selected_cell = Some((row, col - 1));
                tab.table_state.widths_initialized = false;
            }
        }
        if interaction.ctx_move_col_right {
            let tab = &mut self.tabs[self.active_tab];
            if let Some((row, col)) = tab.table_state.selected_cell
                && col + 1 < tab.table.col_count()
            {
                tab.table.move_column(col, col + 1);
                tab.table_state.selected_cell = Some((row, col + 1));
                tab.table_state.widths_initialized = false;
            }
        }

        // --- Copy / Paste ---
        let tab = &mut self.tabs[self.active_tab];
        if interaction.ctx_copy_cell
            && let Some((row, col)) = tab.table_state.selected_cell
        {
            let text = tab
                .table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            tab.table_state.clipboard = Some(text.clone());
            if let Some(ref cb) = self.os_clipboard
                && let Ok(mut cb) = cb.lock()
            {
                let _ = cb.set_text(&text);
            }
        }
        if interaction.ctx_copy {
            self.do_copy();
        }
        if interaction.ctx_copy_markdown {
            self.do_copy_markdown();
        }
        if interaction.ctx_cut {
            self.do_cut();
        }
        if interaction.ctx_paste {
            self.do_paste(interaction.paste_text);
        }
        // --- Add bookmark (cell right-click "Add bookmark...") ---
        // The context menu sets `selected_cell` to the clicked cell, so
        // `begin_add_bookmark` bookmarks that row + column.
        if interaction.ctx_add_bookmark {
            self.begin_add_bookmark();
        }

        // --- Parse in new tab (from cell right-click context menu) ---
        // Resolve scope into modal state *before* taking a `&mut tab`
        // borrow for the mark dispatch below - `build_modal_state`
        // reads the table immutably.
        if let Some(scope) = interaction.ctx_parse_in_new_tab {
            let tab_ref = &self.tabs[self.active_tab];
            self.pending_parse_modal =
                super::dialogs::parse_in_new_tab::build_modal_state(tab_ref, scope);
        }

        // --- Filter values... on a column header (right-click) ---
        if let Some(col_idx) = interaction.ctx_filter_column {
            self.open_column_filter_dialog(Some(col_idx));
        }

        // --- Hide column (right-click) ---
        if let Some(col_idx) = interaction.ctx_hide_column {
            self.tabs[self.active_tab].hidden_columns.insert(col_idx);
        }

        // --- Value frequency (right-click "Value frequency...") ---
        if let Some(col_idx) = interaction.ctx_value_frequency {
            let tab = &mut self.tabs[self.active_tab];
            tab.value_frequency_col = Some(col_idx);
            tab.value_frequency_size = octa::ui::settings::DialogSize::default();
        }

        // --- Number format (right-click "Number format...") ---
        if let Some(col_idx) = interaction.ctx_column_format {
            self.tabs[self.active_tab].open_column_format(col_idx);
        }

        // --- Color marks ---
        let tab = &mut self.tabs[self.active_tab];
        if let Some((keys, color)) = interaction.set_mark {
            for key in keys {
                tab.table.set_mark(key, color);
            }
        }
        if let Some(keys) = interaction.clear_mark {
            for key in keys {
                tab.table.clear_mark(key);
            }
        }

        // --- Large-file mode: the loaded window follows the scroll ---
        let tab = &self.tabs[self.active_tab];
        if tab.large.is_some() {
            // Dragging the (virtual) scrollbar addresses the file directly and
            // outranks the edge triggers below, which only ever move by a page.
            if let Some(row) = interaction.jump_to_row {
                self.large_jump_to_row(row);
                return;
            }
            // Sorting a file that is not in memory means asking for the page
            // again with an ORDER BY, not reordering the rows on screen.
            let step = if interaction.needs_more_rows {
                Some(1)
            } else if tab.table_state.scroll_y() <= 0.0
                && tab.large_page_key.as_ref().is_some_and(|k| k.offset > 0)
            {
                Some(-1)
            } else {
                None
            };
            // The search box is a WHERE clause here, not a row-vector filter.
            let order = large_sort.or_else(|| tab.large_page_key.as_ref().and_then(|k| k.order));
            // The search box becomes a WHERE clause, but only once the typing
            // stops: a `count(*)` per keystroke over a file this size would
            // lock the window.
            let want = crate::app::large_file::search_filter(tab, &tab.search_text.clone());
            let applied = tab
                .large_page_key
                .as_ref()
                .map(|k| k.filter.clone())
                .unwrap_or_default();
            const SETTLE: std::time::Duration = std::time::Duration::from_millis(400);
            let mut filter_ready = want == applied;
            if !filter_ready {
                match &self.large_filter_pending {
                    Some((pending, since)) if *pending == want => {
                        if since.elapsed() >= SETTLE {
                            filter_ready = true;
                        } else {
                            ctx.request_repaint_after(SETTLE);
                        }
                    }
                    _ => {
                        self.large_filter_pending = Some((want.clone(), std::time::Instant::now()));
                        ctx.request_repaint_after(SETTLE);
                    }
                }
            }
            if filter_ready {
                self.large_filter_pending = None;
                self.large_apply_view(order, want);
                if let Some(step) = step {
                    self.large_page_step(step);
                }
            }
            return;
        }

        let tab = &mut self.tabs[self.active_tab];

        // --- Lazy loading: load more rows on demand ---
        if interaction.needs_more_rows
            && tab.bg_can_load_more
            && tab.bg_row_buffer.is_none()
            && tab.table.total_rows.is_some()
        {
            tab.bg_can_load_more = false;
            let buffer = Arc::new(Mutex::new(Vec::<Vec<data::CellValue>>::new()));
            let done_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let exhausted_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            tab.bg_row_buffer = Some(buffer.clone());
            tab.bg_loading_done = done_flag.clone();
            tab.bg_file_exhausted = exhausted_flag.clone();

            let skip_rows = tab.table.row_offset + tab.table.row_count();
            // Background-load chunk size mirrors the first-load cap so the user's
            // Settings choice applies to both passes consistently.
            let max_chunk = formats::initial_load_rows();

            // A live-database tab has no `source_path`; it pages the server
            // instead, with the same buffer/flag protocol the file readers use.
            if tab.table.source_path.is_none()
                && let Some(origin) = tab.db_origin.clone()
            {
                let page_rows = self.settings.db_page_size();
                let conn = self
                    .settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == origin.conn_id)
                    .cloned();
                if let Some(conn) = conn {
                    let secret =
                        octa::ui::settings::db_secrets::get_db_secret(&conn.id, &self.settings);
                    let ssh_secret =
                        octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &self.settings);
                    let cache = self.db_conn_cache.clone();
                    // Reuse the sidebar's own result channel rather than
                    // adding a second one: it is drained every frame and puts
                    // the message in the status bar. A page that failed
                    // silently would look exactly like the end of the table,
                    // which is the failure this whole path exists to remove.
                    let pending = self.db_browser.pending_open.clone();
                    let failure_label = format!("{} @ {}", origin.table, conn.name);
                    let repaint = ctx.clone();
                    let (load_finished, cancel_slot, cancelled) = self.begin_db_load(format!(
                        "{} {failure_label}",
                        octa::i18n::t("db.loading_more")
                    ));
                    // ponytail: LIMIT/OFFSET paging with no ORDER BY, so a
                    // server free to reorder between pages can repeat or skip
                    // a row. Same ceiling `fetch_batches` and every table copy
                    // already accept; an ORDER BY would force a full sort per
                    // page on exactly the warehouse tables this is for.
                    let sql = octa::db::paged_sql(
                        conn.engine,
                        &octa::db::select_all_sql(
                            conn.engine,
                            origin.catalog.as_deref(),
                            &origin.schema,
                            &origin.table,
                        ),
                        page_rows,
                        skip_rows,
                    );
                    std::thread::spawn(move || {
                        let _done = crate::app::flag_guard::FlagOnDrop::new(done_flag, true);
                        let _load = crate::app::flag_guard::FlagOnDrop::new(load_finished, true);
                        let read =
                            cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                                // Same reason as the sidebar open: `with_conn`
                                // retries once, and after a Cancel that retry
                                // would re-run the statement just stopped.
                                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                                    anyhow::bail!("{}", octa::i18n::t("db.load_cancelled"));
                                }
                                if let Ok(mut slot) = cancel_slot.lock() {
                                    *slot = c.cancel_handle();
                                }
                                c.query(&sql)
                            });
                        match read {
                            Ok(t) => {
                                // A short page is the end of the table.
                                if t.rows.len() < page_rows {
                                    exhausted_flag
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                if let Ok(mut buf) = buffer.lock() {
                                    buf.extend(t.rows);
                                }
                            }
                            Err(e) => {
                                // Deliberately NOT `exhausted`: this page
                                // failed or was cancelled, which says nothing
                                // about whether the server still holds rows.
                                // Claiming otherwise would retire the tab's
                                // "+" and report a partial table as complete.
                                // `bg_can_load_more` is already false, so
                                // nothing retries on its own; reopen the table
                                // to try again.
                                if let Ok(mut p) = pending.lock() {
                                    p.push(super::db_browser::DbOpenResult::Failed(format!(
                                        "{} {failure_label}: {e:#}",
                                        octa::i18n::t("db.page_failed")
                                    )));
                                }
                                repaint.request_repaint();
                            }
                        }
                    });
                } else {
                    // The connection was deleted while the tab was open.
                    exhausted_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    done_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            } else if let Some(ref source_path) = tab.table.source_path.clone() {
                let path = std::path::PathBuf::from(source_path);
                let format_name = tab.table.format_name.clone().unwrap_or_default();
                let num_cols = tab.table.col_count();
                let csv_delimiter = tab.csv_delimiter;

                if format_name == "Parquet" {
                    std::thread::spawn(move || {
                        // Bad bytes at row two million, or a USB stick pulled
                        // mid-scroll, must not leave the app repainting every
                        // frame with a spinner that never stops.
                        let _done =
                            crate::app::flag_guard::FlagOnDrop::new(done_flag.clone(), true);
                        if let Err(e) = load_remaining_parquet_rows(
                            &path,
                            skip_rows,
                            max_chunk,
                            buffer.clone(),
                            done_flag,
                            exhausted_flag,
                        ) {
                            eprintln!("Background loading error: {}", e);
                        }
                    });
                } else if format_name == "CSV" || format_name == "TSV" {
                    let delimiter = if format_name == "TSV" {
                        b'\t'
                    } else {
                        csv_delimiter
                    };
                    std::thread::spawn(move || {
                        let _done =
                            crate::app::flag_guard::FlagOnDrop::new(done_flag.clone(), true);
                        if let Err(e) = formats::csv_reader::load_csv_rows_chunk(
                            &path,
                            delimiter,
                            formats::csv_reader::ChunkSpec {
                                skip_rows,
                                max_rows: max_chunk,
                                num_cols,
                            },
                            formats::csv_reader::ChunkSink {
                                buffer,
                                done: done_flag,
                                exhausted: exhausted_flag,
                            },
                        ) {
                            eprintln!("Background CSV loading error: {}", e);
                        }
                    });
                }
            }
        }
    }
}

/// Center the easter-egg ASCII art for an empty file. Picks the accent color
/// from the active theme so the art doesn't fight the theme palette.
fn render_empty_file_placeholder(ui: &mut egui::Ui, theme_mode: ui::theme::ThemeMode) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(super::easter_eggs::EMPTY_FILE_ART)
                .monospace()
                .size(14.0)
                .color(colors.accent),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(super::easter_eggs::EMPTY_FILE_TAGLINE)
                .italics()
                .size(13.0)
                .color(colors.text_secondary),
        );
    });
}

/// A confirmation, as opposed to a failure. Only the prefix Octa itself
/// writes is recognised; anything else is treated as a failure, which is the
/// safe way round (a failure shown for too long beats one that vanishes).
fn is_success_message(msg: &str) -> bool {
    msg.starts_with("Saved")
}

/// How long a status message stays on screen.
///
#[cfg(test)]
mod status_message_tests {
    use super::*;
    use octa::ui::settings::{AppSettings, MIN_STATUS_MESSAGE_SECS};

    /// Every message now gets the same span. Failures used to get 60s against
    /// 10s for confirmations so an error could be read and copied; that is the
    /// hover pause's job now, and the split is gone deliberately. This test
    /// exists so bringing it back is a conscious act, not a quiet regression.
    #[test]
    fn every_message_gets_the_same_lifetime() {
        let s = AppSettings::default();
        assert_eq!(s.status_message_duration().as_secs(), 10);
    }

    /// The setting has no "off": a message that never expires covers the
    /// status bar until restart.
    #[test]
    fn the_timeout_cannot_be_disabled() {
        for secs in [0, 1, 2] {
            let s = AppSettings {
                status_message_secs: secs,
                ..Default::default()
            };
            assert_eq!(
                s.status_message_duration().as_secs(),
                MIN_STATUS_MESSAGE_SECS,
                "{secs}s should be floored"
            );
        }
    }

    /// A longer value is honoured as written.
    #[test]
    fn a_longer_timeout_is_kept() {
        let s = AppSettings {
            status_message_secs: 45,
            ..Default::default()
        };
        assert_eq!(s.status_message_duration().as_secs(), 45);
    }

    /// The colour still depends on the message; only the lifetime is uniform.
    #[test]
    fn colour_classification_is_unchanged() {
        assert!(is_success_message("Saved data.csv"));
        assert!(!is_success_message("Could not save"));
    }
}
