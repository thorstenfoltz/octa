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

## Command line and assistant

Also available as `octa --union` (see the [`--union`](../cli/union.md)
reference) and as the [`union_tables`](../mcp/tools/union_tables.md) MCP /
assistant tool. To match rows side-by-side on a key instead of stacking
them, use [Join Tables](join-tables.md).
