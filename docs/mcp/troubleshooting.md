# MCP Troubleshooting

Common failure modes when wiring Octa's MCP server into a client.

## Server doesn't start

### "command not found: octa"

The MCP client can't find the `octa` binary. Two fixes:

- **Add Octa to PATH.** Move the binary to `~/.local/bin/` (Linux/macOS)
  or `C:\Program Files\Octa\` (Windows, and add to PATH), then
  re-test with `octa --version` in a fresh terminal.
- **Use the full path in the MCP config**. In `claude_desktop_config.json`:

    ```json
    {
      "mcpServers": {
        "octa": {
          "command": "/home/you/.local/bin/octa",
          "args": ["--mcp"]
        }
      }
    }
    ```

### Octa exits immediately

Run `octa --mcp` in a terminal directly. You should see the startup
banner on stderr:

```
octa --mcp ready (default row limit: 1000, cell cap: 65536 bytes; ...)
```

If you see an error instead (e.g. *"could not build tokio runtime"*
or a panic message), that's the real issue. Report it on the
[Octa issue tracker](https://github.com/thorstenfoltz/octa/issues)
with the exact error.

If you see **nothing**, the binary is broken or the terminal is
swallowing output. Try `octa --version` to confirm the binary
itself works.

## Claude Desktop doesn't list Octa's tools

### Hammer icon (🔨) missing

Check `claude_desktop_config.json` exists and is valid JSON:

| Platform | Path                                                              |
|----------|-------------------------------------------------------------------|
| Linux    | `~/.config/Claude/claude_desktop_config.json`                     |
| macOS    | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows  | `%APPDATA%\Claude\claude_desktop_config.json`                     |

Common mistakes:

- **Trailing comma after the last entry**: JSON doesn't allow it.
- **Wrong key name**: must be `mcpServers` (camelCase), not
  `mcp_servers` or `MCPServers`.
- **Wrong `command` path**: test in a terminal first.
- **Comments in JSON**: strict JSON doesn't support `//` comments;
  remove them.

After fixing, **fully quit Claude Desktop** (not just close the
window) and reopen it. The MCP config is read at startup.

### Hammer icon present, but no octa tools

1. Click the hammer icon to see the connected servers list. Is
   `octa` there?
2. If it's there with a red error, click it for the error message.
   Usually means the subprocess crashed.
3. Tail Claude Desktop's logs:

    | Platform | Log path                        |
    |----------|---------------------------------|
    | Linux    | `~/.config/Claude/logs/mcp.log` |
    | macOS    | `~/Library/Logs/Claude/mcp.log` |
    | Windows  | `%APPDATA%\Claude\logs\mcp.log` |

    The MCP server's stderr is captured here. Look for the Octa
    startup banner; if missing, the subprocess didn't start.

## Tools return errors

### "no reader available for /path/to/file"

The file's extension isn't recognised by Octa's FormatRegistry.
Check:

- Is the extension lowercase? Octa lowercases before matching, so
  this should be fine, but if you've renamed a file, double-check
  the actual extension.
- Is it a format Octa supports? See
  [Supported formats](../getting-started/supported-formats.md).
- Did you mean to open it as plain text? Add the extension to
  [**Settings → Performance → Open as text**](../reference/settings.md#performance)
  (e.g. `log4j` for unusual log extensions).

### "read failed: …" with format-specific message

The reader claimed the file but parsing failed. Common cases:

- **CSV** with unusual delimiter or quoting: Octa's CSV reader
  auto-detects but isn't infallible. Open in the GUI; Octa might
  fall back to the [Raw view](../usage/view-modes/raw-text.md)
  with a parse error banner showing the detail.
- **JSON** that isn't well-formed JSON. The error message points
  at the line/column.
- **Encoding mismatch**: Octa expects UTF-8 for text formats.
  Latin-1 / Windows-1252 files fail until converted.

### "run_sql failed: syntax error at \"FOO\""

DuckDB parser error. The model usually corrects on follow-up.
Common quirks:

- **Identifiers with spaces** need double quotes: `"My Column"`,
  not `[My Column]` (that's T-SQL syntax) and not backticks
  (`` `My Column` ``, that's MySQL).
- **String literals** use single quotes: `WHERE region = 'EU'`.
  Double quotes mean *identifier*.

### Empty / capped results when the file has more data

You're hitting the row limit. See [Limits &
truncation](limits-and-truncation.md). Quick fixes:

- Pass `limit: 0` to the tool for unlimited rows (be careful with
  big files).
- Use `run_sql` with `WHERE` / `LIMIT` to filter server-side first.
- For Parquet files larger than Octa's initial-load cap (default
  1M rows), use [`run_sql`](tools/run_sql.md): DuckDB streams the
  whole file regardless.

## JSON-RPC channel errors

### Garbled output / random "Parse error" messages

Something on the server is writing to **stdout** outside the
JSON-RPC protocol. Octa is careful to keep its banner and logs on
**stderr**, but:

- Some shells (zsh on macOS) emit warnings on stdout when launched
  in a subprocess. Test by running `octa --mcp` in a clean
  `bash --noprofile --norc`.
- A misconfigured Python or environment activation script
  prepended to your `command` could spit text. Verify your
  `command` in the MCP config points **directly at the Octa
  binary**, not through a wrapper that prints anything.

### Stdin closes unexpectedly

The MCP client crashed or restarted. The Octa server logs *"input
stream terminated"* to stderr and exits cleanly. Just restart the
client.

## Settings changes don't take effect

Octa reads `mcp_default_row_limit` and `mcp_default_cell_bytes`
**once at server startup**. You changed the value but the limit is
still what it was?

- **Claude Desktop**: fully quit and relaunch.
- **Claude Code**: in some versions the MCP subprocess persists
  across sessions; force a re-spawn with `claude mcp remove octa &&
  claude mcp add octa ...`.
- **MCP Inspector**: Ctrl-C the `npx` process, re-run.

Verify by checking the new startup banner on stderr:

```
octa --mcp ready (default row limit: 2500, cell cap: 65536 bytes; ...)
```

## Performance issues

### Slow responses on tiny files

The first call after `octa --mcp` starts has to:

1. Open the file via `FormatRegistry`.
2. Parse / decompress it.
3. Build the response JSON.

For multi-MB Parquet files there's no caching across calls
(each tool call is independent). If you'll call `read_table` then
`run_sql` on the same file, both pay the full load cost.

There's no client-side cache layer in Octa's MCP server yet.

### High memory use during a session

`run_sql` against very large Parquet files uses DuckDB's streaming
machinery; it shouldn't blow up RAM. But `read_table` with
`limit: 0` against a multi-million-row file pulls every row into
memory before sending. Use `run_sql` with `LIMIT` instead for big
files.

## Reporting bugs

If something is genuinely broken (not a configuration issue),
please file an issue at
[github.com/thorstenfoltz/octa/issues](https://github.com/thorstenfoltz/octa/issues).
Useful info to include:

- Octa version (`octa --version`).
- Your OS and how you installed Octa (binary download, AUR, built
  from source).
- The MCP client (Claude Desktop x.y.z, Claude Code, MCP
  Inspector, etc.).
- Exact `claude_desktop_config.json` (or equivalent), with paths
  redacted if needed.
- Stderr output of `octa --mcp` when reproducing.
- The exact tool call that failed and the error response.

## See also

- [Setup](setup.md) covers the initial wiring for each client.
- [Limits & truncation](limits-and-truncation.md) is the most
  common "this isn't returning what I expected" cause.
- [Tools reference](tools/index.md) lists the input schemas.
