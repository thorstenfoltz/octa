# `apply_recipe`

Replay a saved recipe (`.ocp`, recorded in the Octa GUI) on a table and
return the result. Steps run by column name; a step whose column is missing
is skipped and reported, the rest still run.

Read-only, so it stays available under `--mcp-read-only`. To keep the result,
pass `table` on to [`write_table`](write_table.md).

## Parameters

| Name          | Type   | Meaning                                           |
|---------------|--------|---------------------------------------------------|
| `recipe_path` | string | The `.ocp` recipe file.                           |
| `path`        | string | The data file. May be a cloud URL.                |
| `open_tab`    | string | Use an open GUI tab instead (name, or `@active`). |
| `table`       | string | Sheet or table name for multi-table sources.      |
| `limit`       | number | Maximum rows to return. 0 for unlimited.          |
| `unlimited`   | bool   | Lift the streaming row cap for this call.         |

## Response

```json
{
  "skipped": 1,
  "steps": [
    { "step": "Renamed city -> town", "ran": true, "error": null },
    { "step": "Deleted the columns nope", "ran": false, "error": "column `nope` not found" }
  ],
  "table": { "schema": [...], "rows": [...], "row_count": 3 }
}
```

See [Recipes](../../usage/recipes.md).
