//! Tab groups: a named, coloured run of contiguous tabs that can be folded.
//! The model is pure; `OctaApp::normalise_groups` applies it to the tab list.

use octa::data::MarkColor;

use crate::app::dialogs::tab_group::TabGroupDialog;
use crate::app::state::{OctaApp, TabState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct GroupId(pub u64);

impl GroupId {
    pub(crate) fn fresh() -> GroupId {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        GroupId(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TabGroup {
    pub id: GroupId,
    pub name: String,
    pub colour: MarkColor,
    pub folded: bool,
}

/// New positions so every group's members sit together where its first
/// member was, all else in its old order. `order[k]` = old index at new k.
pub(crate) fn contiguous_order(groups: &[Option<GroupId>]) -> Vec<usize> {
    let mut done = vec![false; groups.len()];
    let mut order = Vec::with_capacity(groups.len());
    for i in 0..groups.len() {
        if done[i] {
            continue;
        }
        done[i] = true;
        order.push(i);
        if let Some(g) = groups[i] {
            for j in i + 1..groups.len() {
                if !done[j] && groups[j] == Some(g) {
                    done[j] = true;
                    order.push(j);
                }
            }
        }
    }
    order
}

/// The group a tab dropped between `left` and `right` belongs to. A member
/// stays in its group while a neighbour is in it; otherwise only a tab
/// between two members of one group joins.
pub(crate) fn group_after_drop(
    own: Option<GroupId>,
    left: Option<GroupId>,
    right: Option<GroupId>,
) -> Option<GroupId> {
    if own.is_some() && (left == own || right == own) {
        own
    } else if left.is_some() && left == right {
        left
    } else {
        None
    }
}

/// Where a dragged group block really lands. `others` holds the groups of
/// the tabs outside the block and `to` is the slot among them; a slot
/// strictly inside another group moves to just after it.
pub(crate) fn snap_past_group(others: &[Option<GroupId>], to: usize) -> usize {
    let inside =
        to > 0 && to < others.len() && others[to - 1].is_some() && others[to - 1] == others[to];
    if !inside {
        return to;
    }
    let g = others[to];
    (to..others.len())
        .find(|&i| others[i] != g)
        .unwrap_or(others.len())
}

/// The first mark colour no group uses, else the palette cycled by count.
pub(crate) fn default_group_colour(existing: &[TabGroup]) -> MarkColor {
    MarkColor::ALL
        .iter()
        .copied()
        .find(|c| !existing.iter().any(|g| g.colour == *c))
        .unwrap_or(MarkColor::ALL[existing.len() % MarkColor::ALL.len()])
}

/// `Group N` with the smallest N not yet taken.
pub(crate) fn next_group_name(existing: &[TabGroup], word: &str) -> String {
    (1..)
        .map(|n| format!("{word} {n}"))
        .find(|name| !existing.iter().any(|g| g.name == *name))
        .expect("an unbounded range always yields a free name")
}

impl OctaApp {
    /// Re-establish the contiguity invariant after a move, open or close, and
    /// drop groups that lost their last member.
    pub(crate) fn normalise_groups(&mut self) {
        let of_tabs: Vec<_> = self.tabs.iter().map(|t: &TabState| t.group).collect();
        let order = contiguous_order(&of_tabs);
        if order.iter().enumerate().any(|(k, &i)| k != i) {
            self.reorder_tabs(&order);
        }
        let tabs = &self.tabs;
        self.tab_groups
            .retain(|g| tabs.iter().any(|t| t.group == Some(g.id)));
    }

    /// Drop tab `from` at slot `to`: it keeps its group while a neighbour is
    /// in it, joins one it lands inside, else ends up ungrouped. Dropping in
    /// place changes nothing.
    pub(crate) fn drop_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() {
            return;
        }
        let own = self.tabs[from].group;
        self.move_tab(from, to);
        let left = to.checked_sub(1).and_then(|i| self.tabs[i].group);
        let right = self.tabs.get(to + 1).and_then(|t| t.group);
        self.tabs[to].group = group_after_drop(own, left, right);
        self.normalise_groups();
    }

    /// Drop tab `from` onto tab `target`, as Vivaldi stacks tabs: it joins the
    /// target's group right after it, and on a loose tab the two start a new
    /// group with the next free name and colour.
    pub(crate) fn drop_onto_tab(&mut self, from: usize, target: usize) {
        if from == target || from >= self.tabs.len() || target >= self.tabs.len() {
            return;
        }
        let g = self.tabs[target].group.unwrap_or_else(|| {
            let name = next_group_name(&self.tab_groups, &octa::i18n::t("tabgroup.word"));
            let colour = default_group_colour(&self.tab_groups);
            self.create_group(name, colour, &[target])
        });
        let to = if from < target { target } else { target + 1 };
        self.move_tab(from, to);
        self.tabs[to].group = Some(g);
        self.normalise_groups();
    }

    /// Drop the whole block of group `g` at slot `to` among the other tabs.
    pub(crate) fn drop_group(&mut self, g: GroupId, to: usize) {
        let block = self.group_members(g);
        let mut order: Vec<usize> = (0..self.tabs.len())
            .filter(|i| !block.contains(i))
            .collect();
        order.splice(to.min(order.len())..to.min(order.len()), block);
        self.reorder_tabs(&order);
        self.normalise_groups();
    }

    /// The tabs in group `g`, in tab-bar order.
    pub(crate) fn group_members(&self, g: GroupId) -> Vec<usize> {
        (0..self.tabs.len())
            .filter(|&i| self.tabs[i].group == Some(g))
            .collect()
    }

    /// The tabs "New group..." on tab `idx` groups: the multi-selection when
    /// it holds `idx`, else `idx` alone.
    pub(crate) fn group_candidates(&self, idx: usize) -> Vec<usize> {
        if !self.tab_multi_selection.contains(&idx) {
            return vec![idx];
        }
        let mut members: Vec<usize> = self.tab_multi_selection.iter().copied().collect();
        members.sort_unstable();
        members
    }

    /// Open the New group dialog for `members`, with the next free name and
    /// the next colour of the palette.
    pub(crate) fn open_new_group(&mut self, members: Vec<usize>) {
        self.tab_group_dialog = Some(TabGroupDialog {
            editing: None,
            members,
            name: next_group_name(&self.tab_groups, &octa::i18n::t("tabgroup.word")),
            colour: default_group_colour(&self.tab_groups),
        });
    }

    /// Open the same dialog to rename and recolour group `g`.
    pub(crate) fn open_edit_group(&mut self, g: GroupId) {
        let Some(group) = self.tab_groups.iter().find(|x| x.id == g) else {
            return;
        };
        self.tab_group_dialog = Some(TabGroupDialog {
            editing: Some(g),
            members: Vec::new(),
            name: group.name.clone(),
            colour: group.colour,
        });
    }

    /// A new group of `members`, gathered where the first of them is.
    pub(crate) fn create_group(
        &mut self,
        name: String,
        colour: MarkColor,
        members: &[usize],
    ) -> GroupId {
        let id = GroupId::fresh();
        self.tab_groups.push(TabGroup {
            id,
            name,
            colour,
            folded: false,
        });
        for &i in members {
            if let Some(t) = self.tabs.get_mut(i) {
                t.group = Some(id);
            }
        }
        self.normalise_groups();
        id
    }

    /// Move `tab` to the end of group `g` and make it a member.
    pub(crate) fn add_to_group(&mut self, tab: usize, g: GroupId) {
        let Some(&last) = self.group_members(g).last() else {
            return;
        };
        if tab >= self.tabs.len() || self.tabs[tab].group == Some(g) {
            return;
        }
        let to = if tab < last { last } else { last + 1 };
        self.move_tab(tab, to);
        self.tabs[to].group = Some(g);
        self.normalise_groups();
    }

    /// Take `tab` out of its group; it lands right after the group.
    pub(crate) fn remove_from_group(&mut self, tab: usize) {
        let Some(g) = self.tabs.get(tab).and_then(|t| t.group) else {
            return;
        };
        let Some(&last) = self.group_members(g).last() else {
            return;
        };
        self.move_tab(tab, last);
        self.tabs[last].group = None;
        self.normalise_groups();
    }

    /// Dissolve group `g`; its tabs stay where they are.
    pub(crate) fn ungroup(&mut self, g: GroupId) {
        for t in self.tabs.iter_mut().filter(|t| t.group == Some(g)) {
            t.group = None;
        }
        self.normalise_groups();
    }

    /// Close every tab of group `g`: unmodified ones at once, the others
    /// through the unsaved-changes prompt, one after another.
    pub(crate) fn close_group(&mut self, g: GroupId) {
        let members = self.group_members(g);
        self.close_queue = members
            .iter()
            .filter(|&&i| self.tabs[i].is_modified())
            .map(|&i| self.tabs[i].id)
            .collect();
        for &i in members.iter().rev() {
            if !self.tabs[i].is_modified() {
                self.close_tab(i);
            }
        }
        self.next_queued_close();
    }

    /// Once no unsaved-changes prompt is up, raise it for the next tab in
    /// `close_queue` (closing it directly if it was saved meanwhile).
    pub(crate) fn next_queued_close(&mut self) {
        while !self.show_close_confirm && !self.close_queue.is_empty() && !self.close_save_pending()
        {
            let id = self.close_queue.remove(0);
            let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
                continue;
            };
            if self.tabs[i].is_modified() {
                self.pending_close_tab = Some(i);
                self.show_close_confirm = true;
            } else {
                self.close_tab(i);
            }
        }
    }

    /// A save started from the unsaved-changes prompt is still going or
    /// waiting on a question: a Save As question, a write-back confirmation
    /// or write, or a running save job of a queued tab.
    fn close_save_pending(&self) -> bool {
        self.pending_save_as.is_some()
            || self.pending_db_write_back.is_some()
            || self.db_write_back_job.is_some()
            || self
                .tabs
                .iter()
                .any(|t| t.saving.is_some() && self.close_queue.contains(&t.id))
    }

    /// Every tab is in group `g`, so folding it would leave nothing to show.
    pub(crate) fn holds_every_tab(&self, g: GroupId) -> bool {
        self.tabs.iter().all(|t| t.group == Some(g))
    }

    /// Fold or unfold group `g`. Folding while the active tab is a member
    /// moves to the nearest tab outside the group, right first.
    pub(crate) fn toggle_fold(&mut self, g: GroupId) {
        let Some(pos) = self.tab_groups.iter().position(|x| x.id == g) else {
            return;
        };
        let active_inside = self
            .tabs
            .get(self.active_tab)
            .is_some_and(|t| t.group == Some(g));
        if !self.tab_groups[pos].folded && active_inside {
            let outside = |i: &usize| self.tabs[*i].group != Some(g);
            let nearest = (self.active_tab + 1..self.tabs.len())
                .find(outside)
                .or_else(|| (0..self.active_tab).rev().find(outside));
            let Some(next) = nearest else {
                self.status_message = Some((
                    octa::i18n::t("greyed.fold_all_tabs"),
                    std::time::Instant::now(),
                ));
                return;
            };
            self.active_tab = next;
        }
        self.tab_groups[pos].folded = !self.tab_groups[pos].folded;
    }

    /// Unfold the group of the active tab, so it is never hidden (Next tab,
    /// a search hit or a new tab can land on a member of a folded group).
    pub(crate) fn reveal_active_tab(&mut self) {
        let Some(g) = self.tabs.get(self.active_tab).and_then(|t| t.group) else {
            return;
        };
        if let Some(group) = self.tab_groups.iter_mut().find(|x| x.id == g) {
            group.folded = false;
        }
    }
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
