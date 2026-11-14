# Find a command

**Ctrl+K** opens a window that searches every command in Octa, from anywhere:
the table, a dialog, even the SQL editor. Type part of a command's name and
press **Enter** to run it, or click anywhere on its row. It is also
**Search > Find a command...**.

## How matching works

- **Every word you type has to appear, in any order.** `piv` finds **Pivot /
  Unpivot...**, and so do `analyse piv` and `piv analyse`.
- **The menu counts too.** Each command shows the menu it lives in, or its
  shortcut group when it has no menu entry, and you can type that as well.
- **Case, accents and punctuation do not matter.** `uber` finds `Über`.
- **The English name always works**, whatever language Octa is set to, so a
  name you know from the documentation finds the command.
- Names that **start** with what you typed come first.

A feature that lives inside a dialog is listed under its own name too, with the
command that opens it beside it: `working days` finds **Working days between
two dates** in **Date/Time calculation...**, `abc` finds **ABC class** in
**Calculated column...**, and `seasonality` finds **Seasonality to table** in
the chart tab. Picking one opens that dialog.

The commands are named exactly as the menus name them. A command with no menu
entry (Next tab, Jump to first row, Zoom in) has its own name, the same one
**Settings > Shortcuts** shows.

## Keys

| Key       | What it does                    |
|-----------|---------------------------------|
| Up / Down | Move through the list.          |
| Enter     | Run the highlighted command.    |
| Escape    | Close without running anything. |

These work while the search field has focus.

## The window

It is a dialog like any other: drag it, resize it, and use the buttons in its
title bar to minimise, maximise or close it. A click elsewhere leaves it open
and hands the keys back to the rest of Octa. Ctrl+K brings it back with what
you typed.

## What runs, and where

A picked command runs in the place you were before you pressed Ctrl+K. Focus
goes back there first, so a command for the SQL editor acts on the editor you
came from. A text field having focus does not hold the command back, as it
would the command's shortcut: you asked for it by name.

A command that cannot run right now is **greyed**, and hovering it says why,
in the same words the menu uses: open a table first, a read-only tab, only for
a database tab, only for a file on disk. It is never hidden, so the list does
not change under you as you switch tabs.

The caret commands of the text editors (Add a caret above or below, a caret at
the end of each line) are not listed: they work inside an editor only.

## Changing the key

Ctrl+K is the default because egui's text fields would otherwise read it as
"delete to the end of the line", and Octa takes the key before they see it.
Rebind it under **Settings > Shortcuts > View > Find a command...**.

## See also

- [Keyboard Shortcuts](../reference/shortcuts.md)
