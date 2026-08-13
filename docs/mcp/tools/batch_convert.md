# `batch_convert`

Convert several files into one target format in a single run. The same engine
that backs the GUI Batch convert dialog and CLI `--batch-convert`.

**Write tool**: it creates files, so it is removed under `--mcp-read-only` and
refused for a chat profile without **Allow writes**.

## When to use

- Normalise a directory of mixed exports into one format before analysis.
- Turn a pile of CSVs into Parquet in one call rather than N `convert` calls.

## Input schema

| Parameter   | Type     | Required? | Default | Description                                                    |
|-------------|----------|-----------|---------|----------------------------------------------------------------|
| `inputs`    | string[] | yes       |         | Source file paths                                              |
| `out_dir`   | string   | yes       |         | Directory the outputs go into. Created if absent               |
| `to`        | string   | yes       |         | Target extension without a dot (`parquet`, `csv`, `json`, ...) |
| `overwrite` | bool     | no        | `false` | Replace outputs that already exist. Default: skip them         |

## Response shape

```json
{
  "converted": 2,
  "failed": 1,
  "skipped": 0,
  "items": [
    {
      "input": "/data/a.csv",
      "output": "/out/a.parquet",
      "status": "done",
      "rows": 1200,
      "error": null
    },
    {
      "input": "/data/c.csv",
      "output": "/out/c.parquet",
      "status": "failed",
      "rows": null,
      "error": "unsupported encoding"
    }
  ]
}
```

One failed file does not stop the run: the rest still convert, and the item
carries the reason.

## Example call

```json
{
  "name": "batch_convert",
  "arguments": {
    "inputs": ["/data/a.csv", "/data/b.csv"],
    "out_dir": "/tmp/out",
    "to": "parquet"
  }
}
```

## Notes

Outputs are named `<out_dir>/<input stem>.<to>`; colliding stems get `_2`,
`_3` suffixes so a run cannot overwrite its own earlier output. Gzip and zstd
inputs decompress automatically. Multi-table sources convert their first table
only, and files convert sequentially.

## See also

- [`convert`](convert.md): a single file, with more control over the target.
- [`partition_table`](partition_table.md): split one table into many files.
