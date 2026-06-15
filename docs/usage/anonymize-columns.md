# Anonymise Columns

**Edit > Anonymise columns...** (Ctrl+Shift+Y) prepares a file for sharing by
masking or scrambling sensitive columns. Add one or more rules, pick a strategy
for each, choose where the result goes, and press Apply. An Apply is a single
undo step (Ctrl+Z reverts the whole operation at once).

## Rules

Each rule targets one or more columns and applies a strategy:

| Strategy | What it does | Example |
| --- | --- | --- |
| Hash | A stable code derived from the value. The same value always gives the same code, so the data stays join-able. | `a@x.com` -> `9f86d081a1b2...` |
| Partial mask | Keep the first or last N characters, replace the rest. | `5551234` -> `***1234` |
| Redact | Replace the whole value with a fixed token or an empty (null) cell. | `secret` -> `[REDACTED]` |
| Fake | A realistic synthetic value (name, email, city, company, phone, UUID). Deterministic, so duplicates stay consistent. | `Alice` -> `Jordan Lee` |

Picking several columns in one **mask / redact / fake** rule applies the same
strategy to each of them.

## Hashing: SHA-256 vs BLAKE3

Both produce a 256-bit digest written as 64 hexadecimal characters.

- **SHA-256** is the widely known industry-standard hash.
- **BLAKE3** is a modern hash that is much faster on large files.

For anonymisation either is fine and the result is equally join-able. Pick
SHA-256 for familiarity, BLAKE3 for speed on big tables. The choice does not
change how the output behaves.

### Full vs shortened hash

By default Octa writes the **full** 64-character hash. Turn off "Output full
hash" to keep only the first N characters as a shorter, tidier ID. The shorter
you make it, the higher the (still small) chance that two different values end
up with the same code, so keep more characters when the column has many distinct
values.

## Salt

The optional **salt** is mixed into every value before hashing. The same value
plus the same salt always gives the same result, so:

- duplicate values stay linked (the data is still join-able), and
- a later re-run with the same salt re-joins to the earlier export.

A non-empty salt makes the output non-guessable (it defeats lookup tables on
low-cardinality columns like emails). Null and empty cells always pass through
unchanged.

## Combining columns into one ID

Select **several** columns in a single **Hash** rule to hash them together into
one new column (a pseudonymous key). For example, hashing `first` + `last` into
`person_id` gives every person a stable code without exposing their name. A
multi-column hash always creates a new column (it never overwrites the sources);
set its name in the rule.

## Output

Choose where the result goes:

- **Replace the columns in place** - overwrite the chosen columns.
- **Add the result as new columns (keep originals)** - append the anonymised
  values as new columns (e.g. `email` stays, `email_anon` is added).
- **Put a sanitised copy in a new tab** - leave the original untouched and open
  a clean copy.

## Command line and assistant

The same engine is available as `octa --anonymize spec.json data.csv` (see the
[`--anonymize`](../cli/anonymize.md) reference) and as the
[`anonymize`](../mcp/tools/anonymize.md) MCP / assistant tool.
