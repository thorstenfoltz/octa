# `hash_columns`

**Append one column holding a hash of chosen columns** of a tabular file and
write the result. Same engine as **Columns > Hash columns...** in the app and
`octa --hash-columns`.

This is a **write** tool, so it is removed under `--mcp-read-only`.

## When to use

- Building a hash key for a data warehouse load.
- Giving rows a stable fingerprint to compare two extracts.

## Input schema

| Parameter     | Type     | Required? | Default          | Description                                                  |
|---------------|----------|-----------|------------------|--------------------------------------------------------------|
| `path`        | string   | yes       | (no default)     | Path to the source file                                      |
| `columns`     | string[] | yes       | (no default)     | Column names, joined in this order                           |
| `algo`        | string   | no        | `md5`            | `md5`, `sha256` or `sha512`                                  |
| `delimiter`   | string   | no        | `\|`             | Put between the values; may be empty                         |
| `null_text`   | string   | no        | `""`             | Stands in for a NULL cell                                    |
| `trim`        | bool     | no        | `false`          | Strip whitespace from each value first                       |
| `upper`       | bool     | no        | `false`          | Upper-case each value first                                  |
| `new_column`  | string   | no        | `hash_<columns>` | Name of the new column; must not exist yet                   |
| `output_path` | string   | no        | overwrite `path` | Where to write the result; format follows its extension      |
| `unlimited`   | bool     | no        | `false`          | Lift the file-loader row cap so every row is read and hashed |

Every value is turned into text first, whatever its type. The hash is
lowercase hex. A file read only in part (the row cap) is refused rather than
written back with fewer rows; pass `unlimited: true`. Database files (SQLite /
DuckDB / GeoPackage) are not valid sources or targets.

## Output

```json
{ "rows_written": 1200, "new_column": "hash_id_email", "output": "/data/customers.parquet" }
```
