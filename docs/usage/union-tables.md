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

By default column names must match exactly, because to some downstream tools a
renamed-only-in-case column really is a different column. Tick **Ignore upper
and lower case in column names** to merge `Amount` and `amount` into one
column; the first spelling encountered names the result, so the output is
named the way one of the real sources spells it. The same option is
`--union-ignore-case` on the command line and `ignore_case` on the
`union_tables` assistant tool.

Apply opens the combined result in a new tab, leaving the sources
untouched.

## Saving the result back in its own format

When every source shares one format, the result tab remembers it. Save As then
opens pre-filled with a matching name, so unioning forty JSON files and writing
one JSON file back is a single click. With a mixed selection there is no single
answer, so the picker opens with no suggestion and you choose the format.

Nothing is written until you save: Apply only opens a tab.

One limit worth knowing for nested formats: Octa reconciles the *columns* of
each source, so nested JSON comes back flattened, with one column per leaf
(`address.city` rather than a nested object).

## Union files straight from the sidebar

You do not have to open a tab per file first. In the directory sidebar:

1. **Ctrl-click** each file you want (**Shift-click** selects a whole run
   between the last click and this one). Selected rows stay highlighted, and
   a ***N* selected** bar appears at the top of the sidebar.
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

Reading many files takes a moment, so it happens in the background: the window
stays responsive and the status bar shows a spinner with a running count
(`Reading files for union: 12/40`) until the dialog opens.

## Union files in the cloud

The same works in the [cloud sidebar](cloud-storage.md). **Ctrl-click** the
objects you want, then click **Union...** in the selection bar that appears
at the top of the cloud section.

Octa downloads the selected objects in the background and then opens the
same reconciliation dialog. A folder of partitioned parquet parts in S3,
Azure Blob or GCS becomes one table without opening a tab per object. As
with local files, a plain click still just opens the object.

The status bar tracks both stages, so a slow bucket never looks like a freeze:
first `Downloading files to union: 12/40`, then the reading count.

Whole folders go in one action, with no object-by-object ticking: right-click a
folder in the cloud tree and choose **Union tables in this folder...**, or
**Union tables in this folder and subfolders...** for a recursive sweep. Octa
lists the prefix, keeps the objects it can read, and unions those, so a prefix
full of `part-*.parquet` becomes one table without picking the parts by hand.

A folder union reads every file fully into memory, so it stops after **500**
files by default and the status bar reports how many were skipped. Change that
number, or tick **Unlimited**, under **Folder union file cap** in
[Settings > Performance](../reference/settings.md#performance).

## Command line and assistant

Also available as `octa --union` (see the [`--union`](../cli/union.md)
reference) and as the [`union_tables`](../mcp/tools/union_tables.md) MCP /
assistant tool. To match rows side-by-side on a key instead of stacking
them, use [Join Tables](join-tables.md).
