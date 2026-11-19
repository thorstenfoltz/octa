//! The top tab bar: one label per tab, its right-click menu, the close
//! button, the "+" for a new scratch tab, and a chip before each tab group.

use eframe::egui;
use egui::Color32;

use octa::data::{MarkColor, ViewMode};
use octa::i18n::t;
use octa::ui;
use octa::ui::menu_entry::{disabled_tip, menu_entry, tile_layout_entries, tile_submenu};
use octa::ui::theme::ThemeColors;

use crate::app::sql_panel::DataStamp;
use crate::app::state::{OctaApp, TabState};
use crate::app::tabs::groups::{GroupId, TabGroup, default_group_colour, snap_past_group};
use crate::app::tabs::tiling::{TileSetId, tile_members};
use octa::ui::tiles::TileLayout;

/// Thickness of the tab bar's horizontal scrollbar.
const SCROLL_BAR_WIDTH: f32 = 6.0;
/// Gap between the tab row and the scrollbar under it.
const SCROLL_BAR_INNER_MARGIN: f32 = 2.0;
/// Height the solid scrollbar claims for itself, added to the tab bar panel so
/// the bar sits cleanly beneath the tabs rather than across them.
const SCROLL_BAR_STRIP: f32 = SCROLL_BAR_WIDTH + SCROLL_BAR_INNER_MARGIN;

/// What a drag in the tab bar carries: one tab, or a group's chip (and with
/// it every member of the group).
#[derive(Clone, Copy)]
enum Dragged {
    Tab(usize),
    Chip(GroupId),
}

/// What a dragged tab joins when it lands on it rather than between tabs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DropOnto {
    Tab(usize),
    Chip(GroupId),
}

/// What the user asked for this frame, applied after the tab loop so the loop
/// never mutates the tab list it iterates.
#[derive(Default)]
struct TabBarAction {
    close: Option<usize>,
    activate: Option<usize>,
    /// "Compare with active tab" from a tab's right-click menu.
    compare_with: Option<usize>,
    /// "Pin tab" / "Unpin tab".
    toggle_pin: Option<usize>,
    rename: Option<usize>,
    export_pdf: Option<usize>,
    refresh: Option<usize>,
    load_all: Option<usize>,
    /// Each tab's rect this frame, in tab order. A member of a folded group
    /// is not drawn and gets its chip's rect, so index `i` is always tab `i`.
    rects: Vec<egui::Rect>,
    /// What is being dragged right now.
    dragging: Option<Dragged>,
    /// What was let go this frame.
    drag_stopped: Option<Dragged>,
    /// What was dropped, and the index its (first) tab ends up at.
    drop: Option<(Dragged, usize)>,
    /// Each group's chip this frame.
    chips: Vec<(GroupId, egui::Rect)>,
    /// The tab dropped onto something, joining a group.
    drop_onto: Option<(usize, DropOnto)>,
    /// "New group..." from this tab's menu.
    new_group: Option<usize>,
    add_to_group: Option<(usize, GroupId)>,
    remove_from_group: Option<usize>,
    /// A click on a group's chip.
    toggle_fold: Option<GroupId>,
    /// The chip menu's entries.
    edit_group: Option<GroupId>,
    ungroup: Option<GroupId>,
    close_group: Option<GroupId>,
    /// "Tile tabs" from this tab's menu: the right-clicked tab and the layout.
    tile: Option<(usize, TileLayout)>,
    /// "Layout" on a tiled tab.
    set_layout: Option<(TileSetId, TileLayout)>,
    /// "Untile" on a tiled tab.
    untile: Option<TileSetId>,
    /// "Tile group" from the chip menu.
    tile_group: Option<(GroupId, TileLayout)>,
    /// "Untile" from the chip menu.
    untile_group: Option<GroupId>,
    /// "Colour" from a tab's menu: the tab and its new colour, `None` for
    /// No colour.
    set_colour: Option<(usize, Option<MarkColor>)>,
}

/// Where tabs dragged to `pointer_x` end up: the number of the other tabs
/// (all but `moving`) whose centre lies left of the pointer.
fn drop_index(centres: &[f32], pointer_x: f32, moving: &[usize]) -> usize {
    centres
        .iter()
        .enumerate()
        .filter(|&(i, &c)| !moving.contains(&i) && c < pointer_x)
        .count()
}

/// What tab `from` dragged to `pointer` joins, if anything: the middle half of
/// another tab, or a chip bar its left quarter. Everywhere else it lands between
/// tabs. Its own group is no target, since inside a group a drag only reorders.
fn drop_onto(
    groups: &[Option<GroupId>],
    rects: &[egui::Rect],
    chips: &[(GroupId, egui::Rect)],
    from: usize,
    pointer: egui::Pos2,
) -> Option<DropOnto> {
    let own = groups.get(from).copied().flatten();
    let over = |r: &egui::Rect, right_part: f32| {
        r.y_range().contains(pointer.y)
            && pointer.x >= r.left() + r.width() * 0.25
            && pointer.x <= r.right() - r.width() * right_part
    };
    if let Some((g, _)) = chips.iter().find(|(g, r)| Some(*g) != own && over(r, 0.0)) {
        return Some(DropOnto::Chip(*g));
    }
    (0..rects.len())
        .find(|&i| i != from && (groups[i].is_none() || groups[i] != own) && over(&rects[i], 0.25))
        .map(DropOnto::Tab)
}

/// What the right-click menu needs to know about the tab it belongs to.
struct TabMenuContext {
    idx: usize,
    is_active: bool,
    pinned: bool,
    has_source: bool,
    has_data: bool,
    /// The tab has a file, table, object or endpoint to read again.
    can_refresh: bool,
    /// The source holds rows this tab does not.
    has_more: bool,
    /// The tab's group.
    group: Option<GroupId>,
    /// The tab's own colour.
    colour: Option<MarkColor>,
    /// Every group, for "Add to group".
    groups: Vec<TabGroup>,
    /// How many tabs "Tile tabs" would tile from here.
    tile_count: usize,
    /// The tab's tile set and its layout.
    tiled: Option<(TileSetId, TileLayout)>,
    /// How many tabs that set holds.
    set_size: usize,
}

impl OctaApp {
    /// Render the top tab bar (only shown when at least one file is open).
    pub(crate) fn render_tab_bar(&mut self, parent_ui: &mut egui::Ui) {
        // A tab can leave a set or a group without either noticing (a result
        // tab replacing it), so the marker never shows a set of one and a
        // group never shows split.
        self.dissolve_small_tile_sets();
        self.normalise_groups();
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if !has_open_file {
            return;
        }
        self.reveal_active_tab();
        let ctx = parent_ui.ctx().clone();
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let tab_frame = egui::Frame::new()
            .fill(colors.bg_secondary)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0_f32, colors.border_subtle));
        egui::Panel::top("tab_bar")
            // Tall enough for a tab row (28) plus the scrollbar strip beneath
            // it (`SCROLL_BAR_STRIP`), so the bar never sits on top of the tab
            // labels.
            .exact_size(28.0 + SCROLL_BAR_STRIP)
            .frame(tab_frame)
            .show(parent_ui, |ui| {
                // Scroll the row sideways once the tabs outrun the window,
                // instead of clipping the overflow out of reach. egui only
                // routes a plain (vertical) mouse wheel to a horizontal-only
                // scroll area when `always_scroll_the_only_direction` is set;
                // without it the wheel would need Shift held down here.
                ui.style_mut().always_scroll_the_only_direction = true;
                // egui's default scrollbars *float*: they are drawn on top of
                // the content, which left the bar lying across the tab labels.
                // A solid bar claims its own strip below them instead.
                let mut scroll_style = egui::style::ScrollStyle::solid();
                scroll_style.bar_inner_margin = SCROLL_BAR_INNER_MARGIN;
                scroll_style.bar_outer_margin = 0.0;
                scroll_style.bar_width = SCROLL_BAR_WIDTH;
                ui.style_mut().spacing.scroll = scroll_style;
                egui::ScrollArea::horizontal()
                    .id_salt("tab_bar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let mut action = TabBarAction::default();
                            let mut chip = egui::Rect::NOTHING;
                            for idx in 0..self.tabs.len() {
                                let group = self.tabs[idx]
                                    .group
                                    .and_then(|g| self.tab_groups.iter().find(|x| x.id == g))
                                    .cloned();
                                if let Some(g) = group {
                                    if idx == 0 || self.tabs[idx - 1].group != Some(g.id) {
                                        chip = self.draw_group_chip(ui, &g, &mut action);
                                    }
                                    if g.folded {
                                        action.rects.push(chip);
                                        continue;
                                    }
                                }
                                self.draw_tab(ui, &ctx, idx, &colors, &mut action);
                            }

                            self.finish_tab_drag(ui, &ctx, &colors, &mut action);

                            // "+" button to add new empty tab (opens editor)
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new("+").size(14.0).color(colors.text_muted),
                                ))
                                .clicked()
                            {
                                let mut new_tab = TabState::new(self.settings.default_search_mode);
                                new_tab.view_mode = ViewMode::Raw;
                                new_tab.raw_content = Some(String::new());
                                self.tabs.push(new_tab);
                                action.activate = Some(self.tabs.len() - 1);
                            }

                            self.apply_tab_bar_action(action, &ctx);
                        });
                    });
            });
    }

    /// One tab: its label (with hover and right-click menu) and close button.
    fn draw_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        idx: usize,
        colors: &ui::theme::ThemeColors,
        action: &mut TabBarAction,
    ) {
        let tab = &self.tabs[idx];
        let is_active = idx == self.active_tab;
        let is_multi_selected = self.tab_multi_selection.contains(&idx);
        let full_label = tab.title_display();
        // 📌 prefix marks pinned tabs at a glance. U+1F4CC, supplementary
        // plane - covered by the bundled NotoEmoji font.
        let label = if tab.pinned {
            format!("\u{1f4cc} {}", full_label)
        } else {
            full_label.clone()
        };
        let menu = TabMenuContext {
            idx,
            is_active,
            pinned: tab.pinned,
            has_source: tab.table.source_path.is_some(),
            has_data: tab.table.col_count() > 0,
            can_refresh: tab.refresh_source().is_some(),
            has_more: tab.source_has_more() && !tab.loading_all,
            group: tab.group,
            colour: tab.colour,
            groups: self.tab_groups.clone(),
            tile_count: tile_members(&self.tab_multi_selection, idx).len(),
            tiled: tab.tile.and_then(|id| {
                self.tile_sets
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| (id, s.layout))
            }),
            set_size: self
                .tabs
                .iter()
                .filter(|t| t.tile.is_some() && t.tile == tab.tile)
                .count(),
        };
        let on_screen = self.active_tile_members().map(|(id, _)| id);
        let tile_tint = tab.tile.is_some() && tab.tile != on_screen;
        let group = tab
            .group
            .and_then(|g| self.tab_groups.iter().find(|x| x.id == g));
        let underline = group.map(|g| ThemeColors::mark_swatch(g.colour));
        let member_of = group.map(|g| t("tabgroup.member_of").replace("{group}", &g.name));
        // Hover shows the full path when file-backed, else the full
        // (untruncated) tab title, so a shortened title is always
        // recoverable on hover.
        let hover_path = tab.tab_hint.clone().unwrap_or_else(|| {
            tab.table
                .source_path
                .clone()
                .unwrap_or_else(|| full_label.clone())
        });

        // Distinct visual states: active uses the accent at 30% alpha;
        // Ctrl-click-selected (but not active) uses the accent at 15% so
        // users see which tabs they staged for compare without confusing
        // them with the active one. A tab with a colour of its own fills with
        // it instead: faint while inactive, so the theme's text still reads
        // on it, and like a group chip while active.
        let own = tab.colour;
        let bg = if let Some(c) = own {
            ThemeColors::mark_swatch(c).gamma_multiply(if is_active {
                0.85
            } else if is_multi_selected {
                0.55
            } else {
                0.35
            })
        } else if is_active {
            colors.accent.gamma_multiply(0.3)
        } else if is_multi_selected {
            colors.accent.gamma_multiply(0.15)
        } else if tile_tint {
            colors.accent.gamma_multiply(0.1)
        } else {
            Color32::TRANSPARENT
        };

        let frame = egui::Frame::new()
            .fill(bg)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .corner_radius(4.0);

        let frame_resp = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                if menu.tiled.is_some() {
                    // Two small squares, since the bundled font has no tile glyph.
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 12.0), egui::Sense::hover());
                    for dx in [0.0, 6.0] {
                        let sq = egui::Rect::from_min_size(
                            r.left_center() + egui::vec2(dx, -2.0),
                            egui::vec2(4.0, 4.0),
                        );
                        ui.painter().rect_filled(sq, 1.0, colors.accent);
                    }
                }
                let text_colour = match (own, is_active) {
                    (Some(c), true) => chip_text(c),
                    (Some(_), false) | (None, true) => colors.text_primary,
                    (None, false) => colors.text_secondary,
                };
                let text = egui::RichText::new(&label).color(text_colour);
                let text = if is_active { text.strong() } else { text };
                let tab_label_resp = ui
                    .add(egui::Label::new(text).sense(egui::Sense::click_and_drag()))
                    .on_hover_ui(|ui| {
                        if let Some(m) = &member_of {
                            ui.label(m);
                        }
                        ui.label(&hover_path);
                        // The same figure View -> Tab memory shows, under
                        // the path, so "which tab is holding the memory"
                        // needs no dialog. The estimate samples at most
                        // 10,000 rows, and the hover redraws every frame, so
                        // it is still kept until the table changes.
                        let stamp = DataStamp::of(&tab.table);
                        let id = egui::Id::new(("tab_mem_hover", idx));
                        let cached = ctx.data(|d| d.get_temp::<(DataStamp, u64)>(id));
                        let bytes = match cached {
                            Some((s, bytes)) if s == stamp => bytes,
                            _ => {
                                let bytes = octa::data::tab_memory::estimate_bytes(&tab.table);
                                ctx.data_mut(|d| d.insert_temp(id, (stamp, bytes)));
                                bytes
                            }
                        };
                        ui.weak(
                            octa::i18n::t("tabmem.hover")
                                .replace("{size}", &octa::data::tab_memory::format_estimate(bytes))
                                .replace("{rows}", &tab.table.row_count().to_string()),
                        );
                    });
                if tab_label_resp.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Default);
                }
                tab_label_resp.context_menu(|ui| tab_context_menu(ui, &menu, action));
                if tab_label_resp.dragged() {
                    action.dragging = Some(Dragged::Tab(idx));
                }
                if tab_label_resp.drag_stopped() {
                    action.drag_stopped = Some(Dragged::Tab(idx));
                }
                if tab_label_resp.clicked() {
                    // Ctrl-click toggles multi-selection without changing the
                    // active tab. Plain click activates and clears the staged
                    // selection.
                    let cmd_held = ctx.input(|i| i.modifiers.command);
                    if cmd_held && !is_active {
                        if is_multi_selected {
                            self.tab_multi_selection.remove(&idx);
                        } else {
                            self.tab_multi_selection.insert(idx);
                        }
                    } else {
                        action.activate = Some(idx);
                        self.tab_multi_selection.clear();
                    }
                }
                // Close button (hidden on pinned tabs - the user has to unpin
                // first via the right-click context menu). The leading spacing
                // lives outside the label so the response rect tightly hugs
                // the × glyph - that way the hover overlay (painted at
                // rect.center) sits exactly where egui drew the original glyph
                // and no horizontal shift is visible on hover.
                if !menu.pinned {
                    ui.add_space(6.0);
                    let close_resp = ui.add(
                        egui::Label::new(egui::RichText::new("\u{00D7}").size(14.0).color(
                            match own {
                                Some(c) if is_active => chip_text(c),
                                _ => colors.text_muted,
                            },
                        ))
                        .sense(egui::Sense::click() | egui::Sense::hover()),
                    );
                    if close_resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::Default);
                        let r = close_resp.rect.expand2(egui::vec2(3.0, 1.0));
                        ui.painter()
                            .rect_filled(r, 3.0, colors.accent.gamma_multiply(0.25));
                        ui.painter().text(
                            close_resp.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "\u{00D7}",
                            egui::FontId::proportional(14.0),
                            colors.error,
                        );
                    }
                    if close_resp.clicked() {
                        action.close = Some(idx);
                    }
                }
            });
        });
        let rect = frame_resp.response.rect;
        if let Some(colour) = underline {
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 1.0,
                egui::Stroke::new(2.0, colour),
            );
        }
        action.rects.push(rect);
    }

    /// The rounded chip before a group's tabs: click folds or unfolds,
    /// right-click opens the chip menu, a drag moves the whole group, and a tab
    /// dropped on it joins the group.
    fn draw_group_chip(
        &self,
        ui: &mut egui::Ui,
        g: &TabGroup,
        action: &mut TabBarAction,
    ) -> egui::Rect {
        let text = chip_text(g.colour);
        let refused = !g.folded && self.holds_every_tab(g.id);
        let hint = t("tabgroup.fold_hint");
        let hover = if refused {
            disabled_tip(&hint, &t("greyed.fold_all_tabs"))
        } else {
            hint
        };
        let chip = egui::Frame::new()
            .fill(ThemeColors::mark_swatch(g.colour).gamma_multiply(0.85))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .corner_radius(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&g.name)
                                .text_style(egui::TextStyle::Button)
                                .color(text),
                        )
                        .selectable(false),
                    );
                    ui.add_space(2.0);
                    let arrow = if refused {
                        ui.visuals().weak_text_color()
                    } else {
                        text
                    };
                    fold_arrow(ui, g.folded, arrow);
                });
            });
        let rect = chip.response.rect;
        action.chips.push((g.id, rect));
        // The whole chip answers, its arrow and padding too.
        let resp = ui
            .interact(
                rect,
                ui.id().with(("group_chip", g.id)),
                egui::Sense::click_and_drag(),
            )
            .on_hover_text(hover);
        if resp.clicked() {
            action.toggle_fold = Some(g.id);
        }
        if resp.dragged() {
            action.dragging = Some(Dragged::Chip(g.id));
        }
        if resp.drag_stopped() {
            action.drag_stopped = Some(Dragged::Chip(g.id));
        }
        let members = self.group_members(g.id);
        let tiled = members.iter().any(|&i| self.tabs[i].tile.is_some());
        resp.context_menu(|ui| chip_context_menu(ui, g.id, members.len(), tiled, action));
        rect
    }

    /// With every tab drawn: paint the insertion line while something is
    /// dragged and turn the release into `action.drop`.
    fn finish_tab_drag(
        &self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        colors: &ui::theme::ThemeColors,
        action: &mut TabBarAction,
    ) {
        let Some(what) = action.dragging.or(action.drag_stopped) else {
            return;
        };
        let Some(pointer) = ctx.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        if let Dragged::Tab(from) = what
            && self.finish_drop_onto(ui, from, pointer, action)
        {
            return;
        }
        // A chip moves its whole group as one block.
        let moving = match what {
            Dragged::Tab(i) => vec![i],
            Dragged::Chip(g) => self.group_members(g),
        };
        let centres: Vec<f32> = action.rects.iter().map(|r| r.center().x).collect();
        let mut to = drop_index(&centres, pointer.x, &moving);
        if matches!(what, Dragged::Chip(_)) {
            let others: Vec<_> = (0..self.tabs.len())
                .filter(|i| !moving.contains(i))
                .map(|i| self.tabs[i].group)
                .collect();
            to = snap_past_group(&others, to);
        }
        if action.drag_stopped.is_some() {
            action.drop = Some((what, to));
            return;
        }
        let others: Vec<egui::Rect> = (0..action.rects.len())
            .filter(|i| !moving.contains(i))
            .map(|i| action.rects[i])
            .collect();
        let (x, r) = match (others.get(to), others.last()) {
            (Some(r), _) => (r.left() - 1.0, r),
            (None, Some(r)) => (r.right() + 1.0, r),
            (None, None) => return,
        };
        ui.painter()
            .vline(x, r.y_range(), egui::Stroke::new(2.0, colors.accent));
    }

    /// When tab `from` is over something it would join: light that up in the
    /// colour of the group it would be in, or on release note the drop.
    /// Returns whether it is.
    fn finish_drop_onto(
        &self,
        ui: &egui::Ui,
        from: usize,
        pointer: egui::Pos2,
        action: &mut TabBarAction,
    ) -> bool {
        let groups: Vec<_> = self.tabs.iter().map(|t| t.group).collect();
        let Some(onto) = drop_onto(&groups, &action.rects, &action.chips, from, pointer) else {
            return false;
        };
        if action.drag_stopped.is_some() {
            action.drop_onto = Some((from, onto));
            return true;
        }
        let (rect, group) = match onto {
            DropOnto::Tab(i) => (action.rects[i], groups[i]),
            DropOnto::Chip(g) => (
                action
                    .chips
                    .iter()
                    .find(|(x, _)| *x == g)
                    .map_or(egui::Rect::NOTHING, |(_, r)| *r),
                Some(g),
            ),
        };
        let colour = group
            .and_then(|g| self.tab_groups.iter().find(|x| x.id == g))
            .map_or_else(|| default_group_colour(&self.tab_groups), |x| x.colour);
        let colour = ThemeColors::mark_swatch(colour);
        ui.painter()
            .rect_filled(rect, 4.0, colour.gamma_multiply(0.3));
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, colour),
            egui::StrokeKind::Inside,
        );
        true
    }

    fn apply_tab_bar_action(&mut self, action: TabBarAction, ctx: &egui::Context) {
        if let Some((what, to)) = action.drop {
            match what {
                Dragged::Tab(from) => self.drop_tab(from, to),
                Dragged::Chip(g) => self.drop_group(g, to),
            }
        }
        if let Some((from, onto)) = action.drop_onto {
            match onto {
                DropOnto::Tab(target) => self.drop_onto_tab(from, target),
                DropOnto::Chip(g) => self.add_to_group(from, g),
            }
        }
        if let Some(idx) = action.activate {
            self.active_tab = idx;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        if let Some(idx) = action.close {
            if self.tabs[idx].is_modified() {
                self.pending_close_tab = Some(idx);
                self.show_close_confirm = true;
            } else {
                self.close_tab(idx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
        if let Some(idx) = action.compare_with {
            self.begin_compare_with_tab(idx);
        }
        if let Some(idx) = action.toggle_pin {
            self.toggle_tab_pinned(idx);
        }
        if let Some(idx) = action.rename {
            self.begin_rename_tab(idx);
        }
        if let Some(idx) = action.refresh {
            self.request_refresh(idx, ctx);
        }
        if let Some(idx) = action.load_all {
            self.active_tab = idx;
            self.open_load_all(ctx);
        }
        if let Some(idx) = action.export_pdf {
            // The dialog prints the active tab, so make it the one that was
            // right-clicked.
            self.active_tab = idx;
            self.pdf_export_dialog = Some(crate::app::state::PdfExportState::default());
        }
        if let Some(idx) = action.new_group {
            self.open_new_group(self.group_candidates(idx));
        }
        if let Some((idx, g)) = action.add_to_group {
            self.add_to_group(idx, g);
        }
        if let Some(idx) = action.remove_from_group {
            self.remove_from_group(idx);
        }
        if let Some((idx, layout)) = action.tile {
            self.tile_selected(idx, layout);
        }
        if let Some((set, layout)) = action.set_layout {
            self.set_tile_layout(set, layout);
        }
        if let Some(set) = action.untile {
            self.untile(set);
        }
        if let Some((g, layout)) = action.tile_group {
            let members = self.group_members(g);
            self.tile_and_show(&members, layout);
        }
        if let Some(g) = action.untile_group {
            self.untile_group(g);
        }
        if let Some(g) = action.edit_group {
            self.open_edit_group(g);
        }
        if let Some(g) = action.ungroup {
            self.ungroup(g);
        }
        // Both can change the active tab.
        if let Some(g) = action.toggle_fold {
            self.toggle_fold(g);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        if let Some((idx, colour)) = action.set_colour
            && let Some(tab) = self.tabs.get_mut(idx)
        {
            tab.colour = colour;
        }
        if let Some(g) = action.close_group {
            self.close_group(g);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
    }
}

/// Text that reads on a group's colour.
fn chip_text(colour: MarkColor) -> Color32 {
    if colour.needs_dark_text() {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// The chip's fold arrow: down when open, right when folded. Painted, because
/// the letter it used to be read as part of the group's name.
fn fold_arrow(ui: &mut egui::Ui, folded: bool, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 12.0), egui::Sense::hover());
    let offsets = if folded {
        [(-2.5, -3.5), (2.5, 0.0), (-2.5, 3.5)]
    } else {
        [(-3.5, -2.5), (3.5, -2.5), (0.0, 2.5)]
    };
    let points = offsets
        .map(|(x, y)| rect.center() + egui::vec2(x, y))
        .to_vec();
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        colour,
        egui::Stroke::NONE,
    ));
}

/// Right-click menu of a group's chip. `tiled`: a member of it is tiled.
fn chip_context_menu(
    ui: &mut egui::Ui,
    g: GroupId,
    size: usize,
    tiled: bool,
    action: &mut TabBarAction,
) {
    if menu_entry(ui, &t("tabgroup.rename"), &t("tabgroup.rename_hint"), None) {
        action.edit_group = Some(g);
        ui.close();
    }
    if let Some(l) = tile_submenu(
        ui,
        &t("tabgroup.tile"),
        &t("tabgroup.tile_hint"),
        (size < 2).then(|| t("greyed.group_needs_two")).as_deref(),
        size,
    ) {
        action.tile_group = Some((g, l));
    }
    let not_tiled = (!tiled).then(|| t("greyed.group_not_tiled"));
    if menu_entry(
        ui,
        &t("tiles.untile"),
        &t("tiles.untile_hint"),
        not_tiled.as_deref(),
    ) {
        action.untile_group = Some(g);
        ui.close();
    }
    if menu_entry(
        ui,
        &t("tabgroup.ungroup"),
        &t("tabgroup.ungroup_hint"),
        None,
    ) {
        action.ungroup = Some(g);
        ui.close();
    }
    if menu_entry(ui, &t("tabgroup.close"), &t("tabgroup.close_hint"), None) {
        action.close_group = Some(g);
        ui.close();
    }
}

/// The tab menu's group entries: New group..., Add to group, Remove from group.
fn tab_group_entries(ui: &mut egui::Ui, menu: &TabMenuContext, action: &mut TabBarAction) {
    let idx = menu.idx;
    let new_hint = t("tabgroup.new_group_hint");
    if menu_entry(ui, &t("tabgroup.new_group"), &new_hint, None) {
        action.new_group = Some(idx);
        ui.close();
    }
    let add_hint = t("tabgroup.add_to_group_hint");
    ui.menu_button(t("tabgroup.add_to_group"), |ui| {
        for g in &menu.groups {
            let entry = egui::Button::new(egui::RichText::new(&g.name).color(chip_text(g.colour)))
                .fill(ThemeColors::mark_swatch(g.colour));
            let already = t("tabgroup.member_of").replace("{group}", &g.name);
            if ui
                .add_enabled(menu.group != Some(g.id), entry)
                .on_hover_text(&add_hint)
                .on_disabled_hover_text(disabled_tip(&add_hint, &already))
                .clicked()
            {
                action.add_to_group = Some((idx, g.id));
                ui.close();
            }
        }
        if !menu.groups.is_empty() {
            ui.separator();
        }
        if menu_entry(ui, &t("tabgroup.new_group"), &new_hint, None) {
            action.new_group = Some(idx);
            ui.close();
        }
    })
    .response
    .on_hover_text(&add_hint);
    let not_in = menu.group.is_none().then(|| t("greyed.not_in_group"));
    if menu_entry(
        ui,
        &t("tabgroup.remove"),
        &t("tabgroup.remove_hint"),
        not_in.as_deref(),
    ) {
        action.remove_from_group = Some(idx);
        ui.close();
    }
}

/// The tab menu's Colour submenu: the twelve colours groups use, each on its
/// own colour like the Add to group entries, the current one framed, then
/// No colour. Any tab, grouped or not.
fn tab_colour_entries(ui: &mut egui::Ui, menu: &TabMenuContext, action: &mut TabBarAction) {
    let hint = t("tabcolour.menu_hint");
    ui.menu_button(t("tabcolour.menu"), |ui| {
        for &c in MarkColor::ALL {
            let stroke = if menu.colour == Some(c) {
                egui::Stroke::new(2.0, ui.visuals().strong_text_color())
            } else {
                egui::Stroke::NONE
            };
            let entry = egui::Button::new(egui::RichText::new(c.label_t()).color(chip_text(c)))
                .fill(ThemeColors::mark_swatch(c))
                .stroke(stroke);
            if ui.add(entry).on_hover_text(&hint).clicked() {
                action.set_colour = Some((menu.idx, Some(c)));
                ui.close();
            }
        }
        ui.separator();
        let none = menu.colour.is_none().then(|| t("greyed.tab_no_colour"));
        if menu_entry(
            ui,
            &t("tabcolour.none"),
            &t("tabcolour.none_hint"),
            none.as_deref(),
        ) {
            action.set_colour = Some((menu.idx, None));
            ui.close();
        }
    })
    .response
    .on_hover_text(&hint);
}

/// The tab menu's tiling entries: Tile tabs, and for a tiled tab Layout and Untile.
fn tab_tile_entries(ui: &mut egui::Ui, menu: &TabMenuContext, action: &mut TabBarAction) {
    if let Some(l) = tile_submenu(
        ui,
        &t("tiles.tile"),
        &t("tiles.tile_hint"),
        (menu.tile_count < 2)
            .then(|| t("greyed.need_two_tabs"))
            .as_deref(),
        menu.tile_count,
    ) {
        action.tile = Some((menu.idx, l));
    }
    let why = menu.tiled.is_none().then(|| t("greyed.tab_not_tiled"));
    let current = menu.tiled.map(|(_, l)| l);
    let layout = ui
        .add_enabled_ui(why.is_none(), |ui| {
            ui.menu_button(t("tiles.layout"), |ui| {
                if let Some(l) = tile_layout_entries(ui, current, menu.set_size) {
                    action.set_layout = menu.tiled.map(|(set, _)| (set, l));
                    ui.close();
                }
            })
            .response
        })
        .inner;
    match &why {
        None => layout.on_hover_text(t("tiles.layout_hint")),
        Some(w) => layout.on_disabled_hover_text(disabled_tip(&t("tiles.layout_hint"), w)),
    };
    if menu_entry(
        ui,
        &t("tiles.untile"),
        &t("tiles.untile_hint"),
        why.as_deref(),
    ) {
        action.untile = menu.tiled.map(|(set, _)| set);
        ui.close();
    }
}

/// Right-click menu of one tab. "Compare with active tab" only makes sense on
/// a non-active tab; "Pin tab" / "Unpin tab" applies to any tab but only
/// file-backed ones (scratch tabs have nowhere to persist).
fn tab_context_menu(ui: &mut egui::Ui, menu: &TabMenuContext, action: &mut TabBarAction) {
    let idx = menu.idx;
    if octa::ui::menu_entry::menu_entry(
        ui,
        &octa::i18n::t("context_menu.compare_active"),
        "",
        menu.is_active
            .then(|| octa::i18n::t("greyed.is_active_tab"))
            .as_deref(),
    ) {
        action.compare_with = Some(idx);
        ui.close();
    }
    if ui
        .add_enabled(
            menu.can_refresh,
            egui::Button::new(octa::i18n::t("refresh.menu")),
        )
        .on_hover_text(octa::i18n::t("refresh.menu_hint"))
        .on_disabled_hover_text(octa::i18n::t("refresh.menu_disabled_hint"))
        .clicked()
    {
        action.refresh = Some(idx);
        ui.close();
    }
    if ui
        .add_enabled(
            menu.has_more,
            egui::Button::new(octa::i18n::t("loadall.menu")),
        )
        .on_hover_text(octa::i18n::t("loadall.menu_hint"))
        .on_disabled_hover_text(octa::i18n::t("loadall.menu_disabled_hint"))
        .clicked()
    {
        action.load_all = Some(idx);
        ui.close();
    }
    let pin_label = octa::i18n::t(if menu.pinned {
        "msg.unpin_tab"
    } else {
        "msg.pin_tab"
    });
    // Size the button to the wider of the two labels (plus padding) so the
    // context-menu entry stays the same width whether the tab is pinned or
    // not. Without this the button shrink-wraps "Pin tab" and looks cramped
    // when the user right-clicks a pinned tab.
    let pin_btn = ui.add_enabled(
        menu.has_source,
        egui::Button::new(pin_label).min_size(egui::vec2(140.0, 0.0)),
    );
    let pin_btn = if !menu.has_source {
        pin_btn.on_disabled_hover_text(octa::i18n::t("msg.pin_needs_file"))
    } else {
        pin_btn
    };
    if pin_btn.clicked() {
        action.toggle_pin = Some(idx);
        ui.close();
    }
    if ui
        .button(octa::i18n::t("context_menu.rename_tab"))
        .clicked()
    {
        action.rename = Some(idx);
        ui.close();
    }
    tab_group_entries(ui, menu, action);
    tab_colour_entries(ui, menu, action);
    tab_tile_entries(ui, menu, action);
    // Every result tab (Summary, the Quality report, a compare) is a table,
    // so the PDF export belongs on all of them, not just the file-backed ones.
    if ui
        .add_enabled(
            menu.has_data,
            egui::Button::new(octa::i18n::t("context_menu.export_pdf")),
        )
        .on_hover_text(octa::i18n::t("file_menu.export_pdf_hint"))
        .on_disabled_hover_text(octa::i18n::t("file_menu.export_pdf_hint"))
        .clicked()
    {
        action.export_pdf = Some(idx);
        ui.close();
    }
}

#[cfg(test)]
#[path = "tab_bar_tests.rs"]
mod tests;
