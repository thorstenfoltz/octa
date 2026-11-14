# Tiles & Tab Groups

Two ways to tame a crowded tab strip: **tiling** shows several tabs at once,
side by side, stacked or in a grid, and **groups** gather related tabs under a
named, coloured chip that you can fold away.

## Tiling

1. Select two or more tabs with <kbd>Ctrl</kbd>+click (one at a time) or
   <kbd>Shift</kbd>+click (a range).
2. Right-click one of them, or open **View > Tile tabs**, and pick a layout:
   **Side by side**, **Stacked** or **Grid**.

The tabs now share the work area. The focused tile has an outline, and that
tab is the active tab: the menus, toolbar, status bar, SQL panel, sidebars and
the assistant all act on it.

- A click anywhere in another tile, with the left or the right button, focuses
  it.
- The mouse wheel scrolls the tile under the pointer. Keys go to the focused
  tile.
- Drag the line between two tiles to resize them (a tile never gets narrower
  than 120 px). Double-click the line for an even split.
- Right-click a tiled tab for **Layout** (the three layouts again) or
  **Untile**. **View > Untile** does the same for the active tab, and a tiled
  group has **Untile** on its chip.
- A small marker in the tab bar shows which tabs are tiled.

The three layouts differ in how they use the space:

- **Side by side** puts every tab in one row, as columns.
- **Stacked** puts every tab in one column, as rows.
- **Grid** uses rows and columns, as evenly as they fit. Four tabs become two
  rows of two, and six become two rows of three. When the count does not fill
  the grid, the last row keeps a free cell. Two tabs in a grid would look exactly
  like side by side, so **Grid** is greyed until you tile three or more.

Clicking a tab outside the set shows that tab alone; clicking a member of the
set shows the whole set again. You can have several sets, but a tab belongs to
at most one. Closing a member drops it from the set, and a set left with one
tab dissolves.

## Tab groups

Drag a tab onto another tab to group the two, or right-click a tab and choose
**New group...** for a dialog. That group takes the right-clicked tab, or your
<kbd>Ctrl</kbd>+click selection when it contains that tab. Give it a name (the
default is Group 1, Group 2, and so on) and one of twelve colours.

A coloured chip with the group's name sits before its tabs, and each member is
underlined in the same colour. Hovering a member names its group.

- **Click the chip** to fold or unfold the group. Folding while a member is
  active moves to the nearest tab outside the group. Octa refuses to fold when
  every tab is in the group.
- **Right-click the chip** for **Rename and colour...**, **Tile group** (the
  three layouts), **Untile**, **Ungroup** and **Close group**. Close group asks
  about unsaved changes one tab at a time.
- **Right-click a tab** for **Add to group** (every group, plus **New
  group...**) and **Remove from group**.
- **Drag a tab onto another tab** to group them, no dialog needed. Two loose tabs
  start a new group with the next free name and colour, which you can change
  from the chip. Dropped onto a member of a group, the tab joins right after that
  member. Dropped onto a chip, it joins at the end of that group, folded or not.
  What it would land on lights up in the group's colour while you drag.
- **Drag a tab to the edge of another tab** to only move it, with the insertion
  line showing where. Between two members it joins their group, and out of a
  group it leaves. A member dropped next to its own group stays in it.
- **Drag the chip** to move the whole group.

## Reopen last session

With **Settings > Opening files > Reopen last session** on, groups and tile
sets come back after a restart. A member that fails to reopen drops out, and a
group or set left with fewer than two tabs is not restored. See
[Reopen last session](tabs-and-sidebar.md#reopen-last-session).

## Shortcuts

Six actions are available and unbound by default: New tab group, Ungroup tabs,
Tile tabs side by side, Tile tabs stacked, Tile tabs in a grid and Untile tabs.
Bind them in **Settings > Shortcuts**; see the
[shortcut reference](../reference/shortcuts.md).

## See also

- [Tabs & Folder Sidebar](tabs-and-sidebar.md)
