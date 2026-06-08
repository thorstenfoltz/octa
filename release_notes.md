# Release notes

Improvements to date detection on load, the whitespace-trim notice, five new
interface languages, and a seasonal touch.

## Date detection

**Date inference now runs on every format.** Columns of date-shaped text are
promoted to a proper Date/DateTime display regardless of which format the file
came from. Previously this pass only ran on text-style formats (CSV, TSV, JSON,
Excel, XML, TOML, YAML, Markdown, DBF). Binary formats that already carry typed
dates (Parquet, Arrow, SQLite, DuckDB, and the rest) are unaffected: the check
only ever inspects plain text columns, so reader-provided Date/Timestamp
columns are left exactly as they were.

**"Looks like a date but stayed text" notice.** When a column looks
date-shaped, most of its values parse as a date, yet a few cannot, Octa now
leaves the column as text and shows a banner explaining why, naming the column,
the layout it matched, how many values parsed, and a few of the offending
values. This makes it clear why a column you expected to become a date is still
text. The notice is dismiss-only; nothing is changed. You can turn it off under
**Settings -> File-Specific**.

## Whitespace trim

**"Dismiss" now undoes the trim.** On the load-time whitespace-trim banner,
**Okay** keeps the trimmed values (as before) and **Dismiss** now restores the
original leading and trailing whitespace instead of just closing the notice.
The revert touches only the cells and titles that were actually trimmed, and
keeps database diff-saves correct.

## Languages

**Five new interface languages.** The interface can now be shown in Greek,
Russian, Japanese, Korean, and Chinese, bringing the total to 22 languages.
Change it under **Settings -> Appearance -> Language**; the switch is live.
Monospace text (such as the SQL editor and its gutter) now renders Greek and
Cyrillic correctly as well, instead of showing empty boxes.
