# Join Tables

**Analyse > Join tables...** (Ctrl+Shift+Q) matches rows between two open
tabs, like a spreadsheet VLOOKUP or a SQL JOIN. You need a second table open
in another tab first; otherwise Octa shows a reminder in the status bar.

## How it works

Pick the **left** table and the **right** table, then add one or more
**conditions**. Each condition pairs any column of the left table with any
column of the right table through an operator:

`=` equal, `<` less than, `<=` less or equal, `>` greater than, `>=` greater
or equal.

The columns do **not** need the same name, and their **types do not need to
match** - Octa converts both sides to a common type before comparing (numbers
when both are numeric, otherwise text). So you can join a numeric `id`
against a text `ref`, or keep rows where one table's date is `>=` another's.
Add several conditions to require all of them (an AND join).

Then pick the join type:

- **Inner** - keep only rows that match.
- **Left** - keep every row of the left table, filling unmatched right
  columns with empty cells.
- **Right** - keep every row of the right table.
- **Full** - keep every row of both.

The matched result opens in a new tab. Joins run through DuckDB, so they are
fast even on large tables.

## Command line and assistant

The command-line `octa --join` (see the [`--join`](../cli/join.md)
reference) and the [`join_tables`](../mcp/tools/join_tables.md) MCP /
assistant tool join on shared **column names** with equality (`--join-on`).
The in-app dialog is the place for different column names or non-equal
operators. To stack tables vertically instead, use
[Union Tables](union-tables.md).
