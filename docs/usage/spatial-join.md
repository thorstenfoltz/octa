# Spatial join

Join by location instead of by matching values. Open **Data -> Join
tables...**, pick the tab with the points as **Left**, and choose **Spatial**
as the join type. **Right** is the tab to join against, the **layer**; the
conditions give way to the spatial options, and **More layers** lists the
other open tabs so you can join several at once.

The points tab needs latitude/longitude columns (found the same way the Map
view finds them) or a point geometry. When you pick **Spatial** and the Left
tab has no points (a regions file you opened last, say), Left switches to a
tab that has them. The dialog shows which columns it uses, or says the tab
has none and keeps **Apply** greyed out.

## Inside

Each point gets the columns of the polygon it lies in: which sales region a
customer belongs to, which district a store is in. Layers here are polygon
files: GeoJSON or shapefile. Holes in polygons are honoured, and a
point in no polygon stays empty.

When polygons of one layer overlap, a point can lie in more than one; the first
is taken and a banner on the result says how many points that affected.

## Nearest

Each point gets the columns of the closest point in each layer, plus
`<layer>_distance_km`: the nearest store to each customer, with how far it is.
Distances are great-circle kilometres over the earth's surface, so there are no
projection settings to get right. **Only within km** leaves points farther away
than that empty; leave it blank for no limit.

## The result

A new tab holding the points table with, per layer, that layer's columns
prefixed with the layer's name (`regions_name`, `regions_manager`). The
geometry column of a layer is left out. Rows without a readable point stay
empty, and a banner counts them. The tabs you joined are not changed.

## Coordinates

Everything must be latitude/longitude (WGS 84). A layer in another coordinate
system (metres in a national grid, say) is refused with a message naming it,
rather than silently matching nothing. Reproject it first.

## Example

Two small tables in `samples/features/` are enough to try both operations.

### Inside: which region is each store in?

`stores.csv` has six stores with their coordinates:

| store | city      | lat     | lon     |
|-------|-----------|---------|---------|
| S1    | Berlin    | 52.5200 | 13.4050 |
| S2    | Hamburg   | 53.5511 | 9.9937  |
| S3    | Munich    | 48.1372 | 11.5755 |
| S4    | Cologne   | 50.9375 | 6.9603  |
| S5    | Frankfurt | 50.1109 | 8.6821  |
| S6    | Stuttgart | 48.7758 | 9.1829  |

`regions.geojson` has two sales regions, each drawn as a polygon:

| name  | manager | shape                                                  |
|-------|---------|--------------------------------------------------------|
| North | Ada     | the north of Germany, from 51.5° N up                  |
| South | Grace   | the south, below 51.5° N, with a hole around Frankfurt |

1. Open both files.
2. Choose **Data -> Join tables...** and set the type to **Spatial**.
   **Left** switches to `stores.csv` (the tab with points) and **Right** to
   `regions.geojson`.
3. Leave **Inside** selected and press **Apply**.

A new tab opens with every store and the region it lies in. The layer's
columns are prefixed with its name, `regions`:

| store | city      | lat     | lon     | regions_name | regions_manager |
|-------|-----------|---------|---------|--------------|-----------------|
| S1    | Berlin    | 52.52   | 13.405  | North        | Ada             |
| S2    | Hamburg   | 53.5511 | 9.9937  | North        | Ada             |
| S3    | Munich    | 48.1372 | 11.5755 | South        | Grace           |
| S4    | Cologne   | 50.9375 | 6.9603  | South        | Grace           |
| S5    | Frankfurt | 50.1109 | 8.6821  |              |                 |
| S6    | Stuttgart | 48.7758 | 9.1829  | South        | Grace           |

Frankfurt stays empty: it lies inside South's outline but in the hole cut out
of it, so it is in no region at all.

### Nearest: which store is closest to each customer?

`customer_locations.csv` has five customers:

| customer | town     | lat     | lon     |
|----------|----------|---------|---------|
| K1       | Potsdam  | 52.3906 | 13.0645 |
| K2       | Lübeck   | 53.8655 | 10.6866 |
| K3       | Augsburg | 48.3705 | 10.8978 |
| K4       | Bonn     | 50.7374 | 7.0982  |
| K5       | Freiburg | 47.9990 | 7.8421  |

1. Open `customer_locations.csv` and `stores.csv`.
2. **Data -> Join tables...**, type **Spatial**. Both tabs have points, so
   pick them yourself: **Left** `customer_locations.csv` (the points that get
   an answer), **Right** `stores.csv` (the points to measure to).
3. Select **Nearest** and press **Apply**.

Each customer gets the closest store's columns and the distance in kilometres
over the earth's surface (the store's `lat` and `lon` columns are left out
here for width):

| customer | town     | stores_store | stores_city | stores_distance_km |
|----------|----------|--------------|-------------|--------------------|
| K1       | Potsdam  | S1           | Berlin      | 27.191             |
| K2       | Lübeck   | S2           | Hamburg     | 57.462             |
| K3       | Augsburg | S3           | Munich      | 56.484             |
| K4       | Bonn     | S4           | Cologne     | 24.266             |
| K5       | Freiburg | S6           | Stuttgart   | 131.388            |

Type `100` into **Only within km** and apply again: Freiburg's nearest store is
131 km away, so Freiburg keeps its own columns but its store columns stay
empty, while the other four customers keep their stores.

One operation applies to every layer of a join. To get a customer's region
*and* nearest store, run two joins: Inside against `regions.geojson`, then
Nearest against `stores.csv` on the result tab.

### The same on the command line

```sh
octa --spatial-join samples/features/stores.csv \
     --spatial-layer samples/features/regions.geojson

octa --spatial-join samples/features/customer_locations.csv \
     --spatial-layer samples/features/stores.csv \
     --spatial-op nearest --within-km 100
```

## Command line and assistant

`octa --spatial-join` (see [`--spatial-join`](../cli/spatial-join.md)) and
the [`spatial_join`](../mcp/tools/spatial_join.md) MCP tool do the same.
