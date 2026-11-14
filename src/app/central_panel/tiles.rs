//! Tiled tabs: the central panel cut into one tile per member of the active
//! tab's set. Each tile draws the whole tab body with `active_tab` pointed at
//! its own tab for the duration; only the focused tile (the real active tab)
//! takes the keyboard, and the wheel goes to the tile under the pointer.

use eframe::egui;

use octa::i18n::t;
use octa::ui::theme::ThemeColors;
use octa::ui::tiles::{GAP, MIN_TILE, shape, sizes_along, tile_rects};

use crate::app::state::OctaApp;
use crate::app::tabs::TabId;
use crate::app::tabs::tiling::{TileDraw, TileSetId};

/// Height of the strip naming a tile's tab.
const STRIP_HEIGHT: f32 = 20.0;
/// Grab width of the line between two tiles, a little wider than the gap.
const DIVIDER: f32 = 6.0;

impl OctaApp {
    /// Draw the set `set_id`, whose `members` (tab-bar order) hold the
    /// active tab.
    pub(super) fn render_tiles(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        set_id: TileSetId,
        members: &[usize],
    ) {
        let Some(set) = self.tile_sets.iter().find(|s| s.id == set_id).cloned() else {
            return;
        };
        let area = ui.available_rect_before_wrap();
        let rects = tile_rects(
            area,
            members.len(),
            set.layout,
            &set.xs,
            &set.ys,
            MIN_TILE,
            GAP,
        );
        // By id: a tile's body can open, close or move tabs (a banner button,
        // a result tab), which shifts every index after it.
        let ids: Vec<TabId> = members.iter().map(|&i| self.tabs[i].id).collect();
        let focused = self.tabs[self.active_tab].id;
        let accent = ThemeColors::for_mode(self.theme_mode).accent;
        let mut pressed_in = None;
        let mut opened = None;

        // The focused tile last: the focus a tile before it took is handed
        // back before the focused one draws, so its own text field finds the
        // focus as it left it.
        let order = (0..ids.len())
            .filter(|&k| ids[k] != focused)
            .chain(ids.iter().position(|&id| id == focused));
        for k in order {
            let (id, rect) = (ids[k], rects[k]);
            let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
                continue;
            };
            let is_focused = id == focused;
            // `rect_contains_pointer` also says no while a window or a menu
            // lies over the tile, so a click there focuses nothing.
            let hovered = ui.rect_contains_pointer(rect);
            let pressed = hovered && ctx.input(|i| i.pointer.any_pressed());
            self.tile_drawing = Some(TileDraw {
                focused: is_focused,
                hovered,
            });
            self.active_tab = idx;

            // Keys are the focused tile's: the others draw without the
            // frame's key events, and give back any focus they took unless
            // the press that focuses them landed in them.
            let keys = (!is_focused).then(|| take_key_events(ctx));
            let focus_before = ctx.memory(|m| m.focused());

            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("tile", id)));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            draw_strip(&mut child, rect, &self.tabs[idx].name(), is_focused);
            self.render_tab_body(ctx, &mut child);

            if let Some(keys) = keys {
                ctx.input_mut(|i| i.events.extend(keys));
                if !pressed {
                    give_back_focus(ctx, focus_before);
                }
            }
            if is_focused {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, accent),
                    egui::StrokeKind::Inside,
                );
            }
            if pressed {
                pressed_in = Some(id);
            }
            // The body opened or switched to another tab (a result tab, a
            // forecast): that one is shown next, as outside tiles.
            let now = self.tabs.get(self.active_tab).map(|t| t.id);
            if now != Some(id) && self.tabs.iter().any(|t| t.id == id) {
                opened = now;
            }
        }
        self.tile_drawing = None;
        self.drag_tile_dividers(ui, set_id, area, members.len());
        ui.advance_cursor_after_rect(area);

        let next = opened.or(pressed_in).unwrap_or(focused);
        self.active_tab = self
            .tabs
            .iter()
            .position(|t| t.id == next)
            .unwrap_or(self.active_tab.min(self.tabs.len() - 1));
        // As a click in the tab bar does.
        if self.tabs[self.active_tab].id != focused {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
    }

    /// The lines between the tiles: drag one to resize its neighbours,
    /// double-click it to make that direction even again. A Grid's vertical
    /// lines run the full height, its horizontal ones the full width.
    fn drag_tile_dividers(
        &mut self,
        ui: &mut egui::Ui,
        set_id: TileSetId,
        area: egui::Rect,
        n: usize,
    ) {
        let border = ThemeColors::for_mode(self.theme_mode).border;
        let Some(set) = self.tile_sets.iter_mut().find(|s| s.id == set_id) else {
            return;
        };
        let (cols, rows) = shape(set.layout, n);
        for across in [true, false] {
            let (parts, total, start, fractions) = if across {
                (cols, area.width(), area.left(), &mut set.xs)
            } else {
                (rows, area.height(), area.top(), &mut set.ys)
            };
            let sizes = sizes_along(total, parts, fractions, MIN_TILE, GAP);
            let mut at = start;
            for cut in 0..parts.saturating_sub(1) {
                at += sizes[cut];
                let centre = at + GAP / 2.0;
                at += GAP;
                let (gap, cursor) = if across {
                    (
                        egui::Rect::from_center_size(
                            egui::pos2(centre, area.center().y),
                            egui::vec2(GAP, area.height()),
                        ),
                        egui::CursorIcon::ResizeHorizontal,
                    )
                } else {
                    (
                        egui::Rect::from_center_size(
                            egui::pos2(area.center().x, centre),
                            egui::vec2(area.width(), GAP),
                        ),
                        egui::CursorIcon::ResizeVertical,
                    )
                };
                let grab = if across {
                    gap.expand2(egui::vec2((DIVIDER - GAP) / 2.0, 0.0))
                } else {
                    gap.expand2(egui::vec2(0.0, (DIVIDER - GAP) / 2.0))
                };
                let line = ui
                    .interact(
                        grab,
                        ui.id().with(("tile_divider", set_id, across, cut)),
                        egui::Sense::click_and_drag(),
                    )
                    .on_hover_text(t("tiles.divider_hint"));
                if line.hovered() || line.dragged() {
                    ui.ctx().set_cursor_icon(cursor);
                    ui.painter().rect_filled(gap, 0.0, border);
                }
                if line.double_clicked() {
                    fractions.clear();
                    break;
                }
                if line.dragged()
                    && let Some(p) = line.interact_pointer_pos()
                {
                    let along = if across { p.x } else { p.y };
                    *fractions =
                        moved_cut(&sizes, cut, along - start - cut as f32 * GAP - GAP / 2.0);
                }
            }
        }
    }
}

/// One line naming the tile's tab, on the panel fill; strong when focused.
fn draw_strip(ui: &mut egui::Ui, tile: egui::Rect, name: &str, focused: bool) {
    let strip = egui::Rect::from_min_size(tile.min, egui::vec2(tile.width(), STRIP_HEIGHT));
    let painter = ui.painter_at(strip);
    painter.rect_filled(strip, 0.0, ui.visuals().panel_fill);
    let colour = if focused {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    painter.text(
        strip.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::TextStyle::Body.resolve(ui.style()),
        colour,
    );
    ui.advance_cursor_after_rect(strip);
}

/// Take this frame's keyboard events (keys, typed text, clipboard, IME) out
/// of the input, for a tile that must not see them.
fn take_key_events(ctx: &egui::Context) -> Vec<egui::Event> {
    ctx.input_mut(|i| {
        let (keys, rest) = std::mem::take(&mut i.events).into_iter().partition(|e| {
            matches!(
                e,
                egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Paste(_)
                    | egui::Event::Ime(_)
            )
        });
        i.events = rest;
        keys
    })
}

/// Put the keyboard focus back where it was before a tile drew.
fn give_back_focus(ctx: &egui::Context, before: Option<egui::Id>) {
    ctx.memory_mut(|m| {
        let now = m.focused();
        if now == before {
            return;
        }
        match (before, now) {
            (Some(id), _) => m.request_focus(id),
            (None, Some(id)) => m.surrender_focus(id),
            (None, None) => {}
        }
    });
}

/// Cut fractions with cut `cut` moved to `bound` px along the cells, kept
/// between its neighbours. `sizes` are the cells as drawn; `sizes_along`
/// then keeps every cell its minimum.
fn moved_cut(sizes: &[f32], cut: usize, bound: f32) -> Vec<f32> {
    let usable: f32 = sizes.iter().sum();
    let mut bounds: Vec<f32> = sizes
        .iter()
        .scan(0.0, |acc, s| {
            *acc += s;
            Some(*acc)
        })
        .collect();
    bounds.pop();
    let lo = cut.checked_sub(1).map_or(0.0, |i| bounds[i]);
    let hi = bounds.get(cut + 1).copied().unwrap_or(usable);
    bounds[cut] = bound.clamp(lo, hi);
    bounds.iter().map(|b| b / usable.max(1.0)).collect()
}

#[cfg(test)]
#[path = "tiles_tests.rs"]
mod tests;
