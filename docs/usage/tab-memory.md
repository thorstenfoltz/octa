# Tab memory

Octa keeps every open tab's rows in memory. Eleven tabs of parquet parts
is eleven tables, and nothing on screen says which of them is the one
holding two gigabytes.

**Data -> Tab memory...** answers that, and lets you give the memory
back. The action also has a rebindable shortcut, shipped unbound; set one
under **Settings > Shortcuts** if you reach for it often.

## What it shows

One row per open tab: its name, its row count, and an estimated size.
A footer totals the estimates.

The numbers are **estimates and say so**, because an exact figure is not
observable from inside the process. A string may hold more capacity than
its length, the allocator pads and rounds, and a hash map keeps spare
buckets whose count it does not publish. A precise-looking byte total
would be a lie with decimal places on it.

What the estimate is good for is comparison: which tab is holding the
memory, and roughly how much. It counts the cells and their text, the
pending-edit overlay, the colour marks, the column metadata and **both
undo stacks**. That last one matters more than it looks: a column type
conversion snapshots the whole column twice, so a tab that feels idle can
be holding two extra copies of a column.

## On the tab itself

Hovering a tab shows its estimated size and row count under the file path,
so the common question, which tab is holding the memory, is answerable
without opening this dialog at all. The figure is computed only for the tab
you are actually pointing at.

## Unloading a tab

**Unload** drops that tab's rows. The tab itself stays exactly where it
is, keeping its name, its file, its view mode and its view state, and the
file is read again the moment you select the tab. It is a way to reclaim
memory from tabs you are not looking at without losing your place.

Unloading is **refused**, not merely warned about, in two cases, because
both of them lose data:

| Refusal                       | Why                                                                                                                         |
|-------------------------------|-----------------------------------------------------------------------------------------------------------------------------|
| The tab has unsaved changes   | Cell edits and structural changes live only in memory. Dropping the rows would throw them away. Save or discard them first. |
| The tab has no file behind it | A new table, a SQL result, a chart tab or a paste has nothing on disk to read back, so its rows are the only copy.          |

The Unload button is disabled in both cases and says which one applies
when you hover it.

## See also

- [Large files](large-files.md) for the mode that leaves rows on disk in
  the first place.
- [Settings > Performance](../reference/settings.md#performance) for the
  initial-load row cap, which decides how much of a file is read up
  front.
