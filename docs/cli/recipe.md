# `--recipe`

Replay a saved recipe (`.ocp`, recorded in the GUI) on a file. Every step
runs by column name. **All or nothing**: when any step cannot run, nothing is
written, the skipped steps are listed on stderr and the exit code is 1.

```sh
octa --recipe monthly.ocp sales_april.csv --recipe-out sales_april_clean.parquet
```

## Flags

| Flag           | Required? | Description                                                               |
|----------------|-----------|---------------------------------------------------------------------------|
| `--recipe`     | yes       | Two values: the RECIPE file and the data FILE.                            |
| `--recipe-out` | no        | Write the result here. Without it, the table goes to stdout in `-f` form. |

## Output

```text
applied 4 step(s); wrote 1200 rows x 6 columns to sales_april_clean.parquet
```

When a step cannot run:

```text
step 1: Renamed Kunde -> customer: skipped, column `Kunde` not found
1 of 4 step(s) could not run; nothing written
```

## Exit codes

| Code | Meaning                                            |
|------|----------------------------------------------------|
| `0`  | Every step ran and the result was written.         |
| `1`  | A step could not run, or a file could not be read. |

See [Recipes](../usage/recipes.md).
