# `--batch-convert`

Convert many files into one target format in a single run. The input files
are never modified.

```
octa --batch-convert --to EXT --out-dir DIR FILE... [--overwrite]
```

| Flag              | Required? | Description                                                               |
|-------------------|-----------|---------------------------------------------------------------------------|
| `--batch-convert` | yes       | The action                                                                |
| `--to EXT`        | yes       | Target extension, without the leading dot (`parquet`, `csv`, `json`, ...) |
| `--out-dir DIR`   | yes       | Output directory. Created if absent                                       |
| `--overwrite`     | no        | Replace outputs that already exist. Default: skip them                    |

`--to` is shared with the cloud transfer actions, where it means a destination
URL; `--out-dir` is shared with [`--partition-by`](partition.md). The actions
are mutually exclusive, so there is no ambiguity.

## Output

Stdout is a bare listing, one line per input, tab-separated and headerless
like `--partition-by`:

```
/data/a.csv ./out/a.parquet done
/data/b.csv ./out/b.parquet done
/data/c.csv ./out/c.parquet failed
```

Per-file errors and the summary go to **stderr**, so the listing stays
machine-readable:

```
/data/c.csv: unsupported encoding
2 converted, 1 failed, 0 skipped
```

The action ignores the global `-f / --format`: this is a record of work done,
not a data table.

## Exit code

**1 if any single file failed**, 0 otherwise, so a pipeline can gate on it:

```
octa --batch-convert --to parquet --out-dir ./out *.csv || exit 1
```

A skipped file (the output already exists) is not a failure and does not
affect the exit code.

## Naming and collisions

Outputs are `<out-dir>/<input stem>.<EXT>`. Two inputs whose stems collide get
`_2`, `_3` suffixes in input order, so a run can never silently overwrite its
own earlier output:

```
octa --batch-convert --to parquet --out-dir ./out /jan/data.csv /feb/data.csv
# -> ./out/data.parquet and ./out/data_2.parquet
```

## Examples

```
octa --batch-convert --to parquet --out-dir ./out a.csv b.csv c.csv
octa --batch-convert --to json --out-dir ./out --overwrite data/*.csv
octa --batch-convert --to csv --out-dir ./out archive/*.parquet.zst
```

Gzip and zstd inputs decompress automatically.

## Limits

Local paths only, the first table only for multi-table inputs (an Excel
workbook converts sheet one), and files convert sequentially.

## Write options

`--compression CODEC` and `--row-group-size N` apply to every file in
the run, exactly as they do for [`--convert`](convert.md):

```bash
octa --batch-convert *.csv --to parquet --out-dir out/ --compression zstd
```

Options a target format cannot honour are ignored rather than rejected,
since one option set covers the whole batch.

Omit them and the run uses whatever is saved in **Settings > Files >
Write options**, the same values the app itself writes with; with no
settings file the built-in defaults apply (`zstd`, writer-default row
groups).

## See also

- [Batch Convert](../usage/batch-convert.md) (GUI) and the
  [`batch_convert`](../mcp/tools/batch_convert.md) MCP tool.
- [`--convert`](convert.md) for a single file.
