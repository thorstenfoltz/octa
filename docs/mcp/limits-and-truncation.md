# Limits & Truncation

The MCP server protects you (and your MCP client) from accidentally
pulling enormous responses through the stdio channel. Two
independent caps apply to every result-bearing tool call:

| Cap               | Default               | Setting                                                            |
|-------------------|-----------------------|--------------------------------------------------------------------|
| **Row limit**     | 1000 rows             | [Settings → MCP → Default row limit](../reference/settings.md#mcp) |
| **Cell byte cap** | 65,536 bytes (64 KiB) | [Settings → MCP → Cell byte cap](../reference/settings.md#mcp)     |

Both can be overridden per call (the row limit explicitly via a
`limit` parameter; the cell cap currently global-only). When a cap
fires, the response includes a flag so the AI client knows the
result is partial.

## Why the defaults

A single MCP response travels through:

1. The `octa --mcp` process's stdout.
2. The MCP client's stdin (Claude Desktop, Claude Code, etc.).
3. The conversation context window in the model.

Every byte costs tokens, and tokens cost latency + money. A
naïvely-unbounded `read_table` against a 100M-row Parquet file
would push gigabytes of JSON through the chain before either side
realised: at best slow, at worst crashes the client.

The 1000-row default is the rough sweet spot: enough rows for the
model to spot patterns, schemas, value distributions; few enough
that a typical response fits in a few thousand tokens.

## How the row limit works

For [`read_table`](tools/read_table.md) and [`run_sql`](tools/run_sql.md):

```
caller passes limit?
  │
  ├─ omitted        → fall back to server default (1000 unless changed)
  ├─ 0              → unlimited (return every row)
  └─ Some(n)        → return min(n, total_rows)
```

The response always includes three envelope fields so the client
knows what happened:

```json
{
  "rows": [...],
  "row_count": 1000,
  "truncated": true,
  "total_rows_available": 47832,
  "cell_truncated": false
}
```

- **`row_count`** is the number of rows actually returned.
- **`truncated`** is `true` when more rows existed than were
  returned.
- **`total_rows_available`** is the underlying total (when cheaply
  known). For streaming Parquet files this might itself be capped
  by the initial-load row cap; see [Streaming format
  caveat](#streaming-format-caveat) below.
- **`cell_truncated`** is an independent flag for the per-cell cap;
  see below.

### When the model sees `truncated: true`

The conversational AI can decide to:

1. **Acknowledge the truncation** and proceed with the sample.
2. **Re-call with a higher limit** if the user wants more rows.
3. **Re-call with `limit: 0`** if the user wants everything.
4. **Re-call with `run_sql`** to filter / aggregate first
   (usually the right move on big files).

The `total_rows_available` value lets the model make this call
intelligently, e.g. *"the file has 50k rows; do you want me to
fetch all of them, or filter first?"*

### When the model should NOT pass `limit: 0`

- **Multi-GB files**: even at 1000 rows per call, a 100M-row
  Parquet is 100,000 round-trips. `limit: 0` would send all 100M
  rows in one response, which the JSON-RPC channel won't gracefully
  handle.
- **Binary-heavy columns** (BLOBs, large embedded JSON): the per-row
  size can be huge; 1000 rows might still be megabytes.
- **Unknown files**: when the model doesn't yet know `total_rows_available`,
  start with the default. Use [`count_rows`](tools/count_rows.md)
  as a discovery step.

## How the cell byte cap works

Every cell's stringified form is measured against the cap (default
64 KiB). When a cell exceeds the cap, the value is replaced with a
truncation marker and `cell_truncated` is set to `true`:

```
[truncated: 1247832 bytes; cap 65536 bytes. Slice the value with --sql / run_sql to fetch the rest.]
```

The marker tells the model:

- How big the original was (`1247832 bytes`).
- What the cap was (`65536 bytes`).
- How to get the full content if needed (`--sql / run_sql` to
  `SUBSTR(...)`).

Set the cap to `0` (under
[Settings → MCP](../reference/settings.md#mcp)) to disable
cell-size truncation. Useful when you regularly work with files
whose every row exceeds 64 KiB, but be aware that big BLOB
columns then travel uncapped.

### What gets affected by the cell cap

| Cell type                                                                    | Capped?                                                                                |
|------------------------------------------------------------------------------|----------------------------------------------------------------------------------------|
| `Null`                                                                       | n/a (no bytes)                                                                         |
| `Bool`                                                                       | n/a (1 byte JSON)                                                                      |
| `Int`, `Float`                                                               | n/a (~ 20 bytes max)                                                                   |
| `String` (short)                                                             | Below cap → unchanged                                                                  |
| `String` (long, e.g. multi-paragraph text, JSON blobs, base64-encoded media) | **Capped if over**                                                                     |
| `Date`, `DateTime`                                                           | n/a (short strings)                                                                    |
| `Binary` (BLOB columns)                                                      | Hex-encoded; **2× larger** than raw bytes, so a 33 KiB BLOB exceeds the 64 KiB default |
| `Nested` (JSON/array stringification)                                        | **Capped if over**                                                                     |

## Streaming format caveat

For Parquet, CSV, and TSV, Octa reads only the first
`initial_load_rows` (default 5,000,000) into memory at file-open
time. This is the same row cap the GUI uses, configurable under
[Settings → Performance](../reference/settings.md#performance)
(including an "Unlimited" checkbox).

For the MCP server, this means:

- A `count_rows` call against a 100M-row Parquet returns 5,000,000
  with `initial_load_capped: true`.
- A `read_table` call against the same file with `limit: 0`
  returns 5,000,000 rows, not 100M.
- A `run_sql` call (including `SELECT count(*) FROM data`) runs
  against the same in-memory snapshot; DuckDB doesn't re-open
  the file, so the cap applies there too.

### Lifting the cap per call

Every read-bearing tool (`read_table`, `count_rows`, `run_sql`,
`convert`, `profile`, `find_duplicates`, `value_frequency`,
`search`) takes a boolean `unlimited` parameter. Pass
`unlimited: true` to lift the file-loader cap for that single call
so the tool sees every row in the file. For tools that *also* have
a `limit` (response-row cap), combine `limit: 0` + `unlimited: true`
to truly return every row.

Alternatively, raise the cap permanently in
[Settings → Performance](../reference/settings.md#performance)
(the field accepts very large values; the "Unlimited" checkbox
sets it to effectively no cap) and restart the MCP server.

### Parquet row-group fallback

Parquet files with more than 32,767 row groups (common with Spark
or streaming ingest writers that emit many small batches) used to
fail the native arrow-parquet reader with
`Row group ordinal 32768 exceeds i16 max value`. Octa retries
those reads through a DuckDB-backed reader automatically — same
schema and types, just routed through DuckDB's parquet
implementation instead. No user action needed.

## Changing the defaults

[Settings → MCP](../reference/settings.md#mcp) has two inputs:

- **Default row limit**: an integer plus an **Unlimited** checkbox.
  Checking Unlimited writes `None` to the config (the server
  returns every row by default).
- **Cell byte cap**: an integer (in bytes). `0` means no cap.

Both values are read **once at MCP server startup**. After
changing them, restart the MCP server:

- **Claude Desktop**: restart Claude Desktop (the subprocess gets
  re-spawned).
- **Claude Code**: `claude mcp remove octa && claude mcp add octa ...`
  to force a respawn, or just let the next invocation pick it up
  (Code re-spawns the MCP subprocess per session in some versions).
- **MCP Inspector**: stop and re-run the `npx` command.

The server logs the active values to **stderr** on startup so you
can verify:

```
octa --mcp ready (default row limit: 1000, cell cap: 65536 bytes; ...)
```

## When to raise limits

Common scenarios for raising the row cap (or going unlimited):

- **Analytics workflows** where the model needs to "see all the
  data" to make accurate summaries.
- **Files under 100k rows**, which are well within an acceptable
  response budget.
- **Filtered queries** via [`run_sql`](tools/run_sql.md) where you
  know the result is small; the filter happens server-side before
  the cap applies.

When to **lower** the cap:

- **Files with very wide rows** (50+ columns of long strings),
  where even 1000 rows might overflow the model's context window.
- **You're running on a metered AI plan** and want to minimize
  token spend per call.

## See also

- [Tools reference](tools/index.md) is where every result-bearing
  tool honours these caps.
- [Settings → MCP](../reference/settings.md#mcp) is where to change
  the row + cell defaults.
- [Settings → Performance](../reference/settings.md#performance)
  governs the initial-load row cap that bounds streaming readers.
- [`run_sql`](tools/run_sql.md) applies a server-side filter
  before the row limit takes effect.
