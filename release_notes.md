A release with two new cell types, Time and Duration, Settings sorted by
task, the zoom level always in the status bar, and a round of fixes for
dates and numbers that came out wrong or went missing on save, for error
messages that left out their reason, and for a map fallback that never fell
back.

## Fixes

### Dates, times and numbers

- **Timestamps before 1970 with a fraction of a second opened as text.** In
  Parquet, Arrow, ORC and Avro files, `1969-12-31 23:59:59.5` became an
  unreadable number instead of a date and time. It reads as a timestamp now.
  R data files and chart axis labels put such times one second late, and are
  right now too.
- **Very large whole numbers were lost on save.** A Parquet `UInt64` above
  9,223,372,036,854,775,807 read as a negative number and was saved as an
  empty cell, and a whole-number decimal that large wrapped around the same
  way. Such values now open as text and are written back unchanged.
- **Dates and times written with a `T` were saved as empty cells.** A value
  such as `2024-01-02T03:04:05`, from **Change type** or from a Databricks or
  BigQuery table, was written to Parquet and to the SQL panel as nothing.
  Octa now keeps every date and time as `2024-01-02 03:04:05`, so they save.
- **BigQuery timestamps showed as numbers** such as `1.7036064E9`. They show
  as dates and times now, to the microsecond.
- **Snowflake dates and timestamps showed as numbers.** A `DATE` read
  `18262` and a `TIMESTAMP` a count of seconds. They read as dates and times
  now.
- **Some Parquet and Arrow columns showed internal text.** A pandas
  `category` column showed `DictionaryArray {...}` with every category in
  each cell, list and struct columns showed `ListArray [...]`, and saving
  wrote that text into the file. Each cell shows its own value now, and a
  category column takes the type of its values.

### Error messages

- **Many error messages stopped before the reason.** Opening a file, a sheet
  or a table, Compare, Summary, the Data quality report, File internals,
  Fill missing values, Union, Pivot, Time series, Fuzzy join, Report, the
  Assistant's tools and the MCP server showed only the last step, such as
  "could not read the file", and left out why. They show the whole message
  now, down to the cause.
- **Some error messages could not be copied.** Fourteen more of them can now
  be marked and copied with a right-click, like the rest.
- **A long dialog title ran under the window buttons**, in some languages
  more than others. It is cut short now, and minimise, maximise and close
  stay clear.
- **A `.gz` or `.zst` file too large to unpack sent you to the wrong
  place.** The message named Settings > Files; the limit is under
  **Settings > Performance > Decompression size cap**.

### Zoom

- **The keyboard zoomed harder than the zoom buttons.** Ctrl+Plus and
  Ctrl+Minus changed the size about three times as much as the View menu's
  buttons, and the percentage shown did not match what the keys did. Every
  way of zooming now takes the same step.

### Map

- **Fallback to geometry never fell back.** When map tiles could not be
  loaded, offline or from a wrong tile address, the map stayed blank although
  **Settings > Map > Fallback to geometry** was on. Octa now fetches one tile
  first, and if that fails the map shows the shapes without tiles and the
  status bar says why.

### Assistant

- **Models an earlier release removed sat at the top of the model list.**
  Octa took them for names you had typed into `models.toml`, so
  `claude-sonnet-4-6` or `gemini-3-flash-preview` led above the current
  models. They are removed now, and a default that named one becomes the
  current default.

### Documentation

- **The settings reference had fallen behind the dialog.** It described
  the Assistant with one provider, model and temperature for everything,
  although each model profile has its own. It called **Debug logging**
  "Debug mode" and said it starts a log, while Octa always keeps one and
  the switch only makes it more detailed. **Run analyses on the database**
  was not listed at all, and many settings went by a different name than
  the one the dialog shows. All of that matches the dialog now, in the
  online documentation and in **Help > Documentation**.
- **The settings overview in Help named things that are gone.** It still
  mentioned a warning about line breaks that was removed, listed the wrong
  settings under Performance, and had nothing on API endpoints or
  Diagnostics.
- **Ctrl + mouse wheel does not zoom the window,** although the
  documentation said so. That sentence is gone; in charts, maps and the
  timeline Ctrl + mouse wheel still zooms the picture.
- **The decompression limit was said to live under Settings > Files.** It
  is under **Settings > Performance**.

### Security updates

- **Updated libraries close known issues.** A crafted Parquet file could
  make Octa run out of memory while reading its footer, and the SSH client
  behind SFTP connections and jump hosts received security fixes.

## What's new

### Times and durations

Clock times without a date (`14:30:00`) and amounts of time (`30:15:00`,
`-00:30:00`) are cell types of their own now, **Time** and **Duration**.
CSV and the other text formats detect them when a file opens. Parquet,
Arrow, DuckDB, Excel, ODS, Avro, TOML, SPSS and SAS files read them typed,
and so does every database connection except Oracle, with MySQL's `TIME` as
a duration. They sort, compare (`clock_in > 08:30`), sum, chart and convert
like numbers.

**Date/Time calculation** gains the duration between two times or dates
(night shifts past midnight included), adding a duration, and durations to
numbers and back. A MySQL `TIME` outside its range is refused rather than
cut short by the server. Three new settings come with it, listed below, and
the new page **Times and durations** in the documentation covers every
format and database.

### Zoom in the status bar

The zoom level now always shows at the right end of the status bar, 100%
included, with a **-** and a **+** button beside it. Click either to zoom by
one step, or click the percentage to go back to 100%. A step is 5% as
before, and **Settings > Appearance > Zoom step** sets it anywhere from 1%
to 25%, for the keys and the View menu too.

### Settings sorted by task

Settings had a **Files** section that mixed opening with saving, and a
**File-Specific** section that held whatever did not fit elsewhere. Both are
gone. **Opening files** now holds everything that decides what happens when
a file opens, from recent files and the last session to date detection,
trimming, header cleanup, the repair offer, Excel sheets and the large-file
notice. **Saving files** holds auto-save, the Save As questions, recipes,
earlier versions and the write options. The Raw view and Notebook options
joined the editor settings, now called **Search & text views**, the
read-only notice moved to **Window**, and **Performance** keeps only limits
and caps. Every setting keeps its value: only its place in the dialog
changed. Messages that tell you where to switch something off, such as the
date and trimming banners, name the new sections, and the documentation
follows the new order.

### Newer models in the Assistant

The model lists offer Claude Opus, Sonnet and Haiku 5.5, GPT-6.1 Terra,
GPT-6.1 Sol, GPT-6 Sol and Luna, Gemini 4 Argon, and the latest open-weight
models (Kimi K3.1 Pro, MiniMax M3.1 Flash, Nemotron 3.5 Super VL, Muse Spark
Open, DeepSeek V4.1 Flash, GLM-5.3, Qwen3.8), newest first. Claude models go
by version, largest first within one (Opus, Sonnet, Haiku 5.5), and Claude
Opus 4.7 joins the list. Haiku 5.5 and GPT-6.1 Terra are the new defaults,
both cheaper than the ones they replace. Gemini 3.7 Flash, 3.5 Flash and 2.5
Pro left the list, because Google deprecated them or closed them to new
projects, and so did Qwen3.7 Plus, which has no open weights. Pro mode works
on GPT-6 models too.

### New and changed settings

New:

- **Settings > Appearance > Zoom step**, 5% by default. How much one zoom
  step changes the size, anywhere from 1% to 25%, for Ctrl+Plus and
  Ctrl+Minus, the View menu and the status bar's buttons.
- **Settings > Opening files > Detect times and durations on open**, on by
  default. A text column holding only clock times (`14:30`) opens as a Time
  column, one holding amounts of time (`30:15:00`, `PT2H`) as a Duration
  column. Switch it off to keep them as text.
- **Settings > Table View > Duration format**, Clock by default. Shows
  durations as `30:15:00`, `1 day 06:15:00` or `PT30H15M`, and writes CSV
  and other text files the same way. Octa reads all three back, and chart
  axes always use the clock form.
- **Settings > Saving files > Write options > Parquet > Durations as
  integer microseconds**, off by default. Writes Duration columns as whole
  microseconds, which every tool reads the same way. Left off, they are
  Arrow durations, read as durations by pyarrow, polars and Octa and as
  integers by DuckDB and Spark. Save As to Parquet says so when it applies.

Changed:

- Every setting from **Files** and **File-Specific**, and three from
  **Performance**, moved to the section of its task (see Settings sorted by
  task). The read-only mode notice is under **Window** now.
- **Settings > Performance > Decompression size cap** now applies to the
  command line and the MCP server too, which used to stop at 4 GB whatever
  it said. The new `--max-decompressed MB` (or `unlimited`) overrides it for
  one run or one server.
- **Settings > Map > Fallback to geometry** now does what it says (see
  Fixes).
- **Settings > Chat / Assistant**: a profile that never chose a model now
  uses the new defaults, Claude Haiku 5.5 and GPT-6.1 Terra.
- The command line and the MCP server read the new time settings from
  `settings.toml` as well, so a file written there matches one written from
  the window.
