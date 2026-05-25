# `search`

Search **every cell** of a tabular file for a query string and return
the matching cells with their location and a context snippet.

## When to use

- Locating a value when you don't know which column holds it.
- Regex / wildcard sweeps across a whole file.
- "Does this file mention `…` anywhere?" probes.

## Input schema

| Parameter   | Type    | Required? | Default        | Description                                                                      |
|-------------|---------|-----------|----------------|----------------------------------------------------------------------------------|
| `path`      | string  | yes       | (no default)   | Path to the file                                                                 |
| `query`     | string  | yes       | (no default)   | Text or pattern to search for                                                    |
| `mode`      | string  | no        | `plain`        | `plain`, `wildcard`, or `regex`                                                  |
| `table`     | string  | no        | (no default)   | Specific table for multi-table sources                                           |
| `limit`     | integer | no        | server default | Max hits to return in the response. `0` = unlimited.                             |
| `unlimited` | bool    | no        | `false`        | Lift the 5,000,000-row file-loader cap so the search scans every row in the file |

### Modes

| Mode       | Behaviour                                           |
|------------|-----------------------------------------------------|
| `plain`    | Case-insensitive substring match (default).         |
| `wildcard` | `*` matches any run of characters, `?` matches one. |
| `regex`    | Full regular expression (regex-crate syntax).       |

## Response shape

```json
{
  "hit_count": 37,
  "returned": 37,
  "truncated": false,
  "hits": [
    { "row": 14, "col": 3, "column_name": "email", "snippet": "ada@example.com" }
  ]
}
```

`hit_count` is the total number of matches; `returned` is how many
`hits` are in this response; `truncated` is `true` when `hit_count`
exceeded the limit.

## Example calls

```json
{
  "name": "search",
  "arguments": { "path": "/tmp/users.csv", "query": "gmail.com" }
}
```

```json
{
  "name": "search",
  "arguments": {
    "path": "/tmp/logs.parquet",
    "query": "^ERROR",
    "mode": "regex"
  }
}
```

## See also

- [`run_sql`](run_sql.md) — `WHERE` clauses for column-scoped, typed
  filtering.
- [`value_frequency`](value_frequency.md) — counts rather than locations.
