A large release. The Assistant now also runs on a Claude, GitHub Copilot or
Codex subscription, beside the API keys it used before, and can rewrite text.
**Classify with Jev** labels rows by a question you ask. Octa looks up exchange
rates, inflation and places from public reference data, gains a row of
analyses (calculated columns, cohorts, funnels, customer segments,
seasonality, number bands), keeps long numbers exact, reads mail, Access,
MATLAB, QlikView, SAS transport, GPX, KML and Lance files, shows tabs side by
side and in groups, and finds every command by name with Ctrl+K. Three more
languages bring the total to 35.

## What's new

### Assistant

- **Subscription profiles**: a profile can now also run through the Claude,
  GitHub Copilot or Codex program you are signed in to, with no API key.
  Profiles with an API key work as before. Octa's tools work on a
  subscription too, and for Claude and Codex the panel shows the plan's
  limits.
- **Writing mode** rewrites the text of a Markdown, text or code tab. The
  proposal comes with **Compare**, **Accept** and **Discard**, and **Undo
  change** afterwards.
- **On a database tab** the Assistant runs its work on the server and never
  downloads the table. A new `quality_report` tool does the same.
- **Show what happens** lists every step of a reply: prompt, messages, tool
  calls and results.
- A label beside the profile says whether it may write, and **Read-only
  mode** (F8) now overrides every profile.
- A profile can have its own limits and export folder, and **No token
  limit**.
- A **Maximise** button gives the panel the whole window.

### Classify with Jev

**Columns > Classify with Jev...** labels every row by a question you ask
in plain words, such as whether a review is a complaint. The answer is a
category, yes or no, or a grade, with a certainty beside it, so you can
sort out the rows worth a second look.

It uses TypeSafe AI's Jev model, so the cells you choose are sent to
servers in the United States. **It is off until you switch it on** under
**Settings > Classify with Jev**, where the API key goes too. Also
`octa --classify` and the MCP tool `classify_rows`.

### Reference data

**Data > Reference data...** adds columns from public sources:

- **Convert currency** at the rate of each row's date, or at the newest
  rate. Rates come from the Bank for International Settlements. A weekend
  takes the last rate before it.
- **In today's money** expresses an amount from one month in the prices of
  another, by the consumer price index of its country. The month comes from
  a column or is the same for every row.
- **Places and region keys** looks up a postcode, a municipality code, a
  Kreisschlüssel or a NUTS code and adds the place, its coordinates and the
  other keys. Postcodes cover 124 countries, municipality codes 34 European
  countries, the Kreisschlüssel Germany.

Currencies and countries are listed by name in your language, such as
`Euro (EUR)`. Rounding is your choice. **Every download is off until you
switch it on** under **Settings > Online data**, nothing about your data is
sent, and Octa ships none of this data itself. Also `octa --reference` and
the MCP tool `reference_data`. The page **Reference data** names every
source and licence.

### Analyses

- **Calculated column** (Columns menu): rank, dense rank, row number,
  running total, percent of total, difference and percent change to the
  previous row, the value from a previous or next row, a visit number from
  timestamps, and an ABC class. Each can restart per group. On a database
  tab the server computes it.
- **Cohorts** (Analyse menu): who came back, by the period of their first
  row, as a share, a count or a sum, with a heatmap.
- **Funnel**: how many people reach each step, in order, with an optional
  time limit.
- **Customer segments (RFM)**: marks for recency, frequency and amount per
  customer, with a segment name.
- **Seasonality**: the Line chart splits a series into trend, repeating
  pattern and rest, as a table and a chart.
- **Number bands** (Columns menu): numbers into bands such as `18-34` and
  `65+`, and band labels back into numbers.
- **Detect outliers** gains **Neighbours**, for a series over time: a value
  is compared with those around it, on request only with the same weekday.
- **Value frequency** counts words as well as whole values.
- **Charts**: Pareto, Heatmap and Calendar are new kinds.
- **Map**: a column of country names or codes colours a world map, in about
  25 languages.
- **Data quality report**: a **Missing numbers** check finds holes in a
  numbering such as invoice numbers.
- **Date/Time calculation** counts working days between two dates, with
  your days off and a list of holidays.

Calculated columns, cohorts, funnel, segments, seasonality, number bands
and Classify also work from the command line and the MCP server.

### Exact numbers

Decimal numbers in CSV and other text files are read digit for digit and
saved to Parquet, Arrow and databases as decimals, not floats. Sums, the
status bar and chart totals add them up exactly, and so do sums on a
database tab. `1.50` and `1.5` are the same value, also when looking for
duplicates. Averages in Summary, Pivot and Time series stay floats, good to
about 15 digits. The new page **Exact numbers** has the details and limits.

### New file formats

- **Mail, calendar and contacts**: `.eml`, `.mbox`, Outlook `.msg`, `.oft`,
  `.pst` and `.olm`, `.ics`, `.vcs` and `.vcf`. One table per kind, with
  attachments in a table of their own.
- **Microsoft Access** `.mdb` and `.accdb`, password protected ones
  included.
- **MATLAB** `.mat`, **QlikView** `.qvd`, **GPX**, **KML** and **KMZ**.
- **SAS transport** `.xpt`, read and write.
- **Lance** datasets, read and write, vector columns included.

### Tabs and the table

- **Tiles**: show tabs side by side, stacked or in a grid. Each tile keeps
  its own view.
- **Tab groups**: drop a tab on another to group them. A group has a name
  and a colour and folds to a chip. Tabs can be dragged into a new order.
  Tiles and groups return with **Reopen last session**.
- **Find a command** (Ctrl+K, also under Help): type a name to run any
  command, or to find a feature inside a dialog.
- **Live tabs**: **View > Follow file** shows lines as they are appended to
  a CSV, TSV, JSON Lines or log file. **Auto-refresh...** reads a database
  or API tab again at an interval. The status bar always shows whether a
  tab refreshes.
- **Just type**: click a cell and type to replace its value. Enter stores
  it and moves down.
- **Fill series** (Edit menu): numbers, IDs that count on, dates or a
  repeating pattern down the selected cells.
- **Copy as** (Edit menu and right-click): Markdown table, SQL IN list,
  JSON or SQL INSERT statements.
- **Hide rows**, and hide several selected columns at once. The status bar
  shows the count and brings them back.
- **Six more mark colours**: pink, teal, lime, indigo, brown and grey.
- **Markdown**: `mermaid` code blocks are drawn as diagrams.
- **Menus show every entry.** One that cannot be used right now is greyed
  and says why on hover, in the menus, the right-click menus and Settings.
- **The zoom level survives a restart.**

### SQL panel

- SQL editors come back with **Reopen last session**, with their text and
  file only, so nothing connects on start.
- Double-click a result cell to mark its row.
- Deleting a saved snippet asks first.
- **Ask** no longer picks a table for you. It uses the tab's own, or one
  the question names.

### Languages

Catalan, Brazilian Portuguese and Traditional Chinese are new. Portuguese
was rewritten as European Portuguese. That makes 35 languages.

### Command line and MCP

New actions, each with a matching MCP tool: `--calculate`
(`calculate_column`), `--cohort` (`cohort_table`), `--funnel` (`funnel`),
`--rfm` (`rfm_segments`), `--seasonality` (`seasonality`), `--bands`
(`number_bands`), `--classify` (`classify_rows`) and `--reference`
(`reference_data`). `--outliers` takes `--outlier-method neighbours` with
`--outlier-window` and `--outlier-same-weekday`. The MCP server now offers
81 tools.

### Other

- A new page, **Other programs Octa works with**, lists the optional
  programs Octa can use, such as `git` and the cloud command line tools.

### New and changed settings

New:

- A model profile gains the providers Claude, GitHub Copilot and Codex
  (subscription) beside the API ones, its own limits and its own export
  folder.
- **Settings > Online data**: five downloads for Reference data, each with
  **Download and use** (off by default), **Refresh every** (1 month),
  **Download now** and **Delete downloaded data**.
- **Settings > Classify with Jev**: **Allow sending data to TypeSafe AI**
  (off by default), API key, model and parallel requests.
- **Settings > Chat / Assistant > Apply writing changes directly**, off by
  default. Writing mode then puts the new text into the editor without
  asking.
- **Settings > Chat / Assistant > Export 'Show what happens'**, off by
  default. Chat exports then carry every step of the debug box, which can
  include your data.
- **Settings > Databases > Minimum auto-refresh interval**, 5 seconds, and
  **Minimum on billed engines**, 60 seconds, for BigQuery, Athena and
  Snowflake, where every read costs money.
- **Settings > SQL > Ask before deleting a snippet**, on by default.

Changed:

- **Reopen last session** also brings back SQL editors, tiles and tab
  groups.
- **Language** lists 35 languages. A saved `no`, `pt`, `sr` or `zh` is read
  as Norwegian Bokmål, Portuguese (Portugal), Serbian (Cyrillic) and
  Simplified Chinese.
- **Font size** and zoom: Octa starts at the zoom you last used.

## Fixes

### Losing work

- **Quitting asked about one tab only.** Closing the window, **File > Exit**
  and the quit shortcut checked the tab in front and nothing else, so unsaved
  edits in a background tab were dropped without a question. Octa now goes
  through every modified tab, shows it and asks. Cancel stops the quit.
- **A capped read could overwrite the whole file.** With a row limit in
  force, some command line actions and MCP tools read part of a file and
  could write that part back over the original. Nothing overwrites its own
  input with part of it any more, checks and conversions on the command line
  read the whole file, and everything else says when it saw only part.
  **File > Save to database** warns the same way on a partly loaded tab.
- **SQL editors disappeared with their tab.** Closing a tab, refreshing it or
  reloading it could drop its SQL editors, and a query still running ended
  as "Server query finished, but its tab was closed". An editor now ends only
  when you close it. When its tab closes, it moves to the tab in front or to
  a new one, with its query and its result.
- **Three actions took one Ctrl+Z per cell.** Date/Time calculation, Add
  column with a formula and Conditional column wrote every cell as an edit
  of its own, so undoing a column of 10,000 rows took 10,000 presses. Each
  is one undo step now.

### Security

- **Three ways round the Assistant's and the MCP server's file limits are
  closed.** A query result written into a database file, an expression in a
  table edit, and a column name in a join could each reach files they should
  not, the last one even in read-only mode. A statement that starts with
  `WITH` and ends in a write no longer passes as a read either.
- **Updated libraries close known issues** in the Excel reader and the
  Markdown parser.

### Numbers

- **Long numbers in text files were rounded.** A CSV value such as
  `12345678901234567890.12` came out as `12345678901234567000`, and a sum of
  0.1, 0.2 and 0.3 was `0.6000000000000001`. Such values are now read and
  added up digit for digit. See Exact numbers above.
- **Nested DuckDB columns were saved as empty cells.** Converting a DuckDB
  file with list, struct, map or vector columns wrote nothing in their place
  and reported success. They are read and written whole now. A `TIME_NS`
  value no longer ends the program.
- **Saving a decimal column to Avro failed** and left an empty file behind,
  and so did a whole number in a column of decimals. Both are written now.
- **Joins and unions across number and text columns** could fail on one
  value that was not a number. Matching values now compare as numbers and
  the rest as text.
- **The diagonal of a correlation matrix** could read `0.9999999999` instead
  of 1, a rounding leftover from the calculation. A column compared with
  itself is exactly 1 now.

### Databases

- **Saving to Oracle could hang up the connection** on a statement longer
  than about 8 KB. Statements are sent in smaller batches.
- **Saving dates to Trino, Athena and Databricks was refused**, because they
  were sent as text. They are sent as dates and timestamps.
- **Sums on a database tab went through a float**, so a long column of
  amounts could be off in the last digits. A decimal column is now added up
  exactly on the server.
- **A refresh of a database or API tab lost its pin and its name.** A
  pinned tab came back unpinned and a renamed one under its old name. Both
  survive a refresh now.

### Display

- **View > Map was greyed for everything but GeoJSON.** Tables with
  latitude and longitude, Shapefiles and the new country map were reachable
  only with F4. The menu entry now works wherever F4 does.
- **A docked Assistant could cover the whole window** after moving it from
  the bottom to a side. Docked panels keep to their share now.
- **Best-fit column width cut long values and the title.** It looked at the
  first 5,000 rows only and measured the title in the wrong font. It looks at
  every row now and makes room for the whole header.
- **XML files lost `&amp;` and other entities** in text and attributes, so
  `Smith &amp; Sons` was read as `Smith  Sons`. They are read as the
  characters they stand for.
- **Union and Join named a cloud tab after its temporary copy**, a
  meaningless file name in the list of tables. It shows the name of the
  cloud file now.
- **A missing `git` or `aws` program read like a broken repository or wrong
  credentials.** The message now says the program was not found.
