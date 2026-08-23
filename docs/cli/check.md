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
kind = "not_null"
```

Kinds are `not_null`, `unique`, `range` (with `min` and `max`, either
optional), `regex` (with `pattern`) and `max_length` (with
`max_length`). A rule without a `column` applies to every column.

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

A rule that could not run at all, because it names a column the file
does not have, appears as its own row with the problem in the `rule`
field. The summary line goes to stderr.

## Exit code

**1** on any violation **and** on any rule that could not run; **0**
only when everything passed. That second half matters: a rules file
whose columns have since been renamed would otherwise report a clean
run over checks it silently skipped.

See [Data Validation](../usage/data-validation.md) for the dialog.
