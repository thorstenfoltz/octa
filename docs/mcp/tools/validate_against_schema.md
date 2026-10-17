# `validate_against_schema`

Validate a tabular file's column schema against an expected JSON
Schema. Pairs naturally with [`export_schema --target json-schema`](export_schema.md):
lock in a schema once, then validate future arrivals against it.

CLI mirror: [`octa --validate-schema`](../../cli/validate-schema.md).

## Scope

Column-level only: it checks column names + types. Per-row
data validation is not performed; readers already type-check values
as they parse them.

JSON Schema is the only supported input format. SQL DDL
and Pydantic parsing would each need a real parser and are deferred
until requested.

## When to use

- CI / pipeline gate: "did this file's schema drift since the contract
  was locked in?"
- Pre-import check before loading data into a typed database.
- Verifying that two systems agree on a schema by exporting from one
  and validating the other.

## Input schema

| Parameter       | Type   | Required? | Default      | Description                             |
|-----------------|--------|-----------|--------------|-----------------------------------------|
| `path`          | string | yes       | (no default) | Path to the file being validated.       |
| `table`         | string | no        | (no default) | Specific table for multi-table sources. |
| `schema_path`   | string | one-of    | (no default) | Path to a JSON Schema file.             |
| `schema_inline` | string | one-of    | (no default) | Inline JSON Schema text.                |

Exactly one of `schema_path` and `schema_inline` must be supplied.

## Response shape

```json
{
  "matches": false,
  "diff": {
    "identical": false,
    "common": [
      { "name": "id", "type": "Int64" }
    ],
    "only_in_a": [
      { "name": "extra_actual", "type": "Boolean" }
    ],
    "only_in_b": [
      { "name": "missing_expected", "type": "Utf8" }
    ],
    "type_mismatches": [
      { "name": "amount", "a": "Utf8", "b": "Float64" }
    ]
  },
  "unparsed_types": []
}
```

Field meanings:

- `matches` is `true` when the actual schema equals the expected one.
- `diff` is the full `SchemaDiff` shape, identical to what
  [`compare_schemas`](compare_schemas.md) returns.
- `unparsed_types` lists the JSON Schema `type` values the parser could not
  map to an Arrow type. Those columns default to `Utf8` in the
  expected schema; investigate when `matches` is unexpectedly `false`.

## Round-trip closure

Schemas produced by `export_schema --target json-schema` round-trip
through this tool: validating a file against its own exported schema
always returns `matches: true`. Use this as a sanity check.

`Timestamp(...)` columns are the one lossy case: JSON Schema cannot
carry the precision unit or timezone, so a re-parsed schema normalises
to `Timestamp(Microsecond, None)`. The original column will report a
type mismatch unless it was already in that exact form.

## Example call

Pointing at an on-disk schema:

```json
{
  "name": "validate_against_schema",
  "arguments": {
    "path": "/data/sales.parquet",
    "schema_path": "/data/sales.schema.json"
  }
}
```

Inline schema:

```json
{
  "name": "validate_against_schema",
  "arguments": {
    "path": "/data/sales.parquet",
    "schema_inline": "{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"integer\"}}}"
  }
}
```

## See also

- [`octa --validate-schema`](../../cli/validate-schema.md), the CLI
  mirror (exit code 1 on drift, CI-pipeable).
- [`export_schema`](export_schema.md): produce the JSON Schema this
  tool consumes.
- [`compare_schemas`](compare_schemas.md): symmetric two-file diff.
- In the app: [Schema drift](../../usage/schema-drift.md)
