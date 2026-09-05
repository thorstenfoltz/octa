# Sample files

One openable example of every format Octa reads, so a change to a reader can be
eyeballed in a minute rather than reasoned about.

Open the folder in the sidebar (**File -> Open folder...**) and click down the
list. Everything under `tables/` is the *same eight rows*, so a difference
between two formats is a difference in the format, not in the data.

The binary files go through **Git LFS** (see `.gitattributes` here). A clone
without `git lfs` installed gets pointer files instead of data; `git lfs pull`
fixes that.

## `tables/` - one dataset, every tabular format

Eight products with an integer key, text, a decimal price, a boolean, a date, a
float and a column with empty cells. Enough to show how each format carries
types and nulls.

| File | Format | Worth looking at |
|---|---|---|
| `products.csv` | CSV | The source of every other file here |
| `products.tsv` | TSV | Same, tab separated |
| `products.json` | JSON | Opens in the JSON tree view, not the grid |
| `products.jsonl` | JSON Lines | One object per line |
| `products.parquet` | Parquet | zstd compressed by default; typed columns |
| `products.arrow` | Arrow IPC | Feather v2 |
| `products.avro` | Avro | Schema travels with the data |
| `products.orc` | ORC | Dates land as text: ORC's Rust writer has no date encoder |
| `products.xlsx` | Excel | One sheet; the multi-sheet path needs a real workbook |
| `takings.xlsx` | Excel, with formulas | A different table on purpose: the `revenue` column and the total row are computed. Hover a cell, or open the Record view (F4), to see the formula behind the value |
| `products.ods` | OpenDocument | Written by Octa's own hand-rolled writer |
| `products.xml` | XML | Row elements, opens in the raw view by default |
| `products.yaml` | YAML | Opens in the raw view by default |
| `products.html` | HTML | A page with one `<table>` and a `<caption>` |
| `products.fwf` | Fixed-width | **No `note` column.** Boundaries are inferred from always-blank positions, and a free-text column with spaces cannot be told from a gap |
| `products.dbf` | dBase | `id` comes back as Numeric, not an integer: DBF's Integer field is 32-bit and Octa widens rather than risk a clamp |
| `products.sav` | SPSS | |
| `products.dta` | Stata | |
| `products.rds` | R dataset | The tabular subset R can hand over |
| `products.h5` | HDF5 | Three plain datasets plus one compound (record) dataset |
| `products.msgpack` | MessagePack | Decoded through the JSON path, so nesting flattens the same way |
| `products.bson` | BSON | Concatenated documents, the shape `mongodump` writes |
| `prices.npy` | NumPy | A single 1-D array: one `value` column |
| `measures.npz` | NumPy zip | Three named arrays, so it opens through the table picker |
| `readings.nc` | NetCDF v3 | A different dataset: twelve hourly readings, because NetCDF is about dimensions |

## `documents/` - formats that are documents, not tables

| File | Format | Worth looking at |
|---|---|---|
| `article.md` | Markdown | Preview / editor / split view. Saving writes it back line for line |
| `notes.txt` | Plain text | Raw view, with a tab and a very long line |
| `price_check.py` | Source code | Syntax colouring in the raw view |
| `Dockerfile` | Source code, no extension | Matched by **name**, not extension |
| `config.toml` | TOML | Nested tables and an array of tables, which a flat table has no shape for |
| `analysis.ipynb` | Jupyter notebook | Markdown and code cells, with outputs that survive an edit |
| `coffee.epub` | EPUB | Two chapters, converted to Markdown on load |
| `menu-dump.sql` | SQL dump | Opens as **text** by default. **File -> Open as -> SQL dump** reads it as its tables instead |

## `geo/` - geometry

| File | Format | Worth looking at |
|---|---|---|
| `cafes.geojson` | GeoJSON | Three points and a polygon; opens in the Map view |
| `cafes.shp` (+`.shx`, `.dbf`, `.prj`) | Shapefile | Geometry in one file, attributes in the sibling `.dbf` |
| `cafes.gpkg` | GeoPackage | A SQLite database with the standard `gpkg_*` metadata, which the picker hides |

## `databases/`, `archives/`, and the two directories

| Path | Format | Worth looking at |
|---|---|---|
| `databases/shop.sqlite` | SQLite | Row edits save back as a diff, not a rewrite |
| `databases/shop.duckdb` | DuckDB | Same, and by far the largest file here: DuckDB's page size, not the data |
| `archives/menu.zip` / `.tar` / `.tgz` | Archive | Listed as rows; **Open selected entry** opens one |
| `archives/products.csv.gz` | gzip | Decompressed transparently; saving re-compresses |
| `archives/products.csv.zst` | zstd | Same |
| `dataset-parts/` | Dataset directory | Two parquet parts as **one** table: right-click the folder -> **Open as dataset...** |
| `delta-table/` | Delta Lake | **File -> Open table folder...**. The first open downloads DuckDB's `delta` extension |

## Not here

- **SAS (`.sas7bdat`)** - Octa reads it, but nothing outside SAS writes one, so
  there is no honest way to generate a sample. Point Octa at a real `.sas7bdat`
  if you need to check that reader.
- **Apache Iceberg** - the same directory-open path as Delta, and writing a
  table needs a catalogue rather than a file layout.
- **Live databases** (Postgres, MySQL, Oracle, Snowflake, ...) - those are
  connections, not files.

## How these were made

Everything Octa can write came out of Octa itself, which is half the point:

```sh
octa --convert samples/tables/products.csv samples/tables/products.<ext>
octa --sql samples/tables/products.csv -q "SELECT * FROM data" \
     --sql-write-to samples/databases/shop.sqlite --sql-write-table products
```

The read-only formats (`.npy`, `.npz`, `.msgpack`, `.bson`, `.h5`, `.nc`,
`.rds`, `.shp`, `.gpkg`, `.epub`, `delta-table/`) were generated with one-off
Python via `uv run --with <pkg>`, and the plain-text ones were written by hand.
