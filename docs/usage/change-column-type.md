# Change column type

Every reader guesses a column's type when it opens the file, and one
stray value is enough to make it guess "text". A column of prices where
a single row reads `n/a` arrives as text, so it sorts alphabetically and
refuses to sum.

**Change type** fixes that after the file is open. That is deliberate:
load-time problems get fixed once you can see the data, never behind a
gate before it.

## Two doors, one set of rules

- **Right-click a column header, then Change type** is the fast one.
  Pick a type and the column converts on the spot, with no dialog to
  answer. When some values in the column would not convert, it opens the
  dialog below instead of half-converting a column you clicked through
  in a single gesture.
- **Columns -> Change type...** opens that dialog directly, for when you
  want to see what will happen first. The action also has a rebindable
  shortcut, shipped unbound; set one under **Settings > Shortcuts** if
  you use it often.

Both doors run the same conversion, so they cannot disagree about what a
value means.

## What the dialog shows

- The column, and the type it currently has.
- The type to change it to.
- A summary line: how many of the loaded values convert, and how many
  will not.
- The first ten values that will not convert, with their row numbers, so
  you can see what kind of thing is in the way before committing.

On a partly loaded file these counts describe the rows that are loaded,
not the whole file, and the dialog says so.

## Values that will not convert

They keep their original text. They are never blanked, and their
presence never causes the conversion to be refused.

This matters more than it sounds. Octa stores a type per column and a
value per cell, so a column declared as a whole number can still hold
the one cell that reads `unknown`. Blanking it would destroy data;
refusing the whole column would leave the other 999 values unusable.
Keeping it is the only answer that loses nothing.

Every such cell is flagged as a problem cell, so <kbd>F10</kbd> and
<kbd>Shift</kbd>+<kbd>F10</kbd> step through them exactly as they step
through validation failures and detected outliers.

## The types

| Type           | Reads                                                |
|----------------|------------------------------------------------------|
| Text           | anything, so this can never fail                     |
| Whole number   | `42`, `1,234`, `1.234` (see below)                   |
| Decimal number | `3.14`, `3,14`, `1.234,56`                           |
| Boolean        | `true`, `false`, `1`, `0`, `yes`, `no`               |
| Date           | seven layouts, see below                             |
| Date and time  | the same layouts plus a time, with or without a zone |

Numbers go through the same reader as the load-time number pass, so
both the English `1,234.56` and the European `1.234,56` are understood.

Converting to **Text** can never fail, so it is always available as a
way back out of a conversion you did not want.

## Dates

Dates go through the same seven layouts the loader itself uses, and the
layout is chosen for the **whole column**: whichever one reads the most
values wins, and then every value is read that way.

That choice is what makes the feature safe. Reading each value on its
own would let `02.03.2020` in a `DD.MM.YYYY` column be read as March
2nd in one row and February 3rd in the next. Choosing once means a
German column of `18.09.2026` converts cleanly and comes out in the
canonical `2026-09-18` form, instead of being refused for not already
looking like ISO.

Two consequences worth knowing:

- A column that is mostly European with one stray ISO value converts the
  European ones and leaves the ISO one as text. That is the honest
  outcome: the minority value would otherwise be read under a calendar
  convention it was not written in.
- A genuinely ambiguous column, where `01/02/2020` reads equally well as
  DD/MM and MM/DD, is read as DD/MM.

### How the layout is chosen, and the knob that controls it

The vote does not read the whole column. It reads the first 10,000 values,
because deciding the layout means parsing each value under **all seven**
layouts: seven date parses per value, in one go, on the thread that draws the
window. Uncapped on a five-million-row column that is thirty-five million
parses, and the window would stop responding for several seconds on a single
menu click.

The sample never changes a reported count. Whichever layout wins, every loaded
row is then classified under it, so the preview's "converts / will not
convert" numbers always describe the whole loaded table.

**Settings > Performance > Change type > Date layout sample** changes the
number, and a companion **Unlimited** box reads the whole column. Raising it
is worth doing in exactly one situation: when a column's first values are not
representative of the rest. A file sorted so that every ISO-formatted row
comes first, with the European ones after, can elect the minority layout from
a small sample and leave the majority of the column as text. Raising the
sample fixes that, at the cost of the freeze described above.

If you are unsure, leave it alone. 10,000 values settle a vote that is
usually unanimous by value fifty.

## Strict or mixed

By default the conversion is **mixed**: what parses is converted, and what
does not keeps its text. That is what makes the feature useful on a real
column, where one `n/a` should not cost you the other 999 numbers.

Sometimes that is exactly the wrong outcome. A column of "mostly numbers"
still sorts and sums as a number column while quietly leaving the
stragglers out, and you may prefer to know before the column is changed
rather than after.

Tick **Only convert if every value converts** and the conversion is
refused whenever anything would fail. Nothing is changed at all: not the
values, not the column's declared type, and nothing lands on the undo
stack. The dialog stays open and says how many values blocked it, so you
can look at them in the failure list, fix them, or untick the box and
convert the rest anyway.

## Undo

The whole conversion is one entry on the undo stack, not one per cell. A
single <kbd>Ctrl</kbd>+<kbd>Z</kbd> puts the column back exactly as it
was, including the values that kept their text and the column's original
declared type.

## Read-only mode

Changing a column's type is an edit, so it is off in read-only mode. The
menu entry and the Convert button explain why when you hover them.
