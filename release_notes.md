## Features

- Add SQLite reader/writer (`.sqlite`, `.sqlite3`, `.db`) with diff-based transactional saves that preserve row identity via `rowid`
- Add DuckDB reader/writer (`.duckdb`, `.ddb`) using a synthetic `__octa_row_id` column for stable row tracking across saves
- Add multi-table picker dialog: databases with multiple user tables prompt for selection (with row counts and schema preview); single-table databases auto-open
- Add SQL Query view: run DuckDB SQL against the active table (exposed as `data`) with Ctrl+Enter; results render in a split pane below the editor
- Database saves are diff-based and transactional — only changed rows are UPDATEd, new rows INSERTed, removed rows DELETEd; schema changes are rejected to protect downstream consumers
- Expand settings and theming: SQL panel preferences, body/custom font options, additional theme presets, and refreshed toolbar/status framing
- Document SQLite, DuckDB, and SQL query support in the README
