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
| `invoice.pdf` | PDF | Two tables on one page (items, payments), so the table picker opens; the title and headings above them are not part of either |
| `scanned.pdf` | PDF, image only | Stands in for a scanned page: opening it says the tables are pictures and need OCR first. Both PDFs come from `cargo test --lib write_pdf_samples -- --ignored` |
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

## `features/` - small tables for trying a feature by hand

Not formats but situations: each file is built so a feature has something
to find. Plain CSV, readable in any editor.

| File | For | Worth looking at |
|---|---|---|
| `customers.csv` + `orders.csv` | Semi / anti join, test data | Customers 2, 5, 7 and 8 never ordered (anti join finds them); order 107 names customer 9, who does not exist. The pair is also a linked parent and child for **Generate test data** |
| `trades.csv` + `quotes.csv` | As-of join | Join on `ticker` `=` and `time` `>=`: each trade gets the last quote before it. The 08:59 trade is earlier than every quote and INITECH has no quotes, so both stay unmatched; the 09:05 trade hits a quote at exactly the same time |
| `id_checks.csv` | ID checks, Tidy format | Rows C01, C03, C06, C07 are valid; C02 is valid but written untidily (spaces, lower case, dashes, stray blanks); C04 has the last check digit wrong in every column; C05 is one character short; C08 is not even the right shape. The `what_is_wrong` column says which |
| `orders_flat.csv` | Find lookup tables | `customer` decides `name` and `city`. Customer c2 is spelt Mueller twice and Muller once (order 5), so `name` follows it at 88% with one breaking row |
| `access.log` | Log files | nginx combined format: eight requests from three clients, two 404s and one 500. Opens as a table with `status`, `path`, `user_agent` and a `utc_offset` of `+02:00` |
| `app.log` | Log files | Java-style application log. The ERROR at 10:00:07 carries a four-line stack trace that stays in its `message`; the `deploy finished by hand` line fits no entry, so it gets its own row in the `raw` column and a banner counts it |
| `stores.csv` + `regions.geojson` | Spatial join | Six German cities with lat/lon, and two regions: North and South, where South has a hole around Frankfurt. **Inside** gives every store its region except Frankfurt, which sits in the hole |
| `customer_locations.csv` | Spatial join, Nearest | Five customers in German towns. Joined with **Nearest** against `stores.csv`, each gets its closest store and the distance; Freiburg's is 131 km away, so `--within-km 100` leaves it empty |
| `monthly_sales.csv` | Trend and forecast | Four years of monthly sales rising about 1 a month with a summer peak (June) and a small wobble. As a Line chart with **Forecast** 12, the forecast repeats the summer peak a level higher |
| `postcodes.csv` | Value shapes | Ten postcodes written `A-99999` (`D-80331`) and one written bare (`80331`, customer C11). The column funnel's **Shapes** switch shows the two shapes; the Quality Report calls the column `mixed` and lists the stray one |
| `malformed.csv` | CSV repair prompt | A byte-order mark, row 3 with one cell too many, row 4 with one too few. With **Offer repair on malformed files** ticked in Settings, opening it offers the repair. Pinned by `tests/csv_tests.rs`, so do not tidy it |
| `room_bookings.csv` | Timeline view | Room A: bookings 1 and 2 overlap, 2 and 3 only touch (not an overlap). Room B: 5 sits inside 4 and 6 overlaps both. Room C: 7 runs over two days, 8 ends before it starts, 9 has no end (a single point) |

The valid IDs are the standard published test numbers (the ISO example
IBANs, the Visa/Mastercard/Amex test cards, the EAN and ISBN examples), none
belongs to anyone. The generator script checked every verdict in the table
above against its own implementation of each check digit.

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
