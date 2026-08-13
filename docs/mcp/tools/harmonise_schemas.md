# `harmonise_schemas`

Rewrite every data file in a folder to one common set of columns,
writing harmonised copies into a **separate** folder.

This is the write half of [`schema_drift`](schema_drift.md): that tool
tells you the folder disagrees with itself, this one fixes it.

!!! warning "Write tool"
    `harmonise_schemas` creates files. It is dropped when the server is
    started with `--mcp-read-only`. The **originals are never modified**
    - output always goes to `out_dir`, which must differ from `dir`.

## Parameters

| Name          | Type    | Required | Default | Description                                                          |
|---------------|---------|----------|---------|----------------------------------------------------------------------|
| `dir`         | string  | yes      | -       | Folder to scan.                                                      |
| `out_dir`     | string  | yes      | -       | Folder to write harmonised copies into. Must differ from `dir`.      |
| `target_file` | string  | no       | -       | Take the target schema from this file instead of the majority shape. |
| `recursive`   | boolean | no       | `false` | Walk subfolders.                                                     |
| `ignore_case` | boolean | no       | `false` | Treat column names differing only in case as one column.             |
| `overwrite`   | boolean | no       | `false` | Replace files that already exist in `out_dir`.                       |

## Response

```json
{
  "target_columns": [
    { "name": "order_id", "type": "Int64" },
    { "name": "amount", "type": "Float64" }
  ],
  "items": [
    {
      "input": "2026-01.parquet",
      "output": "/out/2026-01.parquet",
      "status": "already_matches",
      "rows": 1200,
      "dropped_columns": [],
      "reason": null
    },
    {
      "input": "2026-02.parquet",
      "output": "/out/2026-02.parquet",
      "status": "harmonised",
      "rows": 1180,
      "dropped_columns": ["legacy_note"],
      "reason": null
    },
    {
      "input": "2026-03.parquet",
      "output": "/out/2026-03.parquet",
      "status": "refused",
      "rows": 0,
      "dropped_columns": [],
      "reason": "column 'amount' holds values that will not cast to Float64"
    }
  ],
  "written": 2,
  "refused": 1,
  "skipped": []
}
```

- `status` is `already_matches`, `harmonised` or `refused`.
- Columns missing from a file are added as nulls; columns not in the
  target are dropped and named per file in `dropped_columns`.
- A file whose values will not survive a cast is **refused, never
  written with nulls in place of them**. A harmonised folder of silently
  emptied cells looks clean and is not.
- Two inputs colliding on one output name refuse **both**, deliberately
  unlike [`batch_convert`](batch_convert.md)'s `_2` suffix, which would
  paper over exactly the ambiguity that matters here.

## Notes

- Local paths only; `out_dir` goes through the same write-path
  resolution as every other write tool.
- Multi-table sources contribute their first table only.
- Sequential; schema-level only (no row-level reconciliation).

## See also

- [`schema_drift`](schema_drift.md): which files disagree, before fixing.
- [`compare_schemas`](compare_schemas.md): the same question for two files.
- [Harmonise Schemas](../../usage/harmonise-schemas.md): the same feature
  in the GUI, plus the `--harmonise-schema` CLI action.
