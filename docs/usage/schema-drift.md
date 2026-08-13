# Schema drift

A folder of data files is supposed to be one table. Schema drift finds
the files where it is not: the part written with `amount` as text, the
one that lost a column, the one whose header is `Amount` rather than
`amount`.

Nothing is read but the columns. Parquet reads its footer and Arrow IPC
its header, so scanning hundreds of files costs very little.

## Opening it

- **File → Schema drift...**, then pick the folder.
- Right-click a folder in the sidebar and choose **Scan schemas...**,
  which opens the same dialog with that folder already filled in.

There is no default keyboard shortcut; one can be assigned under
**Settings → Shortcuts**.

## Options

| Option                          | Default | What it does                                                                    |
|---------------------------------|---------|---------------------------------------------------------------------------------|
| **Include subfolders**          | off     | Walks subfolders too, to a depth of 8. Needed for `year=2024/month=03` layouts. |
| **Ignore upper and lower case** | off     | Treats `Amount` and `amount` as one column.                                     |

Case is compared exactly by default, because to some downstream tools a
renamed-only-in-case column really is a different column.

## Reading the result

The scan opens a **Schema drift** tab, and the status bar summarises it
in a sentence.

Files are **grouped**, not listed. Every file with an identical schema
collapses into one variant, and the variants are ordered largest first,
so the odd file out is visibly the minority. For 500 Parquet parts the
answer is "497 look like this, 3 look like that" rather than 500 rows.

The table has a `status` column, a `column` column, then one column per
variant holding that variant's type for that column, or blank where the
variant has no such column. Rows that need attention sort to the top.

| `status`       | Meaning                                             |
|----------------|-----------------------------------------------------|
| `type varies`  | Every variant has the column, with differing types. |
| `missing in N` | N variants do not have the column at all.           |
| `consistent`   | Same type everywhere.                               |

A file that cannot be read does not stop the scan: it is counted in the
status line as skipped, and the rest are still compared.

## Ceilings

- Local folders only. Cloud prefixes are not scanned.
- Multi-table sources (a workbook, a database file) report their first
  table.
- **Include subfolders** stops at depth 8.

## Elsewhere in Octa

The same engine runs on the command line and over MCP:

```bash
octa --schema-drift data/landing --recursive
```

The CLI form exits `1` when the files disagree, so it can gate a CI
step. See [`octa --schema-drift`](../cli/schema-drift.md) and the
[`schema_drift`](../mcp/tools/schema_drift.md) MCP tool, which the
in-app Assistant can also call.

## See also

- [Union tables](union-tables.md): reconcile differing schemas into one
  table rather than only reporting them.
- [Compare view](view-modes/compare.md): two tables side by side.
- [`octa --validate-schema`](../cli/validate-schema.md): one file
  against an expected JSON Schema.
