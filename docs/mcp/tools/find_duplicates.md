# `find_duplicates`

Find **duplicate rows** in a tabular file. You name the columns whose
combined value forms the duplicate key; every row that shares its key
with at least one other row is returned.

## When to use

- Data-quality checks: "are there duplicate `email` values?"
- Validating a supposed primary key.
- De-duplication planning before a `convert` or downstream load.

## Input schema

| Parameter     | Type     | Required? | Default        | Description                                                                               |
|---------------|----------|-----------|----------------|-------------------------------------------------------------------------------------------|
| `path`        | string   | yes       | (no default)   | Path to the file                                                                          |
| `key_columns` | string[] | yes       | (no default)   | Column names whose combined value is the duplicate key                                    |
| `table`       | string   | no        | (no default)   | Specific table for multi-table sources                                                    |
| `limit`       | integer  | no        | server default | Max duplicate rows to return in the response. `0` = unlimited.                            |
| `unlimited`   | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so duplicate detection scans every row in the file |

Keys are compared on the cells' string representation, so `int(1)` and
`float(1.0)` are **not** treated as equal.

## Response shape

```json
{
  "key_columns": ["email"],
  "duplicate_row_count": 4,
  "result": {
    "schema": [ { "name": "id", "type": "Int64" }, … ],
    "rows": [ [ … ], … ],
    "row_count": 4,
    "truncated": false,
    "total_rows_available": 4,
    "cell_truncated": false
  }
}
```

`duplicate_row_count` is the total number of duplicate rows found;
`result` carries the rows themselves (subject to the row/cell caps —
see [Limits & truncation](../limits-and-truncation.md)).

## Example call

```json
{
  "name": "find_duplicates",
  "arguments": {
    "path": "/tmp/contacts.csv",
    "key_columns": ["first_name", "last_name"]
  }
}
```

## See also

- [`value_frequency`](value_frequency.md) — counts of every value, not
  just the duplicated ones.
- [`run_sql`](run_sql.md) — `GROUP BY … HAVING count(*) > 1` for custom
  duplicate logic.
