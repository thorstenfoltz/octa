---
paths:
  - "src/sql/**"
  - "src/app/sql_panel.rs"
  - "src/view_modes/sql.rs"
---
# SQL workspace and SQL view

## Module layout

- `src/sql/mod.rs`: re-exports `SqlWorkspace`, `AttachKind`, `WriteTarget`, `WriteMode`, `WriteReport`, `QueryKind`, `QueryOutcome`, `sanitize_sql_name`. `run_query(table, query)` is a one-shot wrapper (fresh workspace, registers `data`, tears down).
- `src/sql/workspace.rs`: `SqlWorkspace` owns one persistent DuckDB conn + `tables` + `attachments`. API: `new`, `set_active_table` (the `data` TEMP TABLE), `add_table_from_{file,datatable}`, `attach`, `detach`, `remove_table`, `list_attached_tables`, `execute`, `write_result_to_db`. DuckDB ATTACH native for `.duckdb`/`.ddb`; SQLite tries `INSTALL/LOAD sqlite` then falls back to per-table `SqliteReader` (`alias__table`); SQLite writes always via `rusqlite`. `execute` columns all `Utf8`. Sync; MCP wraps with `spawn_blocking`.

## SQL view

Active table exposed as DuckDB temp table `data`. Each tab owns a long-lived `SqlWorkspace` (`TabState.sql_workspace: Option<SqlWorkspaceHandle>` - `Rc<RefCell<SqlWorkspace>>`), dropped on close. Workspace section lists tables + ATTACHed DBs with per-row [x]/[detach], [+ Add table...], [Attach database...]; [refresh] re-pushes the active table's edits (SQL sees a cached snapshot until then). Header also carries the plain-language **Ask** box (see Key Design Patterns; `SqlAction.ask`, greyed with a reason when no chat profile or no columns). Result actions [Run]/[Clear]/[Export...]/[Write result to DB...] (last -> `SqlWriteBackDialog` -> `SqlWorkspace::write_result_to_db`; writing into the open file's own `db_meta.table_name` with Append/Replace is rejected).

Docks Bottom/Top/Left/Right via Settings; editor has a monospace line-number gutter sharing one `ScrollArea`. Template `SELECT * FROM data LIMIT {sql_default_row_limit}` via hint_text. Autocomplete chips at word-token end; while the popup is open (`popup_active`) Up/Down move the selection, Enter or Tab accepts, Esc dismisses - keys are only consumed while it is open, so plain typing keeps Enter/arrows. The editor grabs focus on open via `TabState.sql_editor_focus_pending`. Workspace state is session-only. **The workspace opens on an EMPTY tab too** (no col_count gate; `set_active_table` skipped for zero-column snapshots so `data` just is not registered - attach + query servers with no file open). The inspector row splits in two when attachments exist: `render_attachment_info` is an attachments cheat-sheet (alias + kind + source + one-click example insert; i18n `sql.att_info*`).

Results render a row-counter label directly above the grid (display-only, never in the data/export). Result collection (local `sql/engine.rs::execute_query` AND server connectors) stops at `initial_load_rows()`; `result_rows_label` switches the counter to `sql.result_rows_capped` when row_count sits on the cap (translated x32; hermetic test `tests/sql_result_cap_tests.rs`, own binary since the guard is process-wide). `render_sql_view` zeroes `widgets.{hovered,active}.expansion` for the whole panel - themes paint hovered widgets up to 3px larger, which reads as twitching in the dense grid.

**Attached-table listings are cached** (`SqlWorkspace.attached_tables_cache`, RefCell): the panel rebuilds its workspace snapshot per frame, and for Postgres/MySQL attachments an uncached `list_attached_tables` is a remote information_schema query - two attached servers made the editor unusable. Cache dropped on `detach` + `invalidate_attached_cache()`; remote attachments also skip the per-table `COUNT(*)` entirely (`row_count: None` - on InnoDB that is a table scan).

**History** dropdown: per-tab session `TabState.sql_history` (recorded by `record_sql_history` in `run_workspace_query`, cap 30); `SqlAction.recall_query` loads one back. **Snippets** button opens `src/app/dialogs/sql_snippets_window.rs` (standard DialogSize chrome, app-level `OctaApp.sql_snippets_window_open`/`_size`) with save-current/Insert/delete; persistent `OctaApp.sql_snippets: Vec<SqlSnippet>` (`src/app/sql_snippets.rs`, `<config_dir>/sql_snippets.json`, name+description+query); naming dialog `src/app/dialogs/sql_snippet.rs`; `SqlAction.{insert_snippet,save_snippet,delete_snippet}`. i18n `sql.{history,snippets,snippet_*}`, `dialog.snip_*`.

**Row-diff highlight after mutation** (`sql_row_diff_highlight_enabled` default on + `sql_row_diff_highlight_secs` default 4): `apply_sql_diff_highlight` diffs pre/post via `compare::compare_ordered`, marks changed cells (Cell) + new rows (Row) green on the mutated table, stores keys in `TabState.sql_diff_marks` + expiry `sql_diff_highlight_until`; `OctaApp::expire_sql_diff_highlights` (per frame in `update_loop`) clears them when the timer elapses. i18n `settings.sql_diff_{highlight,secs}`.
