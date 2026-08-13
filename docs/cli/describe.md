# `octa --describe`

One-shot orientation snapshot of a tabular file: format, file size,
row count, schema, and a small sample of rows. Saves the usual
`--schema` then `--head` two-step.

## Synopsis

```bash
octa --describe FILE [--table NAME] [--sample-rows N] [-f FORMAT]
```

| Flag                    | Required | Meaning                                                       |
|-------------------------|----------|---------------------------------------------------------------|
| `--describe FILE`       | yes      | The file to describe.                                         |
| `--table NAME`          | no       | Specific table for multi-table sources (SQLite, DuckDB, ...). |
| `--sample-rows N`       | no       | Sample-row count (default 5, clamped to 100).                 |
| `-f`, `--format FORMAT` | no       | Output format: `tsv` (default), `json`, or `csv`.             |

## Output

`-f tsv` and `-f csv` print a vertical `field / value` table that
shells / `awk` can grep easily:

```bash
$ octa --describe sales.parquet
field                value
path                 /home/me/data/sales.parquet
format_name          Parquet
file_size_bytes      1048576
table                
row_count            47832
initial_load_capped  false
column_count         5
column[id]           Int64
column[region]       Utf8
column[amount]       Float64
column[quarter]      Utf8
column[currency]     Utf8
sample_row[0]        1, EU, 1234.5, Q1, EUR
sample_row[1]        2, US, 9876.0, Q1, USD
…
```

`-f json` returns the same data as a structured object that mirrors
the [MCP `describe_file` shape](../mcp/tools/describe_file.md):

```bash
$ octa --describe sales.parquet -f json
{
  "path": "/home/me/data/sales.parquet",
  "format_name": "Parquet",
  "file_size_bytes": 1048576,
  "row_count": 47832,
  "columns": [
    { "name": "id", "type": "Int64" },
    …
  ],
  "sample_rows": [
    ["1", "EU", "1234.5", "Q1", "EUR"],
    …
  ]
}
```

## Examples

### First look at an unfamiliar file

```bash
octa --describe data.parquet
```

### Bigger preview

```bash
octa --describe data.csv --sample-rows 20
```

### Pick a specific table inside a multi-table source

```bash
octa --describe users.sqlite --table customers
```

### Machine-readable output for downstream tooling

```bash
octa --describe data.parquet -f json | jq '.row_count, .column_count'
```

### How was this file written? (`--deep`)

```bash
octa --describe data.parquet --deep
```

`--deep` appends the file's **physical** layout to the normal snapshot:
the facts as `key<tab>value` lines, then one row per column per row
group. It answers the questions the ordinary output cannot, such as why
a file is large or why it scans slowly.

```text
format               Parquet
rows                 2
row_groups           1
columns              3
writer_version       1
created_by           parquet-rs version 58.4.0
compressed_bytes     173
uncompressed_bytes   173
bloom_filters        0
file_size_bytes      1073

row_group  column  rows  compression   encodings                    compressed_bytes  uncompressed_bytes  nulls  min       max
0          id      2     UNCOMPRESSED  PLAIN, RLE, RLE_DICTIONARY   56                56                  0      1         2
0          city    2     UNCOMPRESSED  PLAIN, RLE, RLE_DICTIONARY   61                61                  0      Helsinki  Tokyo
```

Columns are aligned here for readability; the real output is
**tab-separated** like every other `-f tsv` action, so it pipes straight
into `cut`, `awk` or a spreadsheet.

Octa also reports up to three plain-language **hints** when something
looks wrong: very small row groups, missing column statistics, or no
compression. Hints go to **stderr**, so a piped `--deep` run stays
machine-readable.

Notes:

- `--deep` follows the file, like every other companion flag:
  `--describe FILE --deep`, not `--describe --deep FILE`.
- Parquet reports the full detail. Other formats report their size and
  say they have no inspectable internal structure, which is an answer,
  not an error.
- With `-f json` the same information arrives as an `internals` object
  with `facts` and `hints` keys, matching the MCP tool's response.

## See also

- [`octa --schema`](schema.md): schema-only, no sample rows.
- [`octa --head`](head.md): sample rows only, no schema summary.
- [MCP `describe_file`](../mcp/tools/describe_file.md): same feature
  over MCP.
