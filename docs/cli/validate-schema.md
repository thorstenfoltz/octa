# `octa --validate-schema`

Check a file's column schema against an expected JSON Schema. Exit
code is `0` on a clean match and `1` on any drift, so it slots straight
into a CI pipeline.

## Synopsis

```bash
octa --validate-schema FILE --expect-schema SCHEMA_FILE [--table NAME] [-f FORMAT]
```

| Flag                          | Required | Meaning                                           |
|-------------------------------|----------|---------------------------------------------------|
| `--validate-schema FILE`      | yes      | The file whose schema is being validated.         |
| `--expect-schema SCHEMA_FILE` | yes      | Path to the expected JSON Schema.                 |
| `--table NAME`                | no       | Specific table on FILE (multi-table sources).     |
| `-f`, `--format FORMAT`       | no       | Output format: `tsv` (default), `json`, or `csv`. |

## Scope

`--validate-schema` is **column-level only** (names + types). It does
not iterate row values. JSON Schema is the only supported expected
format; produce one with
[`octa --export-schema -t json-schema`](export-schema.md).

## Output

A four-column table with one row per finding. Healthy ("common")
columns are not listed, only the issues, so an empty table on
stdout combined with exit code 0 means the schema matches.

| Column          | Meaning                                                 |
|-----------------|---------------------------------------------------------|
| `status`        | One of `unexpected`, `missing`, `type_mismatch`.        |
| `column`        | Column name.                                            |
| `actual_type`   | Actual type on FILE (empty for `missing`).              |
| `expected_type` | Expected type from the schema (empty for `unexpected`). |

Unrecognised JSON Schema `type` values (anything outside
`integer` / `number` / `boolean` / `string` / `null`) are reported on
**stderr**, defaulted to `Utf8` for the comparison, and won't fail the
validation in isolation.

## Examples

### Lock in a schema, then validate against it

```bash
octa --export-schema sales.parquet -t json-schema > sales.schema.json
octa --validate-schema sales.parquet --expect-schema sales.schema.json
# exit 0, no output → schema matches
```

### Detect drift after the file changes

```bash
$ octa --validate-schema sales_v2.parquet --expect-schema sales.schema.json
status         column     actual_type  expected_type
missing        currency                Utf8
type_mismatch  amount     Utf8         Float64
$ echo $?
1
```

### Plumbed into a CI step

```yaml
- name: Validate input schema
  run: |
    octa --validate-schema data/inputs.parquet \
         --expect-schema data/inputs.schema.json -f json
```

## Round-trip closure

A JSON Schema produced by Octa's exporter is the canonical inverse of
the parser used here. Validating a file against its *own* exported
schema always exits `0`.

The one lossy case is `Timestamp(...)` columns: JSON Schema cannot
carry the precision unit or timezone, so a re-parsed schema
normalises to `Timestamp(Microsecond, None)`. Columns with a different
exact form (e.g. `Timestamp(Nanosecond, Some("UTC"))`) will report a
type mismatch.

## See also

- [`octa --export-schema -t json-schema`](export-schema.md): produce
  the JSON Schema this command consumes.
- [`octa --compare-schemas`](compare-schemas.md): symmetric two-file
  diff, doesn't use exit code for status.
- [MCP `validate_against_schema`](../mcp/tools/validate_against_schema.md):
  same feature over MCP.
- In the app: [Schema drift](../usage/schema-drift.md)
