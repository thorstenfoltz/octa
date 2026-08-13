# Join Key Finder

**Analyse → Join key finder...** ranks the column pairs that would
actually join the tables you have open.

Joining two unfamiliar tables normally goes: guess the key columns, run
the join, get zero matched rows, guess again. This removes the guessing
by looking at the values rather than the names.

## What it measures

For every pair of columns across every pair of ticked tables:

- **Overlap** is how much of the smaller set of distinct values appears
  in the larger one. A real key pairing is near 100%.
- **Distinct** is how many distinct values a column has relative to the
  rows sampled. A key is near 100%; a status or flag column is near
  zero.

The ranking multiplies the two, which is what stops a `status` column
that happens to contain the same three words on both sides from
outranking `cust_id → id`. Pairs scoring too low to be worth a look are
not shown at all.

Results read like this:

```text
orders.cust_id -> customers.id    98% overlap, 100% distinct   [Use in Join]
```

## Using it

1. Open the tables you want to join.
2. **Analyse → Join key finder...** The active tab and one other are
   ticked for you; tick more to compare three or more tables, which
   gives you every pairing between them.
3. Adjust **Sample rows per table** if you want (default 10,000; more is
   slower and more certain), then press **Scan**.
4. **Use in Join** opens the ordinary [Join](join-tables.md) dialog with
   that table pair and condition already filled in, where you pick the
   join type and run it.

The finder never joins anything itself. It only tells you which columns
are worth joining on, so there is one join implementation and one place
where join options live.

## Over MCP

`suggest_join_keys` gives an agent the same ranking before it calls
`join_tables`:

```json
{ "paths": ["orders.csv", "customers.csv"] }
```

It returns `candidates` best first, each carrying `left_table`,
`left_column`, `right_table`, `right_column`, `overlap`,
`left_distinct`, `right_distinct` and `score`. It is read-only and stays
available under `--mcp-read-only`.

## Ceilings

- **Sampled**, so 98% overlap is strong evidence, not proof. Raise the
  sample if it matters.
- **Single columns only.** Composite keys (two columns together
  identifying a row) are not suggested; use
  [`--unique-columns`](../cli/unique-columns.md) for that side of the
  question.
- Values are compared as trimmed text, so `1` matches `1` across a
  numeric and a text column. That is usually what you want when the two
  sides came from different systems.

<!-- TODO screenshot: the Join key finder with three tables ticked and a
     ranked result list. Listed in docs/assets/screenshots/INDEX.md. -->
