# Union Tables

**Data > Union tables...** stacks two or more open tabs on
top of each other into one new table, like appending several exports of the
same shape.

## How it works

Tick the tabs to combine. Octa builds a **reconciliation plan**: the result
has the union of all their columns. For each merged column you can keep or
drop it and choose its target type. Columns that appear in only some tables
are filled with empty cells for the rest. Mixed numeric types widen to a
common number type; otherwise the column falls back to text.

Apply opens the combined result in a new tab, leaving the sources
untouched.

## Union files straight from the sidebar

You do not have to open a tab per file first. In the directory sidebar:

1. **Ctrl-click** each file you want (**Shift-click** selects a whole run
   between the last click and this one). Selected rows stay highlighted, and
   a **_N_ selected** bar appears at the top of the sidebar.
2. Click **Union...** in that bar, or right-click any selected file and
   choose **Union selected files...**.

Octa reads the files and opens the same reconciliation plan as above, with
one checkbox per file instead of per tab. This is the quick way to stack a
folder of partitioned exports: forty `part-*.parquet` files become one
table without forty tabs. It is not parquet-specific, and the files need not
even share a format: any mix Octa can read (CSV, JSON, parquet, ...) unions
together, since the columns are reconciled either way.

A plain click still opens a file as before, and clears the selection.
Files that cannot be read are skipped, and the status bar reports how many.

## Union files in the cloud

The same works in the [cloud sidebar](cloud-storage.md). **Ctrl-click** the
objects you want, then click **Union...** in the selection bar that appears
at the top of the cloud section.

Octa downloads the selected objects in the background and then opens the
same reconciliation dialog. A folder of partitioned parquet parts in S3,
Azure Blob or GCS becomes one table without opening a tab per object. As
with local files, a plain click still just opens the object.

## Command line and assistant

Also available as `octa --union` (see the [`--union`](../cli/union.md)
reference) and as the [`union_tables`](../mcp/tools/union_tables.md) MCP /
assistant tool. To match rows side-by-side on a key instead of stacking
them, use [Join Tables](join-tables.md).
