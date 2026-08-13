# Fuzzy join

Two tables describe the same customers, and neither shares a key with the
other. The CRM says `Mueller GmbH`, the sales sheet says `Mueller Gmbh.`,
and [Join tables](join-tables.md) matches neither of them.

Fuzzy join matches rows that are **similar** rather than identical.

## Opening it

**Data → Fuzzy join...**, with at least two tables open. There is no
default keyboard shortcut; one can be assigned under
**Settings → Shortcuts**.

## Setting it up

Pick the **left table**, whose rows are kept, and the **right table**,
which is searched for a partner. Then say which columns to compare: pick
one pair, or several when a single column is not distinctive enough, in
which case their scores are averaged.

### The measure

| Measure          | Good for                                         |
|------------------|--------------------------------------------------|
| **Edit ratio**   | Typos and small misspellings.                    |
| **Jaro-Winkler** | Names, where a shared beginning counts for more. |
| **Token set**    | Reordered or differently punctuated words.       |

These are the same measures
[Find near-duplicates](find-near-duplicates.md) uses within a single
table, not a second implementation of the idea.

Values are compared as normalised text: lowercased, with runs of spaces
collapsed and punctuation dropped. That is what lets ` ACME  Ltd. `
meet `acme ltd`.

### The threshold

How similar two values must be to count as a match, from 0 to 1. `0.85`
is a sensible start: raise it when you get matches you do not believe,
lower it when obvious pairs are being missed.

### Blocking

Without a blocking column every left row is compared with every right
row, which grows with the product of the two row counts and is why a row
cap exists. Naming a column that must match **exactly**, such as a
country or a postcode, restricts the comparison to rows that already
agree there. It changes which pairs are compared, never which of them
match, so it makes a large join possible rather than approximate.

## Reading the result

The result opens in a new tab with the left columns, the right columns,
and two more per step:

- **`match_score_N`** is how similar the matched pair was. Empty when
  the row found no partner.
- **`ambiguous_N`** is true when the **runner-up scored nearly as well**.
  Those are the rows to check by hand: the join picked one, but it was a
  close call.

The status bar reports how many rows matched and how many are ambiguous,
and says so when the row cap was reached.

## More than two tables

**Add another table** joins the result to a third, and so on, folding
left to right. Each step gets its own `match_score` and `ambiguous`
columns rather than one score for the whole chain, because matching
against an already fuzzy result compounds the error, and a single number
would hide where it came from.

## Ceilings

- Each left row keeps **one** partner, the best scorer. A row that
  genuinely matches two right rows is not expressible.
- Values are compared as text. There is no numeric or date tolerance.
- The row cap applies per side and is only reached without a blocking
  column. The status bar says when it bit.
- There is no accept/reject review of individual matches yet. Use
  `ambiguous_N` to find the ones worth a look, and filter or edit the
  result tab.

## Elsewhere in Octa

```bash
octa --fuzzy-join crm.csv --fuzzy-join-file sales.csv \
     --fuzzy-on customer=account
```

See [`octa --fuzzy-join`](../cli/fuzzy-join.md) and the
[`fuzzy_join`](../mcp/tools/fuzzy_join.md) MCP tool, which the in-app
Assistant can also call.

## See also

- [Join tables](join-tables.md), the exact-equality join.
- [Find near-duplicates](find-near-duplicates.md), the same measures
  within one table.
- [Join key finder](join-key-finder.md), for working out which columns
  to compare.
