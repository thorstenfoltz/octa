# `schema_drift`

Scan a folder of data files and report which of them disagree about
their columns. Read-only, so it stays available under
`--mcp-read-only`.

CLI mirror: [`octa --schema-drift`](../../cli/schema-drift.md).

## When to use

- A union or a dataset open failed with a type error that named no
  file, and the agent needs to find which part is the odd one out.
- Checking a landing zone before loading it, without reading any rows.
- Answering "are these 500 Parquet parts really one table?".

## Input schema

| Parameter     | Type    | Required? | Default      | Description                                       |
|---------------|---------|-----------|--------------|---------------------------------------------------|
| `path`        | string  | yes       | (no default) | Directory to scan.                                |
| `recursive`   | boolean | no        | `false`      | Walk subdirectories, to a depth of 8.             |
| `ignore_case` | boolean | no        | `false`      | Treat names differing only in case as one column. |

## How files are grouped

Files are grouped by an **exact schema fingerprint**, not compared
pairwise. For 500 parts the useful answer is "497 look like this, 3
look like that", so identical schemas collapse into a single variant
and variants come back largest first, which makes the odd file out the
visible minority.

Only the schema is read, never the rows: Parquet reads its footer and
Arrow IPC its header. Other formats fall back to a full read to get
their columns.

A file that cannot be read lands in `skipped` with a reason rather than
aborting the scan.

## Response shape

```json
{
  "has_drift": true,
  "variants": [
    {
      "files": ["part-00000.parquet", "part-00001.parquet"],
      "file_count": 2,
      "columns": [
        { "name": "id", "type": "Int64" },
        { "name": "amount", "type": "Float64" }
      ]
    },
    {
      "files": ["part-00002.parquet"],
      "file_count": 1,
      "columns": [
        { "name": "id", "type": "Int64" },
        { "name": "amount", "type": "Utf8" }
      ]
    }
  ],
  "drifting_columns": ["amount"],
  "skipped": [
    { "file": "notes.txt.bak", "reason": "no reader for this extension" }
  ]
}
```

`has_drift` is `true` whenever there is more than one variant.
`drifting_columns` names the columns whose type differs between
variants or which some variant lacks entirely.

## Example call

```json
{
  "name": "schema_drift",
  "arguments": {
    "path": "/data/landing",
    "recursive": true
  }
}
```

## Ceilings

- Local paths only. Cloud prefixes are not scanned.
- Multi-table sources report their first table.
- `recursive` stops at depth 8, so a mistyped path cannot walk a whole
  home directory.
- In the in-app Assistant the filesystem sandbox applies, so a scan
  only reaches folders the sandbox already allows.

## See also

- [`octa --schema-drift`](../../cli/schema-drift.md), the CLI mirror,
  which exits 1 on drift for CI gating.
- [`compare_schemas`](compare_schemas.md): two files, side by side.
- [`validate_against_schema`](validate_against_schema.md): one file
  against an expected JSON Schema.
- [`union_tables`](union_tables.md): reconcile differing schemas into
  one table instead of only reporting them.
