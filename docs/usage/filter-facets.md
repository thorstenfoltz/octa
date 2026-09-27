# Filter by value

Every column header carries a small **funnel**, just to the left of its
sort arrows. Click it and a popup lists that column's most common values
with their counts, each with a checkbox: tick the ones to keep, press
**Apply**. Clicking the funnel again closes the popup.

It is the fast way to answer "show me just the rows where this column
says X", without opening a dialog or typing the value out.

## The same filter, a faster door

This writes exactly the same per-column allow-set as the
[Column Filter](search-and-filter.md) dialog. It is not a second kind of
filter and there is no second code path: the filter chips, the filtered
row count, the status bar, the sequential row numbers and every export of
the filtered view all behave as they always did.

**Right-click a column header → Filter values...** opens this same popup,
so the funnel and the right-click give one filter. The window version,
**Columns → Filter by value or shape...**, has the same Values / Shapes
switch plus a column picker and a **Find** field: reach for it when you
want more room or a column that is scrolled out of view.

## What the list shows

The **50 most common values**, ordered by count. When the column holds
more distinct values than that, the popup adds a search box and a line
saying how many are not shown.

**Searching queries the whole column, not the 50 on screen.** A value
that ranks nine hundredth is still findable by typing part of it. That
full scan is exactly why the box appears only when it is needed: opening
the popup on an ordinary column stays cheap, and you pay for the deep
search only when you ask for one.

## All, None, Apply, Clear

- **All** and **None** tick or untick everything *currently listed*, so
  they follow the search box rather than ignoring it. Searching for
  `2024` and pressing All ticks the matching values and leaves the rest
  alone.
- **Apply** keeps only the ticked values.
- **Clear** removes this column's filter and shows every row again.
- **Cancel** closes the popup without touching the filter.

Two selections deliberately mean "no filter" rather than a filter:

- Ticking **everything** would hide nothing, yet the column would still
  show as filtered in the chip row and the header. A filter that does
  nothing but claims to be there is worse than no filter, so it clears.
- Ticking **nothing** would hide every row and leave you looking at an
  empty table with no obvious way back. That clears too.

## Reading the funnel

| Funnel        | Meaning                                                                      |
|---------------|------------------------------------------------------------------------------|
| Grey          | The column has no filter                                                     |
| Accent colour | The column is filtered, the pointer is over the funnel, or its popup is open |

The small accent dot beside the column name means the same thing. It
stays because it is visible on columns too narrow to show the funnel,
and because it is easier to spot when scanning a wide table sideways.

## The chip row

Applying a filter adds a removable chip above the grid, sharing one row
with the Ask-mode comparison chips and the duplicate-filter chip, so
everything narrowing the view is visible in one place. A column keeping
one value reads as `city: Aachen`; a column keeping several reads as a
count, such as `year: 2 values`.

Each chip's `x` removes that column's filter. **Clear all**, which
appears once there is more than one chip, removes every filter in the
row.

## Partly loaded files

On a capped file the counts describe the rows that are **loaded**, not
the whole file, and the popup says so above the list. Reporting a window
count as a file count is the kind of quiet inaccuracy this app works to
avoid, so the note is not optional.

## Shapes instead of values

A **Values / Shapes** switch at the top of the popup swaps the list from
values to shapes: what the values look like with the specifics taken out
(digits become `9`, capitals `A`, other letters `a`). Ticking shapes keeps
every value that has a ticked shape, which is the fast way to spot the
handful of postcodes or IDs typed in the wrong format. See
[Value Shapes](value-shapes.md).

## See also

- [Search and filter](search-and-filter.md) for the Column Filter dialog,
  text search and the predicate filters.
- [Value frequency](value-frequency.md) for the same counts as a full
  tab, with binning for numeric columns and a chart.
- [Value Shapes](value-shapes.md) for the Shapes mode of this funnel.
