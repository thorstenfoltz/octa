//! Tiling: tabs shown side by side in the window. A set's members are the
//! tabs whose `tile` names it, in tab-bar order, so moving a tab needs no
//! bookkeeping here. Drawing lives in `central_panel::tiles`.

use octa::i18n::t;
use octa::ui::tiles::TileLayout;

use crate::app::state::OctaApp;
use crate::app::tabs::groups::GroupId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct TileSetId(pub u64);

impl TileSetId {
    pub(crate) fn fresh() -> TileSetId {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        TileSetId(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TileSet {
    pub id: TileSetId,
    pub layout: TileLayout,
    /// Column and row cut fractions, empty = even.
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
}

/// The tile `render_tiles` is drawing right now, for the code it calls.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TileDraw {
    /// It is the active tab: it owns the keyboard.
    pub focused: bool,
    /// The pointer is over it: it owns the wheel.
    pub hovered: bool,
}

/// The tabs "Tile tabs" works on: the Ctrl-click selection plus `extra` (the
/// right-clicked or active tab), each once, in tab-bar order.
pub(crate) fn tile_members(
    selection: &std::collections::HashSet<usize>,
    extra: usize,
) -> Vec<usize> {
    let mut members: Vec<usize> = selection.iter().copied().collect();
    if !selection.contains(&extra) {
        members.push(extra);
    }
    members.sort_unstable();
    members
}

impl OctaApp {
    /// Tile the selection plus `extra` in `layout` and show it. With fewer
    /// than two tabs it only says why not.
    pub(crate) fn tile_selected(&mut self, extra: usize, layout: TileLayout) {
        let members = tile_members(&self.tab_multi_selection, extra);
        if members.len() < 2 {
            self.status_message = Some((t("greyed.need_two_tabs"), std::time::Instant::now()));
            return;
        }
        self.tile_and_show(&members, layout);
        self.tab_multi_selection.clear();
    }

    /// Tile `members` and make sure the set is on screen.
    pub(crate) fn tile_and_show(&mut self, members: &[usize], layout: TileLayout) {
        self.tile_tabs(members, layout);
        if let Some(&first) = members.first()
            && !members.contains(&self.active_tab)
        {
            self.active_tab = first;
        }
    }

    /// Change a set's layout; its dragged cut positions start over.
    pub(crate) fn set_tile_layout(&mut self, set: TileSetId, layout: TileLayout) {
        if let Some(s) = self.tile_sets.iter_mut().find(|s| s.id == set) {
            s.layout = layout;
            s.xs.clear();
            s.ys.clear();
        }
    }

    /// Dissolve the active tab's set, or say it has none.
    pub(crate) fn untile_active(&mut self) {
        match self.tabs[self.active_tab].tile {
            Some(set) => self.untile(set),
            None => {
                self.status_message = Some((t("greyed.not_tiled"), std::time::Instant::now()));
            }
        }
    }

    /// Tile `members` in a new set; a tab leaves any set it was in.
    pub(crate) fn tile_tabs(&mut self, members: &[usize], layout: TileLayout) -> TileSetId {
        let id = TileSetId::fresh();
        for &i in members {
            if let Some(t) = self.tabs.get_mut(i) {
                t.tile = Some(id);
            }
        }
        self.tile_sets.push(TileSet {
            id,
            layout,
            xs: Vec::new(),
            ys: Vec::new(),
        });
        // The sets these tabs left may be down to one.
        self.dissolve_small_tile_sets();
        id
    }

    /// Dissolve `set`; its tabs stay where they are.
    pub(crate) fn untile(&mut self, set: TileSetId) {
        for t in self.tabs.iter_mut().filter(|t| t.tile == Some(set)) {
            t.tile = None;
        }
        self.tile_sets.retain(|s| s.id != set);
    }

    /// Take group `g`'s tabs out of tiling. A set they shared with other tabs
    /// goes on without them.
    pub(crate) fn untile_group(&mut self, g: GroupId) {
        for t in self.tabs.iter_mut().filter(|t| t.group == Some(g)) {
            t.tile = None;
        }
        self.dissolve_small_tile_sets();
    }

    /// Members of the active tab's set in tab-bar order, when it has 2+.
    pub(crate) fn active_tile_members(&self) -> Option<(TileSetId, Vec<usize>)> {
        let id = self.tabs.get(self.active_tab)?.tile?;
        let members: Vec<usize> = (0..self.tabs.len())
            .filter(|&i| self.tabs[i].tile == Some(id))
            .collect();
        (members.len() >= 2 && self.tile_sets.iter().any(|s| s.id == id)).then_some((id, members))
    }

    /// Drop every set left with fewer than two tabs, and its last tab's mark.
    pub(crate) fn dissolve_small_tile_sets(&mut self) {
        let tabs = &self.tabs;
        self.tile_sets
            .retain(|s| tabs.iter().filter(|t| t.tile == Some(s.id)).count() >= 2);
        let sets = &self.tile_sets;
        for t in &mut self.tabs {
            if t.tile.is_some_and(|id| !sets.iter().any(|s| s.id == id)) {
                t.tile = None;
            }
        }
    }

    /// Whether the code running now may take keys and focus: always, except
    /// while drawing a tile that is not the focused one.
    pub(crate) fn owns_keyboard(&self) -> bool {
        self.tile_drawing.is_none_or(|t| t.focused)
    }
}

#[cfg(test)]
#[path = "tiling_tests.rs"]
mod tests;
