# `check_references`

List the child rows whose **foreign key has no matching parent**. Read-only
analytics (stays available under `--mcp-read-only`).

## When to use

- Before a join, to find out why it will drop rows.
- After an import, to check the two files still line up.
- As an agent's sanity check on a dataset it did not produce.

## Input schema

| Parameter        | Type   | Required? | Default      | Description                                   |
|------------------|--------|-----------|--------------|-----------------------------------------------|
| `path`           | string | yes*      | (no default) | The **parent** file                           |
| `open_tab`       | string | no        | (none)       | Open GUI tab holding the parent, or `@active` |
| `table`          | string | no        | first table  | Table within a multi-table parent source      |
| `parent_column`  | string | yes       | (no default) | The parent's key column                       |
| `child_path`     | string | no        | parent       | The child file                                |
| `child_open_tab` | string | no        | parent       | Open tab holding the child                    |
| `child_table`    | string | no        | first table  | Table within a multi-table child source       |
| `child_column`   | string | yes       | (no default) | The child's foreign-key column                |
| `unlimited`      | bool   | no        | `false`      | Lift the streaming row cap                    |

\* One of `path` or `open_tab` is required. Omitting the child source checks a
**self-reference**, such as a `manager_id` pointing at `id` in one table.

## Response

```json
{
  "clean": false,
  "sentence": "2 child row(s) across 1 value(s) have no parent.",
  "parent_values": 3,
  "checked_rows": 4,
  "null_keys": 1,
  "orphan_rows": 2,
  "orphan_values": 1,
  "orphans": [{ "value": "9", "rows": 2 }]
}
```

`orphans` is most-rows-first and capped at 500 listed values; `orphan_rows` and
`orphan_values` stay exact however long the list would have been.

## Two conventions worth knowing

- **A null or empty key is not an orphan.** In every relational database a null
  foreign key means "no parent", not "a parent that vanished". Those rows are
  counted as `null_keys` and left out of `checked_rows`.
- **Keys are compared as trimmed text**, the same as
  [`suggest_join_keys`](suggest_join_keys.md) and the relationship map. That is
  what makes `1` match `1` when one side came from a CSV and the other from a
  database.
