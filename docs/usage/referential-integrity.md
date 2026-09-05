# Referential Integrity

**Analyse > Referential integrity...** answers one question: which child rows
point at a parent that is not there?

A join that silently drops rows is the most expensive kind of wrong, because
the result still looks like a table. This names the values responsible before
you run it.

## Picking the columns

Two rows, each a tab picker and a column picker: the **parent key** and the
**child key** that points at it. Both start on the tab you opened the dialog
from, so a self-reference - a `manager_id` pointing at `id` in the same table -
needs no extra clicks. Changing a tab clears the column beside it, because
index 3 of one table is not index 3 of another.

## Reading the answer

**A clean check opens no tab.** There would be nothing to look at, and the
status bar saying so is the whole answer:

```text
No orphans: all 4,210 child rows with a key match a parent.
```

When there are orphans, they open as their own tab, one row per offending
value, biggest first:

| Column      | Meaning                       |
|-------------|-------------------------------|
| `child`     | The child column checked.     |
| `key_value` | The value with no parent.     |
| `rows`      | How many child rows carry it. |

The counts ride the tab's notice banner, since they do not fit a column. The
list is capped at 500 distinct values; the counts stay exact however many there
are.

## Two conventions worth knowing

- **A missing key is not an orphan.** In every relational database a null
  foreign key means "no parent", not "a parent that vanished", so counting
  those would report every optional relationship as broken. They are counted
  and reported separately. An empty text cell counts as missing too, because
  that is how a CSV writes an absent value.
- **Keys are compared as trimmed text**, the same convention the
  [relationship map](relationship-map.md) and the
  [join key finder](join-key-finder.md) use. That is what makes `1` match `1`
  when one side came from a CSV and the other from a database.

## Elsewhere

The same check is on the command line as
[`--check-references`](../cli/check-references.md), where it **exits 1 when
orphans exist** so it can gate a build, and in the MCP server as
[`check_references`](../mcp/tools/check_references.md).

## See also

- [Join key finder](join-key-finder.md) suggests which columns are a key pair
  in the first place.
- [Relationship map](relationship-map.md) draws the keys it found across a
  folder of files.
- [Join diagnostics](join-diagnostics.md) explains why a join returns too few
  rows when the keys nearly match.
