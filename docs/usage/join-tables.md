# Join Tables

**Data > Join tables...** (Ctrl+Shift+Q) matches rows between two open
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

Then pick the join type (hover a type in the list for what it keeps):

- **Inner** - keep only rows that match.
- **Left** - keep every row of the left table, filling unmatched right
  columns with empty cells.
- **Right** - keep every row of the right table.
- **Full** - keep every row of both.
- **Semi** - keep the left rows that have a partner, with the left
  table's columns only. A left row with three partners still appears
  once. "Which customers have ordered?"
- **Anti** - keep the left rows that have **no** partner, left columns
  only. "Which customers have never ordered?"
- **As-of** - give every left row the **nearest** right row instead of
  an equal one. See below.

The matched result opens in a new tab. Joins run through DuckDB, so they are
fast even on large tables.

## As-of join: the nearest row, not an equal one

A trade at 09:03:30 has no quote at exactly that second. What you want
is the last quote **before** it. That is an as-of join:

- Add the `=` conditions that must match exactly, for example
  `ticker = ticker`.
- Add **exactly one** `>=` or `<=` condition for the column to search
  along, usually a time or a date: `time >= time` takes the nearest
  **earlier** right row, `time <= time` the nearest **later** one.
- Pick **As-of**.

Every left row stays. One with no earlier row (a trade before the first
quote, a ticker without quotes) keeps empty right columns. Dates and
datetimes are compared as dates, never as text. With no inequality, or
more than one, the dialog says so instead of guessing.

`samples/features/trades.csv` and `quotes.csv` in the repository are
built for trying it.

## Command line and assistant

The command-line `octa --join` (see the [`--join`](../cli/join.md)
reference) and the [`join_tables`](../mcp/tools/join_tables.md) MCP /
assistant tool join on shared **column names** (`--join-on`) and know
every join type above. For an as-of join there, the **last** `--join-on`
column is the one matched to the nearest earlier value, so put the time
last: `--join-on ticker,time --join-type asof`. The in-app dialog is the
place for different column names, other operators, or a nearest later
row. To stack tables vertically instead, use
[Union Tables](union-tables.md).

To join by location instead (the region each point lies in, or the nearest
store), pick **Spatial** as the join type; see [Spatial join](spatial-join.md).
