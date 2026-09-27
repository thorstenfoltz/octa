# `--spatial-join`

Join by location. Each point of FILE gets the columns of the layer polygon it
lies in (`inside`), or of the nearest point of each layer with its distance
(`nearest`). The same join as the Join dialog's **Spatial** type; see
[Spatial join](../usage/spatial-join.md).

```sh
octa --spatial-join customers.csv --spatial-layer regions.geojson
octa --spatial-join customers.csv --spatial-layer stores.csv --spatial-op nearest --within-km 25
```

## Flags

| Flag              | Required? | Description                                                           |
|-------------------|-----------|-----------------------------------------------------------------------|
| `--spatial-join`  | yes       | The points: latitude/longitude columns or a point geometry.           |
| `--spatial-layer` | yes       | A table to join against. Repeat for several layers.                   |
| `--spatial-op`    | no        | `inside` (default) or `nearest`.                                      |
| `--within-km`     | no        | With `nearest`: points farther than this stay empty. Needs `nearest`. |

## Output

The points table with, per layer, the layer's columns prefixed with the layer's
file name (`regions.geojson` gives `regions_name`); the geometry column is left
out. `nearest` adds `<layer>_distance_km`, great-circle kilometres.

```text
id  lat    lon    regions_name  regions_manager
a   52.52  13.40  North         Ada
b   50.11  8.68
```

On stderr: how many points lay in more than one polygon of a layer (the first
was used) and how many rows had no readable point. Coordinates must be
latitude/longitude; a layer in another coordinate system is refused.
