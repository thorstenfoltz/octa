# `--bands`

Add a column of number bands (`18-34`, `65+`) right after a column, or read
band labels back into numbers, and print the table. Never modifies the file.
The same engine as **Columns > Number bands...**; see
[Number bands](../usage/number-bands.md).

```sh
octa --bands people.csv --bands-column age --bands-edges 18,35,65
octa --bands people.csv --bands-column income --bands-by count --bands-count 4
octa --bands survey.csv --bands-column age_group --bands-back middle
```

## Flags

| Flag             | Required? | Description                                                                    |
|------------------|-----------|--------------------------------------------------------------------------------|
| `--bands`        | yes       | The file.                                                                      |
| `--bands-column` | yes       | The column to band, or to read back.                                           |
| `--bands-by`     | no        | `width` (default), `count` or `edges`.                                         |
| `--bands-count`  | no        | How many bands for `width` and `count`. Default `5`.                           |
| `--bands-edges`  | no        | Where bands start, `18,35,65`; semicolons for decimal commas. Implies `edges`. |
| `--bands-back`   | no        | Read bands back instead: `middle`, `lower` or `upper`.                         |

`--bands-back` cannot be combined with `--bands-by`, `--bands-count` or
`--bands-edges`, `--bands-edges` only goes with `--bands-by edges` and
`--bands-count` only with `width` or `count`. Any of those, or a word that is
not one of the choices, exits 1 with the reason.

## Output

The whole table with the new column, named `<column>_band` (or
`<column>_middle`, `_lower`, `_upper`), right after its source:

```text
name  age  age_band  group
Ann   5    <18       unter 18
Bob   18   18-34     18 bis 24
Eve   70   65+       65 und älter
```

With `--bands-back middle` on `group` the same rows give 15, 21 and 69.5.
How many labels held no number goes to stderr.

## See also

- MCP: [`number_bands`](../mcp/tools/number_bands.md)
- In the app: [Number bands](../usage/number-bands.md)
