# `octa --report`

Write an HTML profiling report for a file: per-column statistics,
distribution charts, the most common values and a correlation matrix.

The document is **self-contained**. Its CSS is inline, its charts are
inline SVG, it runs no JavaScript and it fetches nothing, so it opens
from a mail attachment on a machine with no internet and can be
published as a static file with no assets beside it.

## Synopsis

```bash
octa --report OUT.html FILE [--report-sample N] [--report-sections LIST]
     [--table NAME]
```

| Flag                     | Required | Meaning                                                    |
|--------------------------|----------|------------------------------------------------------------|
| `--report OUT.html`      | yes      | Where to write the report.                                 |
| *FILE*                   | yes      | The file to profile (positional).                          |
| `--report-sections LIST` | no       | Comma-separated subset of the four sections. Default: all. |
| `--report-sample N`      | no       | Profile a random sample of N rows instead of every row.    |
| `--table NAME`           | no       | Specific table on a multi-table source.                    |

## Sections

| Name            | What it shows                                                             |
|-----------------|---------------------------------------------------------------------------|
| `stats`         | The per-column statistics table, the same one the Summary tab shows.      |
| `distributions` | One chart per column: a histogram for numbers, a bar of counts otherwise. |
| `top_values`    | The most frequent values per column, with counts and shares.              |
| `correlation`   | A Pearson correlation matrix over the numeric columns.                    |

An unknown section name is an error rather than a silent omission, so a
typo in a scripted call fails loudly.

Every number comes from the engine that already produces it elsewhere in
Octa: the statistics from Summary, the top values from Value frequency,
the matrix from Correlation, the pictures from the Chart tab's own SVG
exporter. The report cannot disagree with what the application shows.

## Sampling

`--report-sample N` profiles a random sample rather than the whole file,
which matters on a table too large to summarise quickly. The document
states that it sampled and gives both counts, so nobody mistakes
approximate numbers for exact ones. Without the flag every row is read.

## Charted columns are capped

Distributions draws at most 50 columns and then names how many it left
out. A 500-column table would otherwise produce a document nobody
scrolls.

## Examples

### A full report

```bash
octa --report sales.html sales.parquet
```

### Statistics only, for a nightly job

```bash
octa --report daily.html data/daily.csv --report-sections stats
```

### A quick look at a very large file

```bash
octa --report peek.html huge.parquet --report-sample 10000
```

## Ceilings

- Local paths only.
- One file per run; there is no folder mode.
- The correlation matrix needs at least two numeric columns, otherwise
  the section is omitted.

## See also

- [Summary](../usage/summary.md), the statistics the report embeds.
- [Correlation](../usage/correlation.md), the matrix it embeds.
- [MCP `create_report`](../mcp/tools/create_report.md), the same feature
  for an agent.
