# `--check`

Check a file's values against a rules file, and exit 1 if they fail.

```bash
octa --check orders.parquet --rules quality.toml
octa --check orders.csv --rules quality.toml -f json
```

## The rules file

TOML, one `[[rule]]` table per check:

```toml
[[rule]]
column = "order_id"
kind = "unique"

[[rule]]
column = "amount"
kind = "range"
min = 0

[[rule]]
column = "email"
kind = "regex"
pattern = ".+@.+"

[[rule]]
column = "end"
kind = "compare"
op = ">="
other_column = "start"

[[rule]]
column = "status"
kind = "one_of"
values = ["open", "closed"]

[[rule]]
kind = "not_null"
```

Kinds are `not_null`, `unique`, `range` (with `min` and `max`, either
optional), `regex` (with `pattern`) and `max_length` (with
`max_length`), plus the ID checks `iban`, `card_number`, `gtin`,
`vat_id` and `email` (see
[Data Validation](../usage/data-validation.md#checking-ids-iban-card-numbers-barcodes-vat-email)).
A rule without a `column` applies to every column.

Two more kinds: `compare` (a `column`, an `op` of `=`, `!=`, `<`, `<=`,
`>`, `>=`, and an `other_column`; the cell must compare that way with the
other column in the same row) and `one_of` (a list of `values`; matching
is exact). Both pass empty cells. `compare` needs a `column`; a `one_of`
without one applies to every column, like the other kinds. See
[Data Validation](../usage/data-validation.md#rules-over-two-columns-and-pick-from-values)
for how values compare.

The same file is written and read by the **Data -> Data validation...**
dialog's Save rules and Load rules buttons, so a rule set you built by
clicking can be committed and run in a pipeline.

## Output

One row per failing rule on stdout:

| Column     | Meaning                                         |
|------------|-------------------------------------------------|
| `rule`     | The kind that failed                            |
| `column`   | The column it applies to, empty for all columns |
| `failures` | How many cells failed it                        |
| `samples`  | Up to three of the offending values             |

For a `compare` rule each sample names both values, for example
`end=2024-01-01, start=2024-02-01`.

A rule that could not run at all, because it names a column the file
does not have, appears as its own row with the problem in the `rule`
field. The summary line goes to stderr.

## Exit code

**1** on any violation **and** on any rule that could not run; **0**
only when everything passed. That second half matters: a rules file
whose columns have since been renamed would otherwise report a clean
run over checks it silently skipped.

See [Data Validation](../usage/data-validation.md) for the dialog.

## See also

- MCP: [`check_rules`](../mcp/tools/check_rules.md)
