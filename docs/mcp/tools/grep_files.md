# `grep_files`

Search **every tabular file in a directory** (one level deep) for a value, like
grep across files. Mirrors the GUI Multi-search directory scope. Read-only
analytics (stays available under `--mcp-read-only`).

## When to use

- "Which file in this folder contains customer 12345?"
- Cross-file lookups without opening each file.

## Input schema

| Parameter          | Type    | Required? | Default      | Description                                          |
|--------------------|---------|-----------|--------------|------------------------------------------------------|
| `dir`              | string  | yes       | (no default) | Directory to search (one level deep; not recursive) |
| `query`            | string  | yes       | (no default) | The text / pattern to search for                    |
| `mode`             | string  | no        | `plain`      | `plain`, `wildcard` (`*`/`?`), or `regex`           |
| `case_sensitive`   | bool    | no        | `false`      | Case-sensitive match                                |
| `whole_word`       | bool    | no        | `false`      | Whole-word match                                    |
| `max_file_size_mb` | integer | no        | `50`         | Skip files larger than this many megabytes          |
| `max_results`      | integer | no        | `1000`       | Cap on total matches returned                       |

Files no reader can parse are skipped (and reported). Capped at `max_results`
matches overall and 1000 per file.

## Response shape

```json
{
  "hits": [
    { "file": "/data/q1.csv", "row": 42, "column": "id", "snippet": "…12345…" }
  ],
  "skipped": [
    { "file": "/data/huge.parquet", "reason": "oversized" }
  ],
  "files_searched": 9,
  "total_hits": 1,
  "truncated": false
}
```

## Example call

```json
{
  "name": "grep_files",
  "arguments": {
    "dir": "/data/exports",
    "query": "ACME*",
    "mode": "wildcard"
  }
}
```

## See also

- [`search`](search.md): match cells within a single file.
- [`run_sql`](run_sql.md): query one file with full SQL.
