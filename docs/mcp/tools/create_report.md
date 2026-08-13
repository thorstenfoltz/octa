# `create_report`

Write a self-contained HTML profiling report for a table: per-column
statistics, distribution charts, the most common values and a
correlation matrix.

A **write** tool: it is removed under `--mcp-read-only`, and refused by a
chat profile that does not allow writes.

CLI mirror: [`octa --report`](../../cli/report.md).

## When to use

- The user asks for "a report" or "a profile" of a file they can send on
  or keep.
- A first look at an unfamiliar table where a single document beats a
  dozen tool calls.

For answering a question in the conversation, prefer
[`profile`](profile.md) or [`schema`](schema.md): they return data, this
one returns a file.

## Input schema

| Parameter     | Type     | Required? | Default         | Description                                            |
|---------------|----------|-----------|-----------------|--------------------------------------------------------|
| `out_path`    | string   | yes       | (no default)    | Where to write the report.                             |
| `path`        | string   | no        | (no default)    | Source file. Omit when `open_tab` is set.              |
| `open_tab`    | string   | no        | (no default)    | An open GUI tab's name, or `@active`.                  |
| `table`       | string   | no        | (no default)    | Specific table on a multi-table source.                |
| `title`       | string   | no        | the file's name | Heading and browser-tab title.                         |
| `sections`    | string[] | no        | all four        | `stats`, `distributions`, `top_values`, `correlation`. |
| `sample_rows` | integer  | no        | (every row)     | Profile a random sample of this many rows.             |

## Self-contained

The CSS is inline, the charts are inline SVG, there is no JavaScript and
nothing is fetched at open time, so the file works as a mail attachment
or a static publish with no assets beside it.

Every number comes from the engine that already produces it elsewhere in
Octa, so the report cannot disagree with what the application shows.

## Sampling

`sample_rows` profiles a random sample rather than the whole table. The
document states that it sampled and gives both counts, so a reader
cannot mistake approximate numbers for exact ones.

## Response shape

```json
{
  "path": "/home/user/Downloads/sales.html",
  "bytes": 48213
}
```

## Example call

```json
{
  "name": "create_report",
  "arguments": {
    "path": "/data/sales.parquet",
    "out_path": "sales.html",
    "sections": ["stats", "top_values"]
  }
}
```

## Sandbox

In the in-app Assistant `out_path` resolves through the usual write
sandbox: a bare name lands in the configured export folder, and an
absolute path is accepted only where the profile allows it. Over MCP the
path is used as given.

## Ceilings

- Distributions charts at most 50 columns and names how many it omitted.
- The correlation matrix needs at least two numeric columns, otherwise
  the section is omitted.

## See also

- [`octa --report`](../../cli/report.md), the CLI mirror.
- [`profile`](profile.md): the same statistics as JSON, for answering a
  question rather than producing a document.
- [`correlation`](correlation.md) and [`value_frequency`](value_frequency.md),
  the engines two of the sections embed.
