# `find_overlaps`

Find rows whose time spans overlap inside a lane: two bookings of one
room, one person on two shifts. The same pairs as the GUI's
[Timeline](../../usage/view-modes/timeline.md) view and `octa --overlaps`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name       | Type   | Meaning                                                              |
|------------|--------|----------------------------------------------------------------------|
| `path`     | string | The file. May be a cloud URL.                                        |
| `open_tab` | string | Use an open GUI tab instead (name, or `@active`).                    |
| `table`    | string | Sheet or table name for multi-table sources.                         |
| `start`    | string | Start column. Default: the first date column.                        |
| `end`      | string | End column. Default: the second date column; none makes rows points. |
| `lane`     | string | Only rows with the same value here can overlap.                      |
| `label`    | string | A column shown beside each row of a pair.                            |
| `limit`    | number | Maximum pairs to return. `0` for unlimited.                          |

## Response

```json
{
  "overlap_count": 4,
  "lanes_with_overlaps": 2,
  "backwards_rows": [8],
  "overlaps": { "schema": [...], "rows": [["Room A", 1, "Team meeting", "2026-10-05 09:00", "..."]], "row_count": 4 }
}
```

`backwards_rows` lists the 1-based rows whose end is before their start;
they are left out of the search.
