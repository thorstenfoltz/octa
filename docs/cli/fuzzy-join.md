# `octa --fuzzy-join`

Join tables on how **similar** their values are rather than on exact
equality. The CRM says `Mueller GmbH`, the sales sheet says
`Mueller Gmbh.`, and [`--join`](join.md) matches neither.

## Synopsis

```bash
octa --fuzzy-join FILE --fuzzy-join-file FILE --fuzzy-on LEFT=RIGHT
     [--fuzzy-method NAME] [--fuzzy-threshold N] [--fuzzy-block LEFT=RIGHT]
     [--fuzzy-join-type TYPE] [--fuzzy-max-rows N] [-f FORMAT]
```

| Flag                       | Required | Default      | Meaning                                              |
|----------------------------|----------|--------------|------------------------------------------------------|
| *FILE*                     | yes      |              | The left table (positional).                         |
| `--fuzzy-join-file PATH`   | yes      |              | A table to join. Repeatable; one join step each.     |
| `--fuzzy-on LEFT=RIGHT`    | yes      |              | Columns to compare. Repeatable; scores are averaged. |
| `--fuzzy-method NAME`      | no       | `edit_ratio` | `edit_ratio`, `jaro_winkler` or `token_set`.         |
| `--fuzzy-threshold N`      | no       | `0.85`       | Minimum average score, `0.0` to `1.0`.               |
| `--fuzzy-block LEFT=RIGHT` | no       |              | Compare only rows agreeing exactly on these columns. |
| `--fuzzy-join-type TYPE`   | no       | `left`       | `inner`, `left`, `right` or `full`.                  |
| `--fuzzy-max-rows N`       | no       | `20000`      | Rows considered per side.                            |

## How matching works

Values are compared as **normalised text**: lowercased, with runs of
whitespace collapsed and punctuation removed, before scoring. That is
what lets ` ACME  Ltd. ` meet `acme ltd`.

Each left row keeps its **single best partner**, the highest scorer at or
above the threshold. Ties go to the first occurrence.

### Measures

| Name           | Good for                                            |
|----------------|-----------------------------------------------------|
| `edit_ratio`   | Typos and small misspellings. `1 - edits / length`. |
| `jaro_winkler` | Names, where a shared prefix counts for more.       |
| `token_set`    | Reordered or differently punctuated words.          |

These are the same measures the
[near-duplicate finder](../usage/find-near-duplicates.md) uses within one
table, not a second implementation.

## Reading the output

Every step adds two columns:

- `match_score_N`, the average similarity of the pair that matched. Empty
  on an unmatched row.
- `ambiguous_N`, true when the **runner-up scored nearly as well** (within
  0.05). Those are the matches to check by hand: the engine picked one,
  but it was close.

The per-step report (rows compared, matched, ambiguous, whether the row
cap was hit) goes to **stderr**, so a pipe stays parseable.

## Blocking

Without `--fuzzy-block` every left row is compared against every right
row, which is quadratic. A blocking column restricts comparison to rows
that agree on it exactly, so a country code, a postcode or a first letter
turns an impossible join into a quick one. It changes **which pairs are
compared**, never which of them match.

## Examples

### Match customers to accounts by name

```bash
octa --fuzzy-join crm.csv --fuzzy-join-file sales.csv \
     --fuzzy-on customer=account
```

### Stricter, on two columns, blocked by country

```bash
octa --fuzzy-join crm.csv --fuzzy-join-file sales.csv \
     --fuzzy-on name=account --fuzzy-on city=town \
     --fuzzy-threshold 0.92 --fuzzy-block country=cc
```

### Keep only the rows that matched

```bash
octa --fuzzy-join crm.csv --fuzzy-join-file sales.csv \
     --fuzzy-on customer=account --fuzzy-join-type inner
```

## Ceilings

- Values are compared as text; there is no numeric or date tolerance.
- One partner per left row. A left row matching two right rows genuinely
  is not expressible.
- `--fuzzy-max-rows` applies per side, and the report says when it bit.
- With several tables the fold runs left to right, so a bad match early
  propagates. That is why the score and flag are recorded per step rather
  than collapsed into one number.
- The CLI applies one set of settings to every step. Use the GUI dialog
  when the steps need different columns or thresholds.

## See also

- [`octa --join`](join.md), the exact-equality join.
- [Find near-duplicates](../usage/find-near-duplicates.md), the same
  measures applied within one table.
- [`octa --unique-columns`](unique-columns.md) and the
  [join key finder](../usage/join-key-finder.md), for working out which
  columns to compare.
- [MCP `fuzzy_join`](../mcp/tools/fuzzy_join.md), the same feature for an
  agent.
