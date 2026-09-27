# `--test-data`

Generate new rows shaped like one or more real tables: the same columns,
numbers and dates drawn from the real spread, fake names, emails, IBANs and
IDs, code-like text in the same shape, and empty cells at the real rate.

```sh
octa --test-data customers.csv --test-data-rows 20 --seed 7
octa --test-data customers.csv orders.csv --test-data-out testdata/
```

Several files are generated **together**: when one points at another (an
`orders.customer_id` pointing at `customers.id`), the child column draws
only from the parent's generated IDs, so the test tables still join.

## Flags

| Flag                            | Required? | Description                                                                              |
|---------------------------------|-----------|------------------------------------------------------------------------------------------|
| `--test-data`                   | yes       | One or more tables to imitate.                                                           |
| `--test-data-rows`              | no        | Rows per generated table. Default: as many as the real table has.                        |
| `--seed`                        | no        | Seed; the same seed gives the same rows. Default `0`.                                    |
| `--test-data-out`               | no        | One input: the output file. Several: an existing folder, written as `<name>_test.<ext>`. |
| `--test-data-rename-categories` | no        | Replace the real values of small category columns with `value_1`, `value_2`, ...         |

Without `--test-data-out`, one table goes to stdout in the `-f` format.
Several inputs without a folder is an error.

## Output

The plan goes to stderr, one line per column:

```text
note: customers.segment keeps its real values (a small category); pass --test-data-rename-categories to replace them
customers.customer_id: running_number
customers.name: fake_name
customers.segment: category
orders.customer_id: link to customers.customer_id
orders.status: category
wrote 1000 rows x 4 columns to testdata/customers_test.csv
```

## Exit codes

| Code | Meaning                                                               |
|------|-----------------------------------------------------------------------|
| `0`  | Generated and written.                                                |
| `1`  | A file could not be read or written, or several inputs had no folder. |

See [Test Data](../usage/test-data.md).
