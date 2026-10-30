# Man Page Reference

The full man-page reference for `octa(1)`. The source of truth is
[`docs/cli/octa.1.adoc`](https://github.com/thorstenfoltz/octa/blob/master/docs/cli/octa.1.adoc)
(AsciiDoc); this page mirrors that content as Markdown for the
docs site.

On Linux (with the man page installed) `man octa` gives you the
same content at a terminal. See
[Installation](../getting-started/installation.md) for how to get
the man page on disk.

## Name

**octa**: multi-format tabular data viewer, editor, CLI tool, and
MCP server.

## Synopsis

```text
octa [FILE...]
octa ACTION [OPTIONS]
octa --mcp [--mcp-read-only] [--mcp-tools LIST] [--mcp-without LIST]
```

## Description

**octa** is a desktop application for viewing and editing tabular
data files. It opens Parquet, CSV, JSON, SQLite, DuckDB, Excel and
many more, over 30 formats in all, in a spreadsheet-like view with
sorting, filtering, full-text search, inline editing, SQL queries, and
file comparison.

Run without an action flag, it launches the graphical interface and
opens each *FILE* in a tab. Run with one action flag, it performs that
action, prints the result and exits. The actions are listed below,
grouped by task. Only one action runs per invocation. Each action takes
the files shown in its usage line; any other trailing *FILE* is ignored
with a warning.

A *FILE* may be a local path, `-` for standard input, a plain
`http://` or `https://` URL, or a cloud object URL (`s3://bucket/key`,
`az://container/blob`, `gs://bucket/key`, `sftp://user@host/path`,
`davs://host/path`). A URL or cloud object is downloaded to a
temporary file and read as usual. Cloud credentials come from a saved
cloud connection covering the URL, otherwise from the ambient chain
(`AWS_*` environment variables, a cached SSO session, `az login`,
Google application default credentials). Cloud objects are read-only
here: output still goes to a local path. See
[Pipelines](#pipelines) and [Reading from a URL](#reading-from-a-url)
under Examples.

`octa --help` prints the flag list, and `octa --help-all` adds worked
examples for every action.

## Actions

Each entry shows the action's usage line, what it does, and the
options that belong to it alone. Options several actions share (`-f`,
`--rows`, `--out-dir`, `--table` and others) are described under
[Common options](#common-options).

### Look at a file

`--schema FILE`
:   Print the column schema of *FILE* as a two-column table (column
    name, data type). For streaming formats (Parquet, CSV, TSV) the
    reader loads the initial-row batch (**2,000,000 rows** by default)
    and projects the schema from that. See [`octa --schema`](schema.md).

`--head FILE [-n N] [--sort-by COLS [--sort-desc COLS] [--natural]]`
:   Print the first *N* rows of *FILE* to standard output. *N* defaults
    to 20 and is set with `-n` / `--lines`. For streaming formats the
    reader stops at the initial-load cap and *N* is a slice off that,
    so this is fast on large files. With `--sort-by` the rows are
    sorted before the first *N* are taken. See [`octa --head`](head.md).

`--tail FILE [-n N]`
:   Print the last *N* rows of *FILE*. *N* defaults to 20 and is set
    with `-n` / `--lines`. Streaming formats load with the standard row
    cap, so the tail reflects the end of the loaded window; raise
    `--rows` to tail the true end of a very large file. See
    [`octa --tail`](tail.md).

`--sample FILE [-n N] [--seed N]`
:   Print a random *N*-row sample of *FILE* (without replacement,
    original row order preserved). *N* defaults to 20 (`-n` /
    `--lines`). The sample is reproducible for a given `--seed`
    (default `0`). See [`octa --sample`](sample.md).

`--describe FILE [--table NAME] [--sample-rows N] [--deep]`
:   Print a one-shot orientation snapshot of *FILE*: format, file size,
    row count, column schema, and a small sample of rows. The `--table`
    flag picks a specific table on multi-table sources. See
    [`octa --describe`](describe.md).

    `--sample-rows N`
    :   Number of preview rows (default `5`, clamped to `100`).

    `--deep`
    :   Also report how the file is physically written: row groups,
        compression codec, encodings and per-column statistics,
        followed by one row per column per row group. Plain-language
        hints about a poor layout (very small row groups, missing
        statistics, no compression) go to standard error, so a piped
        run stays parseable. Parquet reports full detail; other formats
        report their size and state that they expose no inspectable
        structure. Place it after the file: `--describe FILE --deep`.
        See [File Internals](../usage/file-internals.md).

### Analyse

`--report OUT.html FILE [--report-sample N] [--report-sections LIST]`
:   Write an HTML profiling report for *FILE* to *OUT.html*: per-column
    statistics, distribution charts, the most common values and a
    correlation matrix. The document is self-contained (inline CSS,
    inline SVG, no JavaScript) and fetches nothing, so it can be mailed
    or published as-is. See [`octa --report`](report.md).

    `--report-sample N`
    :   Profile a random sample of *N* rows instead of every row. The
        report states that it sampled.

    `--report-sections LIST`
    :   A comma-separated subset of `stats`, `distributions`,
        `top_values` and `correlation` (default: all four). An unknown
        name is an error rather than a silent omission.

`--unique-columns FILE [--table NAME] [--max-combo N]`
:   Find columns (and optional small combinations) whose values are
    unique across *FILE*. Useful for primary-key reconnaissance on
    undocumented data. See [`octa --unique-columns`](unique-columns.md).

    `--max-combo N`
    :   Also test column pairs (`2`) or triples (`3`). Default `1`,
        clamped to `[1, 3]`.

`--shapes FILE --shapes-column COL [--shapes-exact]`
:   What a column's values look like, digits folded to `9`, capitals to
    `A`, other letters to `a`, punctuation kept, and a run of letters
    of one kind written once (`Anna Schmidt` is `Aa Aa`). One row per
    shape, most common first, with a count and an example. See
    [`octa --shapes`](shapes.md).

    `--shapes-column COL`
    :   Column to shape. Required.

    `--shapes-exact`
    :   Shape letter by letter (`Aaaa Aaaaaaa`) instead of by word.

`--lookups FILE [--lookups-min N]`
:   Hidden lookup tables: columns that always follow another column.
    One row per key and following column, with its consistency,
    conflicting keys and breaking rows. See [`octa --lookups`](lookups.md).

    `--lookups-min N`
    :   Share of rows (0 to 1) that must agree with their key. Default
        `0.95`.

`--outliers FILE [--outlier-method M] [--outlier-cols COLS] [--outlier-k K]`
:   Flag numeric outlier cells in *FILE* and print their coordinates.
    See [`octa --outliers`](outliers.md).

    `--outlier-method M`
    :   `iqr` (default) or `zscore`.

    `--outlier-cols COLS`
    :   Comma-separated columns to scan (default: all).

    `--outlier-k K`
    :   Threshold multiplier (default `1.5` for IQR, `3.0` for z-score).

`--detect-pii FILE [--pii-sample N]`
:   Scan *FILE* for likely PII columns (email, phone, IBAN, credit
    card, SSN) and print the findings. See [`octa --detect-pii`](pii.md).

    `--pii-sample N`
    :   Rows sampled per column (default `500`).

`--compare-distributions FILE --dist-column COL [--dist-file-b FILE] [--dist-column-b COL]`
:   Do two columns look like the same population? Numbers are compared
    with a two-sample Kolmogorov-Smirnov test, which looks at the whole
    shape rather than one summary of it; anything else is compared as
    categories with a chi-square test. Prints a two-column `field` /
    `value` table led by a plain-language `headline` (the answer), then
    the `verdict` (`same` or `different` at p = 0.05), the test, its
    statistic and p-value. When the test does not apply (fewer than 20
    usable values, or more than 50 distinct ones), a `result` row says
    why instead of a verdict. Rare categories are pooled into one
    `(other)` bucket rather than dropped, and nulls are left out.
    `--table-a` / `--table-b` pick the table on multi-table sources.
    See [`octa --compare-distributions`](compare-distributions.md).

    `--dist-column COL`
    :   The column to compare. Required.

    `--dist-file-b FILE`
    :   A second file. Omit it to compare two columns of the first.

    `--dist-column-b COL`
    :   The second column. Defaults to `--dist-column`.

`--forecast FILE --forecast-x COL --forecast-y COL [--forecast-periods N] [--forecast-season N]`
:   Forecast one column over time (Holt-Winters) with 80% and 95%
    ranges. See [`octa --forecast`](forecast.md).

    `--forecast-x COL`
    :   Time column: dates, date-times or numbers, evenly spaced.
        Required.

    `--forecast-y COL`
    :   Column of numbers to forecast. Required.

    `--forecast-periods N`
    :   How many periods ahead. Default `12`.

    `--forecast-season N`
    :   Season length in points. Default: read from the spacing.

`--resample COL FILE --value-cols COLS [--interval I] [--agg A] [--group-by COLS]`
:   Group *FILE* into time buckets: one row per `--interval` of *COL*.
    The time column is cast with `TRY_CAST`, so a row that will not
    parse buckets as NULL instead of failing the whole run. See
    [`octa --resample`](timeseries.md).

    `--value-cols COLS`
    :   The columns to aggregate. Required.

    `--interval I`
    :   `minute`, `hour`, `day` (default), `week`, `month`, `quarter`
        or `year`.

    `--agg A`
    :   `sum` (default), `mean`, `min`, `max`, `count`, `first` or
        `last`.

    `--group-by COLS`
    :   Extra grouping columns, giving one series per combination.

`--rolling COL FILE --window N --order-by COL [--agg A] [--partition-by-cols COLS]`
:   Add a rolling aggregate of *COL* to *FILE*. See
    [`octa --rolling`](timeseries.md).

    `--window N`
    :   Rows in the frame, including the current row.

    `--order-by COL`
    :   Column that orders the frame. Required: a rolling aggregate over
        unordered rows is meaningless.

    `--agg A`
    :   `mean` (default), `sum`, `min`, `max`, `count`, `first` or
        `last`.

    `--partition-by-cols COLS`
    :   Restart the frame per group. Spelled that way because
        `--partition-by` is the split-into-files action.

`--overlaps FILE [--overlaps-start COL] [--overlaps-end COL] [--overlaps-lane COL] [--overlaps-label COL]`
:   Rows whose time spans overlap inside a lane (two bookings of one
    room). One row per pair on stdout; **exits 1 when any is found**.
    Spans that only touch do not overlap. Without `--overlaps-start` /
    `--overlaps-end` the first two date columns are used. See
    [`octa --overlaps`](overlaps.md).

    `--overlaps-start COL`, `--overlaps-end COL`
    :   Start and end columns. No end makes every row a point.

    `--overlaps-lane COL`
    :   Only rows with the same value in *COL* can overlap.

    `--overlaps-label COL`
    :   A column shown beside each row of a pair.

`--cell-history FILE --history-column COL (--history-key COLS --history-value VALUES | --history-row N) [--history-depth N]`
:   The commits that changed one cell of a file in a Git repository,
    newest first, following the row by key and the file through
    renames. See [`octa --cell-history`](cell-history.md).

    `--history-column COL`
    :   Column of the cell. Required.

    `--history-key COLS`
    :   Comma-separated key column(s) that identify the row.

    `--history-value VALUES`
    :   Comma-separated values of the row in each key column.

    `--history-row N`
    :   1-based row number, when there is no key.

    `--history-depth N`
    :   How many commits to read, newest first. Default `50`.

`--relationships DIR [--recursive]`
:   Rank the likely relationships between the tables in *DIR* and print
    them best first: which column of which table lines up with which
    column of which other table. Names take no part in the ranking; the
    score comes from how much the values overlap, weighted by how
    distinct each side is, so a status column cannot outrank a real
    key. Each row also carries `orphans`, the number of distinct values
    on the left with no partner on the right, which is what separates
    two candidates that overlap identically. Reads values, so it is
    capped at 30 files and at 10,000 rows per table. Unreadable files
    are noted on stderr. Always exits `0`: this is a report, not a
    gate. Add `--recursive` to walk subdirectories. See
    [`octa --relationships`](relationships.md).

### Compare and check

`--diff FILE_A FILE_B [--diff-mode MODE] [--diff-on COLS]`
:   Compare two files, or with `--diff-db` a file and a live database
    table. `--diff-mode` selects the strategy; the output is a table
    tagged by a leading `status` column, and a summary line (per-mode
    counts) is written to standard error. Complements
    `--compare-schemas`, which diffs only the column metadata. See
    [`octa --diff`](diff.md).

    `--diff-mode MODE`
    :   Comparison strategy (default `set`): `set` compares rows by
        whole-row content (every column, positionally) and prints rows
        unique to each side (`only_in_a` / `only_in_b`); the files
        should share the same column order. `ordered` lines up row *i*
        of A with row *i* of B, prints matched rows that differ as
        `changed` (with a `changed_columns` column) plus any trailing
        `only_in_a` / `only_in_b` rows. `join` matches rows on the
        `--diff-on` key column(s) and prints added (`only_in_b`),
        removed (`only_in_a`), and `changed` rows.

    `--diff-on COLS`
    :   Key column(s) for `--diff-mode join`, comma-separated (matched
        by name). Required when the mode is `join`; ignored otherwise.

    `--diff-tolerance N`
    :   Numbers within this absolute difference count as the same.
        Ordered and join modes only; set mode refuses it.

    `--diff-tolerance-pct P`
    :   Numbers within this percentage of the larger value count as the
        same. A pair passes if it is within either number tolerance.

    `--diff-time-tolerance SECS`
    :   Dates and times at most this many seconds apart count as the
        same. A date counts as midnight.

    `--diff-ignore-case`
    :   Compare text without regard to upper and lower case, join keys
        included.

    `--diff-ignore-spaces`
    :   Compare text trimmed, with runs of spaces as one, join keys
        included.

    `--diff-ignore-columns COLS`
    :   Columns to leave out of the comparison, comma-separated or
        repeated. They still appear in the output.

    `--diff-db CONN`
    :   Compare against a live database table instead of a second file.
        *CONN* names a saved database connection. With this set,
        `--diff` takes a single file: that file is side A and the table
        is side B, so `only_in_a` means "in the file, not the table".
        Requires `--diff-db-table`. The table is read under the usual
        row cap. See [Compare with a Database
        Table](../usage/compare-with-database.md).

    `--diff-db-table TABLE`
    :   Table to compare against on the `--diff-db` connection, as
        *SCHEMA.TABLE* or *CATALOG.SCHEMA.TABLE* (the three-part form is
        for Trino, Snowflake, Databricks and BigQuery). An unqualified
        name uses the connection's own database.

`--compare-schemas FILE_A FILE_B [--table-a NAME] [--table-b NAME]`
:   Diff the column schemas of two files. Prints a four-column table
    (`status` / `column` / `type_a` / `type_b`). `status` is one of
    `common`, `only_in_a`, `only_in_b`, `type_mismatch`. Matching is by
    exact, case-sensitive column name. For multi-table sources, the
    table to read from each side is selected via `--table-a` /
    `--table-b`. See [`octa --compare-schemas`](compare-schemas.md).

`--compare-folders FOLDER FOLDER... [--no-recursive]`
:   Compare two or more folders file by file. Each *FOLDER* is a local
    path or a cloud URL (`s3://`, `az://`, `gs://`, `sftp://`,
    `davs://`). Files are matched by their path inside each folder;
    copies of different sizes are different without being read, and
    copies of equal size are compared by a BLAKE3 hash of their
    contents. Prints one row per file with a `status` of `same`,
    `different` or `missing`, the folders that have it and lack it,
    and the size, modified time and hash per folder. Each folder is
    listed up to the file limit from the settings
    (`folder_compare_max_files`, default 100,000); a folder cut short
    is named on stderr. Exit code is `0` when every file is the same
    everywhere and `1` when anything differs, is missing, or a folder
    hit the limit. See [`octa --compare-folders`](compare-folders.md).

    `--no-recursive`
    :   Compare only the top level of each folder.

`--drift-report FILE_A FILE_B [--fail-on SPEC]`
:   Compare two versions of the same dataset and report how it moved.
    Columns are matched by name: one present on a single side is
    reported as added or removed, and every shared column gets its null
    rate, distinct count and, when numeric, its minimum, maximum and
    mean compared. Columns with few enough distinct values also list
    the category values that appeared and vanished. Every figure comes
    from the same Summary pass the **Analyse -> Summary** tab uses, so
    the two can never disagree. The report goes to stdout; the row
    counts, added and removed column names and the pass/fail summary go
    to stderr, so a pipe stays parseable. Exit code is `0` unless
    `--fail-on` was given and a gate was breached. See
    [`octa --drift-report`](drift-report.md).

    `--fail-on SPEC`
    :   Turn the report into a CI gate. *SPEC* is a comma-separated
        list of *metric*:*change* pairs, for example
        `null_rate:0.05,rows:0.1`, where *change* is the largest
        relative move that still passes. A metric that never appears in
        the report is simply never applied. A baseline of zero that
        moved at all counts as an unbounded change, so a null rate going
        from `0` to `0.5` breaches any gate. Metric names are `rows`,
        `null_rate`, `distinct_count`, `min`, `max` and `mean`.

`--schema-drift DIR [--recursive] [--ignore-case]`
:   Scan *DIR* and report which files disagree about their columns.
    Files are grouped by schema, largest group first, so the answer to
    a folder of 500 parts is "497 look like this, 3 look like that".
    The report table goes to stdout; the per-variant file lists, any
    unreadable files and the drifting column names go to stderr, so a
    pipe stays parseable. Exit code is `0` when every file agrees and
    `1` otherwise, which is CI-pipeable. Add `--recursive` to walk
    subdirectories (depth 8) and `--ignore-case` to treat column names
    differing only in case as the same column. See
    [`octa --schema-drift`](schema-drift.md).

`--validate-schema FILE --expect-schema SCHEMA_FILE [--table NAME]`
:   Check *FILE*'s column schema against a JSON Schema. Exit code is
    `0` on a clean match, `1` otherwise, which is CI-pipeable. Schemas
    produced by `--export-schema -t json-schema` round-trip through
    this command. See [`octa --validate-schema`](validate-schema.md).

    `--expect-schema SCHEMA_FILE`
    :   Path to the expected JSON Schema. Required.

`--check FILE --rules RULES.toml`
:   Check *FILE*'s values against a rules file and report which rules
    failed. The report goes to stdout with one row per failing rule,
    its failure count and up to three offending values; the summary
    line goes to stderr. Exit code is `1` on any violation **and** on
    any rule that could not run, because a rules file whose columns
    have since been renamed would otherwise report a clean run over
    checks it silently skipped. The same rules file is written and read
    by the **Data -> Data validation...** dialog. See
    [`octa --check`](check.md).

    `--rules FILE`
    :   The rules file. Required. It is TOML, one `[[rule]]` table per
        check, naming its column and one of the kinds `not_null`,
        `unique`, `range` (with `min` and `max`), `regex` (with
        `pattern`) or `max_length` (with `max_length`). A rule without
        a `column` applies to every column. Example:

        ```toml
        [[rule]]
        column = "order_id"
        kind = "unique"

        [[rule]]
        column = "amount"
        kind = "range"
        min = 0
        ```

`--check-references PARENT --parent-column COL [--child-file FILE] --child-column COL`
:   List the child rows whose foreign key has no matching parent, one
    row per offending value (capped at 500 values; the counts in the
    summary on stderr stay exact). **Exits 1 when there are any**, so
    it works as a CI gate. A null or empty key is not an orphan: in
    every relational database it means "no parent", so those rows are
    counted separately and do not fail the gate. Keys are compared as
    trimmed text, so `1` from a CSV matches `1` from a database.
    `--table-a` / `--table-b` pick the table on multi-table sources.
    See [`octa --check-references`](check-references.md).

    `--parent-column COL`
    :   The parent's key column. Required.

    `--child-file FILE`
    :   The child file. Omit it for a self-reference, such as a
        `manager_id` pointing at `id` in the same table.

    `--child-column COL`
    :   The child's foreign-key column. Required.

### Clean and transform

`--dedupe FILE [--dedupe-on COLS] [--dedupe-keep WHICH]`
:   Remove duplicate rows from *FILE* and print the result to stdout.
    See [`octa --dedupe`](dedupe.md).

    `--dedupe-on COLS`
    :   Comma-separated key columns. Without it the whole row is the
        key.

    `--dedupe-keep WHICH`
    :   Which occurrence survives: `first` (default) or `last`.

`--impute COL=STRATEGY FILE`
:   Fill missing or empty cells of a column of *FILE*. Repeatable.
    Strategies: `mean`, `median`, `mode`, `ffill`, `bfill`,
    `const:VALUE`. See [`octa --impute`](impute.md).

`--anonymize SPEC FILE`
:   Mask or scramble sensitive columns of *FILE* per the JSON *SPEC*
    file and print the sanitised table to stdout. The input file is
    never modified. The spec lists per-column rules (`hash` /
    `partial_mask` / `redact` / `fake`) plus an optional shared `salt`;
    columns are named. The same value plus the same salt always maps
    the same way, so duplicates stay linked and a re-run re-joins to an
    earlier export. See [`octa --anonymize`](anonymize.md).

`--hash-columns COL,COL,... FILE [--hash-algo ALGO] [--hash-delimiter TEXT] [--hash-null TEXT] [--hash-trim] [--hash-upper] [--hash-name NAME]`
:   Append a column holding a hash of the named columns and print the
    table to stdout. The input file is never modified. The columns are
    joined in the order given, with the delimiter between them, and
    every value is turned into text first, whatever its type. The hash
    is lowercase hex. See [`octa --hash-columns`](hash-columns.md).

    `--hash-algo ALGO`
    :   `md5` (default), `sha256` or `sha512`.

    `--hash-delimiter TEXT`
    :   Put between the values (default `|`). May be empty:
        `--hash-delimiter ""`.

    `--hash-null TEXT`
    :   Stands in for a NULL cell (default empty).

    `--hash-trim`
    :   Strip whitespace from each value before hashing it.

    `--hash-upper`
    :   Upper-case each value before hashing it.

    `--hash-name NAME`
    :   Name of the new column (default `hash_<columns>`). It must not
        exist yet.

`--recipe RECIPE FILE [--recipe-out FILE]`
:   Replay a recipe (a `.ocp` file, short for Octa reCiPe, recorded in
    the GUI) on *FILE*. Every step runs by column name. When any step
    cannot run, nothing is written, the skipped steps go to stderr and
    the exit code is `1`. See [`octa --recipe`](recipe.md).

    `--recipe-out FILE`
    :   Write the result to *FILE* instead of stdout.

`--test-data FILE... [--test-data-rows N] [--seed N] [--test-data-out PATH] [--test-data-rename-categories]`
:   Generate new rows shaped like each *FILE*: the same columns,
    numbers and dates from the real spread, fake names, emails, IBANs
    and IDs, empty cells at the real rate. Several files are generated
    together so links between them still join. The plan goes to stderr,
    with a note for every small category column that keeps its real
    values. See [`octa --test-data`](test-data.md).

    `--test-data-rows N`
    :   Rows per table (default: as many as the real one).

    `--test-data-out PATH`
    :   Output file for one input, or an existing folder for several
        (written as `<name>_test.<ext>`). Without it, one table goes to
        stdout.

    `--test-data-rename-categories`
    :   Replace real category values with `value_1`, `value_2`, ...

### Combine and split

`--union FILE --union-file FILE [--union-drop COL] [--union-cast COL=TYPE] [--union-ignore-case]`
:   Stack *FILE* plus every `--union-file` into one table (at least
    two files). See [`octa --union`](union.md).

    `--union-file FILE`
    :   Another source. Repeatable.

    `--union-drop COL`
    :   Column to omit from the output. Repeatable.

    `--union-cast COL=TYPE`
    :   Override a column's target Arrow type. Repeatable.

    `--union-ignore-case`
    :   Treat column names differing only in case as one column. Off by
        default; the first spelling encountered names the output
        column.

`--join FILE --join-file FILE --join-on COLS [--join-type TYPE]`
:   Join *FILE* plus every `--join-file` on the `--join-on` key(s). See
    [`octa --join`](join.md).

    `--join-file FILE`
    :   Another source. Repeatable.

    `--join-on COLS`
    :   Key column(s), comma-separated or repeated. Required.

    `--join-type TYPE`
    :   `left` (default), `inner`, `right`, `full`, `semi` (rows of the
        first file with a partner), `anti` (rows without one) or `asof`
        (the last `--join-on` key matches the nearest earlier value).

`--fuzzy-join FILE --fuzzy-join-file FILE --fuzzy-on LEFT=RIGHT [--fuzzy-method NAME] [--fuzzy-threshold N] [--fuzzy-block LEFT=RIGHT] [--fuzzy-join-type TYPE] [--fuzzy-max-rows N]`
:   Join *FILE* to each `--fuzzy-join-file` on how **similar** the
    values are rather than on exact equality, for tables that name the
    same thing differently (`Mueller GmbH` against `Mueller Gmbh.`).
    Each left row keeps its single best partner. The output gains
    `match_score_N` and `ambiguous_N` per step; `ambiguous` means the
    runner-up scored nearly as well, so that match is the one to check.
    The per-step report goes to stderr. See
    [`octa --fuzzy-join`](fuzzy-join.md).

    `--fuzzy-join-file FILE`
    :   The table to join against. Repeat it to join several in turn.

    `--fuzzy-on LEFT=RIGHT`
    :   The columns to compare. May be repeated, in which case the
        scores are averaged.

    `--fuzzy-method NAME`
    :   `edit_ratio` (default), `jaro_winkler` or `token_set`.

    `--fuzzy-threshold N`
    :   The minimum average score in `0.0..=1.0` (default `0.85`).

    `--fuzzy-block LEFT=RIGHT`
    :   Compare only rows that agree exactly on these columns, which is
        what makes a large join feasible.

    `--fuzzy-join-type TYPE`
    :   `inner`, `left` (default), `right` or `full`.

    `--fuzzy-max-rows N`
    :   Cap on the rows considered per side (default `20000`).

`--spatial-join FILE --spatial-layer FILE... [--spatial-op inside|nearest] [--within-km N]`
:   Join by location: each point of *FILE* gets the columns of the
    layer polygon it lies in, or of the nearest layer point with its
    distance. See [`octa --spatial-join`](spatial-join.md).

    `--spatial-layer FILE`
    :   A table to join against. Repeat it for several layers. At least
        one is required.

    `--spatial-op inside|nearest`
    :   The operation. Default `inside`.

    `--within-km N`
    :   With `--spatial-op nearest`: points farther than this stay
        empty.

`--merge FILE FILE... [--merge-original FILE] [--merge-key COLS] [--merge-prefer N] [--merge-out FILE] [--merge-format EXT]`
:   Merge two or more versions of a table, per row and per cell. With
    `--merge-original`, a change made in one version is taken and only
    different changes to one cell, or a row deleted in one version and
    edited in another, conflict; without it, every cell where the
    versions differ conflicts. Rows are matched by `--merge-key`, or by
    position without one. While a conflict is open nothing is written,
    the conflicts are printed on stdout and the exit code is `1`, which
    makes it usable as a git merge driver: `octa --merge %A %B
    --merge-original %O --merge-key id --merge-out %A --merge-format %P`.
    See [`octa --merge`](merge.md).

    `--merge-original FILE`
    :   The table every version was edited from. Optional.

    `--merge-key COLS`
    :   Key column(s), comma-separated or repeated.

    `--merge-prefer N`
    :   Settle every conflict in favour of version *N* (1 = the first
        file given to `--merge`).

    `--merge-out FILE`
    :   Write the result to *FILE* instead of stdout.

    `--merge-format EXT`
    :   Format of the files, for files without an extension. An
        extension (`csv`) or a file name to take it from (git's `%P`).

`--harmonise-schema DIR (--out-dir DIR | --combine --out FILE) [--target-file FILE] [--recursive] [--ignore-case] [--overwrite]`
:   Rewrite every file in *DIR* to one common set of columns, writing
    harmonised copies into `--out-dir`. The originals are never
    modified. The target schema is the shape most files already have,
    or the schema of `--target-file` when given. A column missing from
    a file is added as nulls; a column not in the target is dropped and
    named in the report. A file whose values cannot be cast to a target
    type is **refused** rather than written with nulls in place of
    those values, because a harmonised folder of silently emptied cells
    looks clean and is not. Two inputs that would write the same output
    name are both refused rather than one being renamed. The report
    goes to stdout; refusals and dropped columns go to stderr. Exit
    code is `0` when nothing was refused and `1` otherwise. Add
    `--recursive`, `--ignore-case`, and `--overwrite` to replace
    existing files in the output folder. See [Harmonise
    Schemas](../usage/harmonise-schemas.md).

    `--target-file FILE`
    :   Take the target schema from this file instead of the shape most
        files already have.

    `--combine`
    :   Fold the folder into ONE file, written to `--out FILE`, instead
        of writing a harmonised copy of each. A `source_file` column
        says which file every row came from (suffixed, as
        `source_file_2`, if the data already has one). Nothing is cast,
        so nothing is refused: columns only some files have are kept
        and left empty for the others, as `--union` does. A file that
        cannot be read is named, skipped and counted. Asking for it
        without `--out` is refused rather than guessed at.

`--partition-by COL --out-dir DIR FILE [--partition-format EXT] [--partition-layout LAYOUT]`
:   Split *FILE* into one file per distinct value of *COL*, written
    into `--out-dir`. Combines with `--split-rows` / `--split-size` to
    cap each piece as well. See [`octa --partition-by`](partition.md).

    `--partition-format EXT`
    :   Output extension (default: the source's).

    `--partition-layout LAYOUT`
    :   How the pieces are named. `flat` (default) writes
        `<value>.<ext>` side by side; `folder` writes
        `<value>/part-0001.<ext>`; `hive` writes
        `<column>=<value>/data.<ext>`; `hive-parts` writes
        `<column>=<value>/part-0001.<ext>`. All four hold the same rows
        and all four reopen as one table, since the partition column is
        written into every file. Only `flat` loses information in the
        name: it sanitises and lowercases the value into a file stem, so
        values differing only in case or punctuation collide and gain
        `_2`, `_3`. The folder layouts keep the value as it is,
        replacing only characters that cannot appear in a path. `hive`
        is what Spark, Athena, DuckDB and pandas expect from a
        partitioned dataset.

`--split-rows N | --split-size SIZE --out-dir DIR FILE [--partition-by COL]`
:   Split *FILE* into pieces of at most *N* rows or *SIZE* bytes each,
    written into `--out-dir`. Works alone (splits the whole table) or
    with `--partition-by` (splits each group). Every file then gets a
    part number: `name_part_0001.ext`, or `part-0001.ext` inside a
    folder. See [`octa --partition-by`](partition.md).

    `--split-rows N`
    :   At most *N* rows per output file.

    `--split-size SIZE`
    :   At most *SIZE* per output file, e.g. `25MB`, `500KB`, `1GB`
        (1 MB = 1,000,000 bytes; `MiB` for 1,048,576). Combines with
        `--split-rows`. Files land a little under the limit.

`--to-workbook OUT.xlsx FILE...`
:   Write every *FILE* into one Excel workbook, one worksheet per file.
    Sheet names come from the file stems and are corrected to Excel's
    rules automatically: at most 31 characters, no forbidden
    punctuation, and duplicates numbered `_2`, `_3`. Needs at least two
    inputs; for a single file use `--convert`. Prints a headerless
    `input<TAB>sheet<TAB>rows` listing. Every input is read before
    anything is written, so an unreadable file fails the run instead of
    leaving a half-written workbook. See [Saving](../usage/saving.md).

### Convert and SQL

`--convert IN OUT [--sort-by COLS] [--password-env VAR] [--compression CODEC] [--row-group-size N]`
:   Convert *IN* to *OUT*. Both formats are inferred from each path's
    extension and routed through the shared format registry. Read-only
    output formats (SAS, R datasets, HDF5, NetCDF, EPUB, GeoJSON) are
    rejected with a clear error. Conversion is bounded by the
    initial-load cap (2,000,000 rows by default); pass `--rows all` to
    convert the full file. *OUT* may be `-` for standard output, with
    `--to` naming the format, since a pipe has no name. See
    [`octa --convert`](convert.md).

`--batch-convert FILE... --out-dir DIR --to EXT [--overwrite] [--password-env VAR]`
:   Convert every *FILE* into `--out-dir` as `--to EXT`. Output names
    are `<stem>.<ext>`; two inputs sharing a stem get `_2`, `_3`
    suffixes in input order, so a run cannot overwrite its own earlier
    output. Existing outputs are skipped unless `--overwrite` is given.
    One failed file does not stop the run. Output is a headerless
    `input<TAB>output<TAB>status` listing on stdout, with per-file
    errors and a summary on stderr. The exit code is `1` when any file
    failed. See [`octa --batch-convert`](batch-convert.md).

`--sql FILE -q QUERY [--sql-table NAME=PATH ...] [--sql-attach ALIAS=PATH ...] [--sql-write-to PATH --sql-write-table TABLE [--sql-write-schema SCHEMA] [--sql-write-mode MODE]] [--stream] [--explain]`
:   Run a SQL query against *FILE*, which DuckDB sees as a table called
    **data**. Extra tables and whole DuckDB / SQLite databases can be
    added, so a single invocation can JOIN across formats, and the
    SELECT result can be persisted to a DuckDB or SQLite file.
    Mutations (INSERT / UPDATE / DELETE) on `data` itself do **not**
    persist back to *FILE*, since the in-memory DuckDB connection is
    discarded at exit. Use `--convert` on the post-mutation output if
    you need to persist. See [`octa --sql`](sql.md).

    `-q QUERY`, `--query QUERY`
    :   The SQL query. Always reference the file's data as the table
        **data**.

    `--sql-table NAME=PATH`
    :   Register an extra file as a workspace table named *NAME*. Any
        supported format. Repeatable.

    `--sql-attach ALIAS=PATH`
    :   `ATTACH` a DuckDB or SQLite database under *ALIAS*. Repeatable.
        After attachment every inner table is queryable as
        `alias.schema.tbl` (DuckDB) or `alias.tbl` (SQLite when the
        DuckDB sqlite extension is bundled, otherwise per-table fallback
        registration under `alias__table`).

    `--sql-write-to PATH`
    :   Persist the SELECT result to *PATH* instead of printing it. The
        extension picks DuckDB (`.duckdb`, `.ddb`) or SQLite (everything
        else). Created if missing. Requires `--sql-write-table`.

    `--sql-write-table TABLE`
    :   Target table name for `--sql-write-to`.

    `--sql-write-schema SCHEMA`
    :   Target schema for `--sql-write-to`. DuckDB only; defaults to
        `main`. SQLite has no schemas; pass `main` or leave unset.

    `--sql-write-mode MODE`
    :   `create` (default; errors if the table already exists),
        `replace` (drop and recreate), or `append` (`INSERT` into the
        existing table; column count and order must match).

    `--stream`
    :   Let DuckDB scan the file where it lies instead of loading its
        rows. For a Parquet, CSV or JSON file this lets an aggregate
        cover every row of a file far larger than memory: the primary
        file is registered as a view named `data` rather than as a
        loaded table, so `--rows` no longer bounds what the query sees.
        Every other action needs the rows themselves and says so on
        stderr rather than ignoring the flag. A format DuckDB cannot
        scan also falls back to a normal read with a note. See [Large
        files](../usage/large-files.md).

    `--explain`
    :   Show the query plan instead of running the statement. Prints the
        steps in plain words, numbered in the order the database works
        through them; with `-f json` or `-f csv`, one row per step. One
        statement. The plan is DuckDB's own (the local workspace).
        Nothing is executed. Also works with `--db-query`.

`--export-schema FILE [-t TARGET]`
:   Render *FILE*'s column schema as SQL DDL, a Pydantic model, a
    TypeScript interface, a JSON Schema document, or a Rust struct, and
    print it to standard output. Only the column list is read and no
    rows are serialised. Short form: `-e`. See
    [`octa --export-schema`](export-schema.md).

    `-t TARGET`, `--target TARGET`
    :   One of `postgres` (default), `mysql`, `sqlite`, `databricks`,
        `snowflake`, `pydantic`, `typescript`, `json-schema`, or `rust`.

### Databases

These actions work on a saved database connection, managed in
**Settings -> Databases** or with `--add-connection`. See
[Databases](databases.md). Two options are shared by the group:

`--db CONNECTION`
:   Saved connection name (case-insensitive) or id. Passwords stay in
    the system keyring.

`--db-catalog NAME`
:   Catalog (the top namespace level) for a three-level engine, used by
    `--db-tables`, `--db-write-table` and `--db-export`. Only
    Snowflake, Databricks and BigQuery have one; passing it to any
    other engine is an error. With `--db-tables` and no catalog, the
    catalogs themselves are listed.

`--db-tables --db CONNECTION [--db-catalog NAME]`
:   List every schema and table of the connection as a two-column
    table. On Trino, Snowflake, Databricks and BigQuery, which have a
    catalog level above the schema, the catalogs themselves are listed
    unless `--db-catalog` names one.

`--db-query SQL --db CONNECTION [--explain]`
:   Run one SQL statement on the connection, server-side and in the
    engine's native dialect. SELECT results print in `-f FORMAT`;
    mutations report rows affected and are refused unless the
    connection's "Allow writes" switch is on. Results stop at the
    initial-load row cap; use `--rows` to raise or lift it.

    `--explain`
    :   Show the query plan instead of running the statement, as for
        `--sql`. Works on PostgreSQL, MySQL / MariaDB, Databricks and
        BigQuery (BigQuery: the bytes it would read). Nothing is
        executed.

`--db-write-table SCHEMA.TABLE --db CONNECTION FILE [--db-write-mode MODE] [--db-catalog NAME]`
:   Write *FILE* into the given server table. Refused unless the
    connection allows writes.

    `--db-write-mode MODE`
    :   `create` (default; error if the table exists), `append`, or
        `replace`.

`--db-copy SCHEMA.TABLE --db CONNECTION --db-copy-to CONNECTION [--db-copy-target SCHEMA.TABLE] [--db-copy-target-catalog NAME] [--db-write-mode MODE]`
:   Copy a table from the `--db` connection to another saved
    connection, server to server. Refused unless the target connection
    allows writes.

    `--db-copy-to CONNECTION`
    :   Target connection, a saved connection name or id. Required.

    `--db-copy-target SCHEMA.TABLE`
    :   Target table. Defaults to the source schema and table.

    `--db-copy-target-catalog NAME`
    :   Target catalog on a three-level engine.

    `--db-write-mode MODE`
    :   `create` (default), `append` or `replace`.

`--db-export SCHEMA --db CONNECTION --out-dir DIR [--to EXT] [--db-export-tables T1,T2] [--overwrite]`
:   Export the tables of *SCHEMA* into `--out-dir`, one file per table
    named after it. `--to` picks `parquet` (default), `csv`, `tsv` or
    `jsonl`. Each table is read page by page, so its size does not
    matter. A file that is already there is skipped unless
    `--overwrite`. One failed table does not stop the rest; the exit
    code is 1 if any failed. On a three-level engine name the catalog
    with `--db-catalog`.

    `--db-export-tables T1,T2`
    :   Tables to export, comma separated. Default: every table of the
        schema.

`--sync-sql FILE --db CONNECTION --sync-table SCHEMA.TABLE --sync-on COLS`
:   Print the SQL that would make a live database table match *FILE*,
    and write nothing. Reads the table, matches rows on `--sync-on`,
    and prints one transaction to stdout: DELETEs for rows only on the
    server, full-row UPDATEs for rows whose values differ, INSERTs for
    rows only in the file. Counts go to stderr, so the script itself
    pipes cleanly to a file or to `psql`. For teams whose database
    changes must be reviewed as a script rather than applied by a tool.

    Numbers are compared as numbers, so a file's `120.50` and a
    `numeric(12,2)` column's `120.50` are not a change. Columns present
    only in the file are reported on stderr and skipped: this never
    emits `ALTER TABLE`. Only the rows loaded under the initial-load cap
    are compared; use `--rows` to raise it.

    `--sync-table SCHEMA.TABLE`
    :   Target table. A three-part *CATALOG.SCHEMA.TABLE* is accepted
        for the engines that have catalogs. Required.

    `--sync-on COLS`
    :   Comma-separated key columns matching file rows to table rows.
        Required: without a key there is no way to tell an updated row
        from a delete plus an insert.

### Cloud and web

The cloud actions take `s3://`, `az://`, `gs://`, `sftp://` and
`davs://` URLs. Credentials come from a saved connection covering the
URL, else the ambient chain. See [Cloud CLI](cloud.md).

`--cloud-ls URL [--recursive]`
:   List a cloud bucket or prefix, one folder level by default. Add
    `--recursive` to flatten everything under the prefix.

`--cloud-get URL --out FILE`
:   Download one cloud object to *FILE*.

`--cloud-put FILE --to URL`
:   Upload a local file to the cloud URL.

`--cloud-copy URL --to URL`
:   Copy a cloud object, or a whole prefix (source ending in `/`), to
    the `--to` URL. Within one bucket the backend copies server-side;
    across buckets, accounts or providers the object is streamed in
    blocks, so object size does not drive memory. At most 10,000
    objects per run.

`--cloud-move URL --to URL`
:   Like `--cloud-copy`, then delete the source. Object stores have no
    rename. The delete runs only after every copy succeeded.

`--cloud-delete URL [--recursive]`
:   Delete a cloud object, or a whole prefix with `--recursive`. Cannot
    be undone unless the bucket has versioning enabled.

`--api CONNECTION [--api-path PATH]`
:   Read a saved REST/JSON API endpoint (**Settings -> API endpoints**)
    as a table. It takes the connection's name or id, never a URL: the
    host and its credential were chosen when the connection was saved.
    Its authentication is applied and its pagination is walked, so the
    result is every page's rows. Reading stops at the endpoint's last
    page, at the row cap (`--rows`), or at a fixed page ceiling;
    stopping on a limit is noted on stderr. A next-page link pointing
    at a different host stops the read with an error rather than being
    followed. For a one-off address, give any read action an `https://`
    URL instead. See [`octa --api`](api.md).

    `--api-path PATH`
    :   Path under the endpoint's base URL, overriding the connection's
        own. It is always joined under that base URL, so it cannot move
        the request to another host.

### Connections

`--list-connections`
:   Print the saved cloud and database connections (names and targets
    only, never secrets).

`--add-connection SPEC [--secret-env VAR]`
:   Add or update a saved connection from a `key=value,key=value` spec.
    `kind=` and `name=` are required. A connection with the same name
    is replaced wholesale, keeping its id and stored secret, so
    re-running a provisioning script is idempotent; omitted keys revert
    to defaults. Unknown keys are an error. Cloud
    (`kind=s3|azure|gcs`): bucket, region, endpoint, prefix, account,
    profile, account_level, anonymous, allow_writes, force_path_style,
    allow_http. Database
    (`kind=postgres|mysql|mssql|oracle|redshift|clickhouse|exasol|trino|athena|snowflake|databricks|bigquery`):
    host, port, database, user, allow_writes, query_timeout.
    `query_timeout` is whole seconds (default 60) and is read only by
    the engines that poll over HTTP for their results (Trino, Athena,
    Snowflake, Databricks, BigQuery). PostgreSQL, Redshift and MySQL
    also take `sslmode` (`disable|prefer|require|verify-ca|verify-full`,
    or MySQL's `disabled|preferred|required|verify_identity`; default
    `prefer`), `sslrootcert` (a PEM CA file) and `hostaddr` (an IP
    address to dial instead of `host`, which then only names the
    certificate). File database (`kind=duckdb|sqlite`): `file` (the
    path, which must exist) and `allow_writes`; other keys and
    `--secret-env` are refused. Only password authentication can be
    expressed here; other methods need the Settings dialog.

    `--secret-env VAR`
    :   Name of an environment variable holding the secret, so it never
        appears in argv. Database: the password. S3:
        *ACCESS_KEY_ID:SECRET_ACCESS_KEY[:TOKEN]*. Azure: the account
        key or a SAS token. GCS uses application-default credentials and
        takes no secret.

`--remove-connection NAME`
:   Remove a saved connection by name or id, and delete its stored
    secret.

### MCP server

`--mcp [--mcp-read-only] [--mcp-tools LIST] [--mcp-without LIST]`
:   Start a Model Context Protocol (MCP) server that speaks JSON-RPC on
    standard input / output, so an AI agent can read, query, compare
    and (unless read-only) write tabular data through Octa. The full
    tool list with descriptions is what the server returns for
    `tools/list`; the [MCP server guide](../mcp/index.md) has a page per
    tool. The tools come in groups: `core` (reading, describing,
    searching and SQL), `quality` (duplicates, outliers, PII, rules and
    correlations), `compare` (schemas, row-level differences and drift
    between two sources), `combine` (unions, joins, join keys and
    reconciling column names), `reshape` (pivot, resample, rolling
    windows, dedupe, impute, anonymise), `databases` (live database
    connections, their tables and queries), `cloud` (objects in S3,
    Azure and GCS) and `write` (writing files, editing tables,
    converting, charts and reports).

    The defaults (response row cap of 1000 rows, per-cell byte cap of
    64 KiB, and file-loader cap of 2,000,000 rows) are configurable
    under [Settings -> MCP](../reference/settings.md#mcp) and
    **Settings -> Performance**. They are read once at server startup;
    changes require a restart. Per call, pass `limit: 0` to lift the
    response cap and `unlimited: true` to lift the file-loader cap so
    the tool sees every row on disk. Parquet files with very many row
    groups fall back to a DuckDB-backed reader automatically. See
    [Limits & truncation](../mcp/limits-and-truncation.md) for the full
    mechanics.

    `--mcp-read-only`
    :   Leave out every tool that writes: the `write` group, the `cloud`
        tools other than the listing, and the two database writers. The
        server then exposes a read-only surface. See [Read-only
        mode](../mcp/setup.md#read-only-mode).

    `--mcp-tools LIST`
    :   Advertise ONLY these tools, comma-separated. Takes group names
        (above) and individual tool names, mixed freely:
        `--mcp-tools core,databases` or `--mcp-tools read_table,run_sql`.
        Without it every tool is advertised. A tool left out is also
        not callable. An unrecognised name is an error and the server
        does not start. This is the lever for an agent's context size:
        the client reads the tool list once and carries it in every
        request to its model, and all of Octa's tools together are
        roughly 39,000 tokens of description and schema. See
        [Advertising fewer tools](../mcp/setup.md#advertising-fewer-tools).

    `--mcp-without LIST`
    :   Advertise everything except these, in the same spelling as
        `--mcp-tools`. Applied after it, so the two combine:
        `--mcp-tools core,write --mcp-without convert`.

## Common options

`-f FORMAT`, `--format FORMAT`
:   Output format for every action that prints a table. It has no
    effect for `--convert` (whose output format comes from the output
    path's extension), `--export-schema` (which emits source code
    chosen by `-t`), the plain listings `--partition-by`,
    `--batch-convert` and `--to-workbook` print, or `--mcp`. *FORMAT*
    is one of:

    - `tsv` *(default)*: tab-separated values, one row per line, header
      row first. TAB and newline characters in cells are replaced with
      spaces, since TSV has no escape mechanism.
    - `json`: pretty-printed JSON array of `{column: value}` objects.
      Numeric and boolean cells keep their native JSON types; dates,
      blobs and nested values become strings.
    - `csv`: RFC 4180 CSV. Fields with comma, quote, or newline are
      properly quoted; embedded quotes are doubled.

`--rows N|all`
:   Override the initial-load row cap for this invocation. Streaming
    formats (Parquet, CSV, TSV) honour a process-wide cap (default
    **2,000,000** rows). Pass a number (commas/underscores allowed) to
    raise it or `all` to disable the cap entirely. Applies to every
    action that reads a file. `--partition-by`, `--split-rows` and
    `--split-size` read every row unless `--rows` says otherwise, so no
    piece is silently missing.

`--max-decompressed MB|unlimited`
:   How large a `.gz` or `.zst` input may grow when unpacked, for this
    invocation or, given with `--mcp`, for the whole server (no tool call
    can raise it). Megabytes, 1 MB = 1,000,000 bytes, commas allowed.
    `unlimited` removes the limit. Without it, the decompression limit in
    [Settings -> Performance](../reference/settings.md) applies (read from
    `settings.toml`), else **4,295** MB (4 GiB). A file that would unpack
    past the limit is refused, which guards against a tiny archive that
    unpacks to terabytes.

`-n N`, `--lines N`
:   Row count for `--head`, `--tail`, and `--sample`. Default **20**.

`--seed N`
:   Seed for `--sample` and `--test-data`, for reproducible output.
    Default **0**.

`--sort-by COLS`
:   Sort the rows of `--head` or `--convert` by these columns, first key
    first (comma-separated or repeated). Sorts the rows that were read,
    up to the row cap: add `--rows all` to sort a whole large file. With
    `--head` the sort happens before the first *N* rows are taken.

`--sort-desc COLS`
:   The `--sort-by` columns that sort high to low.

`--natural`
:   Sort text naturally (file2 before file10) instead of letter by
    letter, the default.

`--table NAME`
:   For `--describe`, `--validate-schema`, `--unique-columns`: the
    table to read from *FILE* when the source is multi-table.

`--table-a NAME`, `--table-b NAME`
:   For `--compare-schemas`, `--compare-distributions` and
    `--check-references`: the table to read from each side when the
    source is multi-table (SQLite, DuckDB, GeoPackage).

`--out-dir DIR`
:   Output directory for `--partition-by`, `--split-rows`,
    `--split-size`, `--batch-convert`, `--harmonise-schema` and
    `--db-export` (required; created if absent).

`--out FILE`
:   Output file for `--cloud-get` and for `--harmonise-schema
    --combine`.

`--to EXT|URL`
:   The target extension for `--convert IN -` (no leading dot, e.g.
    `csv`, `parquet`, `json`), `--batch-convert` and `--db-export`; the
    cloud URL for `--cloud-put`, `--cloud-copy` and `--cloud-move`.

`--overwrite`
:   Replace existing outputs in `--batch-convert`, `--harmonise-schema`
    and `--db-export`, which skip them otherwise.

`--recursive`
:   Walk subdirectories for `--schema-drift`, `--relationships` and
    `--harmonise-schema`. For `--cloud-ls` it flattens the listing; for
    `--cloud-delete` it is the required confirmation that a folder
    delete is meant.

`--ignore-case`
:   Treat column names differing only in case as the same column
    (`--schema-drift`, `--harmonise-schema`).

`--password-env VAR`
:   With `--convert`, write only a password-protected AES-256 zip
    (`out.csv` becomes `out.csv.zip`); with `--batch-convert`, write
    every output as its own zip (`a.csv` becomes `a.csv.zip`). The
    passphrase is taken from environment variable *VAR*, never from
    argv.

`--compression CODEC`
:   Compression codec for the written file: `uncompressed`, `snappy`,
    `zstd`, `gzip` or `lz4`. Applies to `--convert` and
    `--batch-convert`; Parquet targets only, ignored by other formats.
    An unknown name is rejected before any file is written. Omitted,
    the codec saved in **Settings -> Saving files -> Write options** is used,
    or `zstd` when nothing is saved.

`--row-group-size N`
:   Rows per Parquet row group for `--convert` and `--batch-convert`.
    Larger groups scan faster; smaller groups let readers skip more
    precisely. Omitted means the writer's own default.

`--completions SHELL`
:   Print a shell completion script for *SHELL* to standard output and
    exit. One of `bash`, `zsh`, `fish`, `powershell`, `elvish`. The
    script is generated from Octa's own argument list, so it completes
    every flag this page documents. Wire it into the running shell with
    `eval "$(octa --completions zsh)"`, or let `install.sh` write the
    files under the install prefix. See [`--completions`](completions.md).

`-h`, `--help`
:   Print the flag list and exit. `-h` and `--help` produce the same
    output.

`--help-all`
:   Print the flag list plus worked examples for every action, and
    exit. The examples are behind their own flag because printing them
    by default buried the flag list under something the size of this
    manual page.

`--version`
:   Print the Octa version and exit.

## Output

Tabular data is written to **stdout**. Status messages, warnings,
and errors are written to **stderr**. This means
`octa --sql FILE -q QUERY -f json | jq ...` is safe even when an
error occurs, since the data stream stays clean.

`--batch-convert`, `--harmonise-schema`, `--partition-by`, `--db-copy`
and `--db-export` report progress on stderr while they run, on a
single line that rewrites itself (`[137/500] file.csv  ETA 1:12`). The
line appears only when stderr is a terminal, so redirected output and
CI logs are unchanged.

## Exit status

`0`
:   Success.

`1`
:   Any error: invalid arguments, file not found, parse failure, write
    rejection, and so on. These actions also exit `1` on a finding, so
    CI pipelines can gate on them directly:

    | Action               | Exits 1 when                                                  |
    |----------------------|---------------------------------------------------------------|
    | `--validate-schema`  | The schemas differ.                                           |
    | `--schema-drift`     | The files disagree about their columns.                       |
    | `--check`            | A rule failed or could not run.                               |
    | `--check-references` | Child rows have no parent.                                    |
    | `--compare-folders`  | A file differs or is missing, or a folder hit the file limit. |
    | `--drift-report`     | A `--fail-on` gate was breached.                              |
    | `--overlaps`         | Overlapping spans were found.                                 |
    | `--merge`            | A conflict is open.                                           |
    | `--recipe`           | A step could not run.                                         |
    | `--harmonise-schema` | A file was refused.                                           |
    | `--batch-convert`    | An input failed.                                              |
    | `--db-export`        | A table failed.                                               |

## Examples

### Look at a file

Open multiple files in the GUI:

```bash
octa file1.csv file2.parquet file3.json
```

Print the schema of a Parquet file:

```bash
octa --schema sales.parquet
```

Print the first 5 rows of a CSV as JSON:

```bash
octa --head data.csv -n 5 -f json
```

Print the last rows / a reproducible random sample:

```bash
octa --tail data.csv -n 5
octa --sample data.parquet -n 20 --seed 1
```

One-shot file snapshot:

```bash
octa --describe data.parquet --sample-rows 3
octa --describe users.sqlite --table customers -f json
```

### Analyse

Find primary-key candidates:

```bash
octa --unique-columns users.csv
octa --unique-columns orders.parquet --max-combo 2 -f json
```

Did a column's distribution move between two files?

```bash
octa --compare-distributions sales.csv --dist-column amount --dist-column-b amount_last_year
octa --compare-distributions q2.parquet --dist-column price --dist-file-b q3.parquet
```

### Compare and check

Diff two schemas:

```bash
octa --compare-schemas v1.parquet v2.parquet
octa --compare-schemas a.sqlite b.sqlite --table-a users --table-b users -f json
```

Diff two files' rows:

```bash
octa --diff v1.csv v2.csv
octa --diff v1.csv v2.csv --diff-mode ordered
octa --diff v1.csv v2.csv --diff-mode join --diff-on id
octa --diff a.parquet b.parquet -f json
```

Validate a file against a JSON Schema in CI:

```bash
octa --export-schema sales.parquet -t json-schema > sales.schema.json
octa --validate-schema sales.parquet --expect-schema sales.schema.json
```

Find orders whose customer does not exist:

```bash
octa --check-references customers.csv --parent-column id \
     --child-file orders.csv --child-column customer_id
```

### Clean and transform

Add a stable key built from several columns:

```bash
octa --hash-columns customer_id,order_date orders.parquet -f csv > keyed.csv
octa --hash-columns first,last,birth --hash-algo sha256 --hash-trim --hash-upper people.csv
```

### Combine and split

One workbook from several tables:

```bash
octa --to-workbook report.xlsx sales.csv returns.parquet stock.json
```

Fold a folder of parts into one file:

```bash
octa --harmonise-schema ./parts --combine --out ./all-parts.csv
```

### Convert and SQL

Convert formats:

```bash
octa --convert in.csv out.parquet
octa --convert workbook.xlsx tidy.sqlite
```

Group-by aggregation:

```bash
octa --sql sales.parquet -q 'SELECT region, SUM(amount) FROM data GROUP BY region'
```

Read every row of a huge file:

```bash
octa --sql huge.parquet -q 'SELECT count(*) FROM data' --rows all
octa --head huge.parquet -n 100 --rows 10,000,000
```

Pipe a SQL result through `jq`:

```bash
octa --sql users.parquet -q 'SELECT email FROM data WHERE active' -f json | jq -r '.[].email'
```

JOIN across formats:

```bash
octa --sql sales.parquet \
     --sql-table customers=customers.csv \
     -q 'SELECT c.name, SUM(d.amount) FROM data d
         JOIN customers c ON d.cid = c.cid GROUP BY c.name'
```

ATTACH a DuckDB warehouse and JOIN against it:

```bash
octa --sql sales.parquet \
     --sql-attach wh=warehouse.duckdb \
     -q 'SELECT count(*) FROM data d
         JOIN wh.main.products p ON d.cid = p.cid'
```

Write a SQL result back to a DuckDB schema:

```bash
octa --sql sales.parquet -q 'SELECT region, SUM(amount) AS total
                             FROM data GROUP BY region' \
     --sql-write-to analytics.duckdb \
     --sql-write-schema reports --sql-write-table q4_summary
```

Export a schema as Snowflake DDL:

```bash
octa --export-schema sales.parquet -t snowflake
octa -e users.parquet -t pydantic > users_model.py
```

### Databases

Browse a live database connection:

```bash
octa --db-tables --db warehouse
octa --db-tables --db warehouse --db-catalog sales_prod
octa --db-query "SELECT * FROM public.users LIMIT 10" --db prod
```

Review a change as SQL before it touches the server:

```bash
octa --sync-sql users.csv --db prod --sync-table public.users --sync-on id > change.sql
```

Copy a table between two servers, replacing the target:

```bash
octa --db prod --db-copy analytics.orders \
     --db-copy-to warehouse --db-write-mode replace
```

### Cloud and web

```bash
octa --cloud-ls s3://bucket/prefix/
octa --cloud-ls s3://bucket/ --recursive

octa --cloud-get s3://bucket/data.parquet --out ./data.parquet
octa --cloud-put ./data.parquet --to s3://bucket/data.parquet

octa --cloud-copy s3://a/data/ --to gs://b/backup/
octa --cloud-move s3://a/old.csv --to s3://a/archive/old.csv
octa --cloud-delete s3://a/old.csv
octa --cloud-delete s3://a/scratch/ --recursive

# A saved REST/JSON endpoint, every page of it
octa --api Orders
octa --api Orders --api-path /customers -f csv > customers.csv
```

### Connections

```bash
octa --list-connections

export S3_KEY='AKIAEXAMPLE:wJalrXUtnFEMI'
octa --add-connection 'kind=s3,name=prod,bucket=my-bucket,region=eu-central-1,allow_writes=true' \
     --secret-env S3_KEY

octa --add-connection 'kind=postgres,name=warehouse,host=db.internal,database=analytics,user=reader' \
     --secret-env PGPASSWORD

octa --remove-connection warehouse
```

### MCP server

Start the MCP server:

```bash
octa --mcp
octa --mcp --mcp-read-only --mcp-tools core,quality
```

### Pipelines

`-` stands for standard input as a *FILE*, and for standard output as
the `--convert` target, so octa fits in the middle of a shell
pipeline.

```bash
# Read a table from a pipe: the format is sniffed from the bytes
curl -s https://example.org/sales.csv | octa --schema -

# Query it without ever writing a file
cat sales.csv | octa --sql - -q "SELECT city, SUM(amount) FROM data GROUP BY city"

# Convert on the way through; --to says what to write, since a pipe has no name
cat sales.csv | octa --convert - - --to json | jq '.[0]'
```

Reports and counts go to stderr, so what reaches the next command in
the pipeline is exactly the data. Piped input is buffered to a
temporary file first: every reader needs to seek (a Parquet footer is
at the end of the file), so a pipe cannot be streamed straight
through.

### Reading from a URL

Anywhere a *FILE* is accepted, a URL works too.

```bash
# A plain web address: downloaded to a temp file, no configuration needed
octa --schema https://example.org/exports/sales.csv
octa --sql https://example.org/exports/sales.csv -q "SELECT * FROM data LIMIT 5"

# Cloud object storage resolves through the saved connections instead
octa --schema s3://bucket/sales.parquet
```

The format comes from the URL's path, ignoring any query string, so a
signed link ending `sales.csv?token=...` still reads as CSV; a URL
with no extension falls back to content sniffing. Both forms are
read-only, and a non-2xx response is an error naming the status code
rather than a file, so a 404 HTML page is never parsed as a one-column
table.

Tool-supplied URLs are restricted. When the address comes from an
agent (the MCP server or the in-app assistant) rather than from you,
Octa resolves the host first and refuses anything that is not globally
routable: loopback, private ranges, link-local (which includes the
cloud metadata endpoint at 169.254.169.254), and unique-local IPv6.
Redirects are not followed on that path either, since a redirect is
how a check on the first hop gets bypassed. Addresses you type
yourself, on the command line or in **File > Open URL**, are not
restricted: reading from `localhost:8000` is a normal thing to ask
for.

## Environment

`OCTA_CONFIG_DIR`
:   Directory holding `settings.toml`, used verbatim on every platform
    and taking precedence over `XDG_CONFIG_HOME` / `HOME` / `APPDATA`.
    Required in containers, which usually set none of those; without it
    Octa reports that it has no config directory rather than silently
    running without settings.

`OCTA_DEBUG`
:   Set to 1 to force debug mode on for one run without changing the
    saved setting: verbose logging, plus one log line per mouse press
    and release recording its position, whether it counted as a click,
    and which layer of the interface it reached. Intended for
    diagnosing the GUI itself, when the Settings dialog is the thing
    misbehaving and its checkbox cannot be reached. A build from source
    additionally outlines every clickable area on screen; release
    binaries cannot, since the toolkit compiles that drawing out. See
    [Diagnostics](../reference/diagnostics.md).

`OCTA_NO_KEYRING`
:   Set to 1 to skip the OS keyring entirely, so secrets are read from
    and written to `settings.toml` (chmod 0600). Secrets already fall
    back to the file when the keyring errors; this avoids the lookup on
    systems where it can never work, such as a container with no D-Bus.

`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AZURE_STORAGE_ACCOUNT`, `GOOGLE_APPLICATION_CREDENTIALS`
:   Ambient cloud credentials, used when no saved connection covers a
    URL.

## Files

`$XDG_CONFIG_HOME/octa/settings.toml`
:   Linux. User settings. Created on first launch with defaults.
    See [Settings reference](../reference/settings.md) for every
    key.

`$HOME/Library/Application Support/Octa/settings.toml`
:   macOS. Same purpose.

`%APPDATA%\Octa\settings.toml`
:   Windows. Same purpose.

## See Also

`man(1)`, `jq(1)`, `duckdb(1)`, `parquet-tools(1)`

- `octa --help-all` for worked examples of every action.
- Project homepage: <https://github.com/thorstenfoltz/octa>
- Online documentation: <https://thorstenfoltz.github.io/octa/>
- [Common tasks](../tips/workflows.md) covers worked CLI
  workflows (CSV to Parquet pipelines, JSON-line filtering, etc.).

## Bugs / Feedback

Report bugs at <https://github.com/thorstenfoltz/octa/issues>.

## Author

Thorsten Foltz and contributors.

## Copyright

Copyright (c) 2026 Thorsten Foltz. Licensed under the MIT
license.
