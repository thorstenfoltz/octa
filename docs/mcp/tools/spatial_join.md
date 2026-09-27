# `spatial_join`

Join by location: the polygon each point lies in, or the nearest point of each
layer with its distance. The same join as the GUI's
[Spatial join](../../usage/spatial-join.md) and `octa --spatial-join`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name        | Type     | Meaning                                                                         |
|-------------|----------|---------------------------------------------------------------------------------|
| `points`    | object   | `{path}` or `{open_tab}` (plus optional `table`): latitude/longitude or points. |
| `layers`    | object[] | One or more tables to join against, each `{path}` or `{open_tab}`.              |
| `op`        | string   | `inside` (default) or `nearest`.                                                |
| `within_km` | number   | With `nearest`: points farther than this stay empty.                            |
| `limit`     | number   | Maximum rows to return. `0` for unlimited.                                      |

## Response

```json
{
  "multi_match": 0,
  "no_point": 0,
  "table": {
    "schema": [
      { "name": "id", "type": "Utf8" },
      { "name": "lat", "type": "Float64" },
      { "name": "lon", "type": "Float64" },
      { "name": "regions_name", "type": "Utf8" }
    ],
    "rows": [["a", 52.52, 13.4, "North"], ["b", 50.11, 8.68, null]],
    "row_count": 2,
    "truncated": false,
    "total_rows_available": 2
  }
}
```

Layer columns are prefixed with the layer's file name (or tab name); the
geometry column is left out. `nearest` adds `<layer>_distance_km`,
great-circle kilometres. `multi_match` counts points that lay in more than one
polygon of a layer (the first was used); `no_point` counts rows without a
readable point. Coordinates must be latitude/longitude.
