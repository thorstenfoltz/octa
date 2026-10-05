# `--hash-columns`

Append a column holding a hash of the named columns and print the table to
stdout. The input file is never modified.

```
octa --hash-columns COL,COL,... FILE [--hash-algo md5|sha256|sha512]
     [--hash-delimiter TEXT] [--hash-null TEXT] [--hash-trim] [--hash-upper]
     [--hash-name NAME] [-f tsv|json|csv]
```

The columns are joined in the order given, with the delimiter between them,
and every value is turned into text first, whatever its type. The hash is
lowercase hex.

| Flag               | Default          | What it does                                                 |
|--------------------|------------------|--------------------------------------------------------------|
| `--hash-algo`      | `md5`            | `md5`, `sha256` or `sha512`                                  |
| `--hash-delimiter` | `\|`             | Put between the values; may be empty (`--hash-delimiter ""`) |
| `--hash-null`      | empty            | Text that stands in for a NULL cell                          |
| `--hash-trim`      | off              | Strip whitespace from each value first                       |
| `--hash-upper`     | off              | Upper-case each value first                                  |
| `--hash-name`      | `hash_<columns>` | Name of the new column; must not exist yet                   |

## Examples

```
octa --hash-columns customer_id,order_date orders.parquet -f csv > keyed.csv
octa --hash-columns first,last,birth --hash-algo sha256 --hash-trim --hash-upper people.csv
```

The same function is in the app as **Columns > Hash columns...** and in the
MCP server as [`hash_columns`](../mcp/tools/hash_columns.md).
