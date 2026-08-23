# Join Diagnostics

You expected 10,000 matched rows and got 12. This tells you why.

Open it from **Analyse -> Join diagnostics...**

## What it does

Pick a table and a key column on each side, then press **Diagnose**.

It compares the two key columns and reports what is actually wrong. It
**changes nothing**: the fixes it lists are advice, not actions.

No language model is involved. Each suggested fix is the same
count of matching keys, recomputed with one normalisation applied, and a
fix is listed only when it strictly beats the current count. It runs
locally and gives the same answer every time.

## What it reports

| Field                        | Meaning                                                           |
|------------------------------|-------------------------------------------------------------------|
| **Rows read**                | How many rows were examined per side (see [Sampling](#sampling)). |
| **Distinct keys**            | How many different key values each side holds.                    |
| **Matching keys**            | How many distinct keys exist on **both** sides right now.         |
| **What would help**          | Normalisations that would raise that number.                      |
| **Only on the left / right** | Up to five real unmatched values per side.                        |

Counts are over **distinct keys, not rows**. A join failing on three
customer IDs is one problem, however many thousands of rows carry them.

## What would help

The usual causes of a join that silently matches nothing:

| Fix                             | Typical cause                                             |
|---------------------------------|-----------------------------------------------------------|
| **Trim spaces from both sides** | A fixed-width export, or a stray space in one system.     |
| **Ignore upper and lower case** | Two systems that disagree on casing.                      |
| **Collapse repeated spaces**    | Hand-typed values, double spaces between words.           |
| **Ignore punctuation**          | `Mueller GmbH.` against `Mueller GmbH`.                   |
| **Ignore leading zeros**        | An ID that went through a spreadsheet and lost its `007`. |

A fix appears **only when it strictly improves** the match count. That
makes an empty list a real answer rather than a shrug: no simple change
helps, and the two columns probably hold genuinely different things.

Nothing is applied for you. Fix the data with
[Transform column](transform-column.md), or at the source, and run the
diagnosis again.

## Sampling

Both sides are read up to a sample limit, 10,000 rows each by default.
When either table is longer the report says so, because the counts are
then partial: a key that only appears in row 50,000 has not been looked
at.

## Handing over to Join

**Use in Join** opens the [Join tables](join-tables.md) dialog with both
tables and columns already filled in. There is one join implementation
in Octa, and this is not it.

## Over MCP

The same diagnosis is the `diagnose_join` tool, so an assistant can run
it before proposing a join. It is read-only and stays available under
`--mcp-read-only`. See [`diagnose_join`](../mcp/tools/diagnose_join.md).

## See also

- [Join key finder](join-key-finder.md) - which columns to join on, when you do not know yet
- [Join tables](join-tables.md) - performing the join
- [Fuzzy join](fuzzy-join.md) - joining on "similar to" rather than "equals"
