# Pivot / Unpivot

Reshape a table between **long** and **wide** form, the way a
spreadsheet pivot table does. Open it via
**Analyse -> Pivot / Unpivot...**.

The result always opens in a **new detached tab**, so your original
table is never modified. It runs on the table as you currently see it,
including any unsaved edits.

<!-- SCREENSHOT: pivot-dialog.png: The Pivot / Unpivot dialog in Pivot mode, showing the Pivot/Unpivot toggle, a "Spread column" dropdown, an aggregate dropdown with a value-column dropdown, and a group-by column checklist. -->
![Pivot dialog](../assets/screenshots/pivot-dialog.png){ .screenshot-placeholder }

## Pivot (long to wide)

Pivot spreads one column's distinct values into new columns:

| Field             | Meaning                                                                              |
|-------------------|--------------------------------------------------------------------------------------|
| **Spread column** | The column whose values become the new column headers (e.g. `month`).                |
| **Aggregate**     | How to combine values landing in each new cell: `sum`, `count`, `avg`, `min`, `max`. |
| **of**            | The value column being aggregated (e.g. `sales`).                                    |
| **Group by**      | The identity columns kept as rows (e.g. `region`). Empty = every remaining column.   |

**Example.** Spread `month`, aggregate `sum` of `sales`, group by
`region` turns a long sales log into a region-by-month grid of totals.

## Unpivot (wide to long)

Unpivot is the reverse: it melts several columns into a name/value
pair. Pick the **columns to unpivot** (at least two), then name the
generated **name column** and **value column**.

**Example.** A wide `region, jan, feb, mar` table becomes a long
`region, name, value` table with one row per region-month.

## How it works

Both modes build a DuckDB
[`PIVOT` / `UNPIVOT`](https://duckdb.org/docs/sql/statements/pivot)
statement and run it against the active table (exposed to DuckDB as
the table `data`, the same as the [SQL panel](sql.md)). Because the
output is a detached tab with no source path, **Save As** prompts for
a new file and the original is safe.

## See also

- [SQL Panel](sql.md) for arbitrary queries, including hand-written
  `PIVOT` statements with multiple aggregates.
- [Summary](summary.md) for per-column statistics.
