# `compare_schemas`

Diff the column schemas of two tabular files. Reads only the column
metadata (no row data) and returns the four-way comparison.

This is the MCP counterpart of the [`octa --compare-schemas`](../../cli/compare-schemas.md)
CLI action.

## When to use

- Spotting schema drift between two versions of the same file
  (`v1.parquet` vs. `v2.parquet`).
- Comparing the same table across two databases (e.g. staging vs.
  prod).
- Generating a quick "what changed?" report before a data migration.

For ongoing validation against a fixed expected schema, use the
[`validate_against_schema`](validate_against_schema.md) tool instead.

## Input schema

| Parameter | Type   | Required? | Default      | Description                                     |
|-----------|--------|-----------|--------------|-------------------------------------------------|
| `path_a`  | string | yes       | (no default) | Path to the first file.                         |
| `path_b`  | string | yes       | (no default) | Path to the second file.                        |
| `table_a` | string | no        | (no default) | Specific table on file A (multi-table sources). |
| `table_b` | string | no        | (no default) | Specific table on file B (multi-table sources). |

Column matching is by **exact, case-sensitive name**. `ID` and `id`
are different columns.

## Response shape

```json
{
  "identical": false,
  "common": [
    { "name": "id", "type": "Int64" },
    { "name": "name", "type": "Utf8" }
  ],
  "only_in_a": [
    { "name": "legacy_flag", "type": "Boolean" }
  ],
  "only_in_b": [
    { "name": "region", "type": "Utf8" }
  ],
  "type_mismatches": [
    { "name": "amount", "a": "Float64", "b": "Utf8" }
  ]
}
```

Order in `common` and `type_mismatches` follows the order of side A.
`only_in_b` follows the order of side B.

## Example call

```json
{
  "name": "compare_schemas",
  "arguments": {
    "path_a": "/data/sales_2024.parquet",
    "path_b": "/data/sales_2025.parquet"
  }
}
```

Comparing two tables inside SQLite databases:

```json
{
  "name": "compare_schemas",
  "arguments": {
    "path_a": "/data/staging.sqlite",
    "table_a": "users",
    "path_b": "/data/prod.sqlite",
    "table_b": "users"
  }
}
```

## See also

- [`octa --compare-schemas`](../../cli/compare-schemas.md), the CLI mirror.
- [`validate_against_schema`](validate_against_schema.md) for one-sided
  validation against a fixed expected schema.
- [`export_schema`](export_schema.md) to lock in a schema for later
  comparison.
- Command line: [`octa --compare-schemas`](../../cli/compare-schemas.md)
