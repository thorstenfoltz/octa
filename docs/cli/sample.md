# `octa --sample`

Print a random N-row sample of a file. The sample is taken **without
replacement**, the chosen rows keep their **original order**, and it is
**reproducible** for a given seed. Default **20 rows**.

## Synopsis

```bash
octa --sample FILE [-n N] [--seed S] [-f tsv|json|csv] [--rows N|all]
```

| Flag                | Default | Meaning                                                         |
|---------------------|---------|-----------------------------------------------------------------|
| `-n N`, `--lines N` | `20`    | Sample size. When N ≥ row count, every row is returned.         |
| `--seed S`          | `0`     | RNG seed. The same seed + file yields the same sample.          |
| `-f`, `--format`    | `tsv`   | Output format (see [CLI overview](index.md#output-formatting)). |
| `--rows N\|all`     | 5 M     | Initial-load row cap for streaming formats (see Performance).   |

## Examples

### A reproducible 20-row sample

```bash
octa --sample sales.parquet --seed 1
octa --sample sales.parquet --seed 1   # identical output to the line above
```

### Custom sample size and format

```bash
octa --sample events.csv -n 100 --seed 7 -f json
```

### Different seeds give different samples

```bash
octa --sample sales.parquet -n 5 --seed 1
octa --sample sales.parquet -n 5 --seed 2   # a different set of 5 rows
```

## Performance

For streaming formats the sample is drawn from the rows within the
**initial-load cap** (5 million by default). To sample from the whole
of a very large file, raise the cap with `--rows all`:

```bash
octa --sample huge.parquet -n 50 --rows all
```

## Notes

- Sampling is uniform without replacement; for `N ≥ row count` the
  whole table is returned (still in original order).
- The seed makes pipelines deterministic. Use a fixed seed in tests and
  documentation; vary it when you want a fresh draw.

## See also

- [`octa --head`](head.md) / [`octa --tail`](tail.md): the first / last
  N rows.
- [MCP `sample` tool](../mcp/tools/sample.md): the same access pattern
  via MCP, with a `seed` parameter.
