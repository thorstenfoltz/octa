# MCP Setup

Three common MCP clients (Claude Desktop, Claude Code, and the
MCP Inspector) each have a slightly different way to register a
local MCP server. This page walks through all three.

## Prerequisites

1. Octa must be installed and on your `PATH`. Verify with:

    ```bash
    octa --version
    ```

    If you get *"command not found"*, either move the binary somewhere
    on `PATH` (`/usr/local/bin/`, `~/.local/bin/`, `C:\Program
    Files\Octa\`) or use the **full path** in the configurations below
    (`/home/you/octa/target/release/octa`, `C:\Tools\octa.exe`, etc.).

2. Confirm `--mcp` starts the server:

    ```bash
    octa --mcp
    ```

    You should see a one-line startup banner on **stderr**:

    ```
    octa --mcp ready (default row limit: 1000, cell cap: 65536 bytes; …)
    ```

    Press Ctrl+C to stop. Stdout is reserved for JSON-RPC traffic;
    you won't see anything on stdout unless an MCP client is talking
    to it.

## Read-only mode

Add `--mcp-read-only` to expose a **read-only tool surface**: every
file-writing tool is omitted from the server, so an agent wired to Octa
can inspect and query data but cannot modify files. The dropped tools are:

- `write_table`
- `edit_table`
- `convert`
- `transform_columns`
- `anonymize`
- `partition_table`

Every other tool stays available, including the read-only analytics
(`pivot`, `correlation`, `grep_files`) and `list_objects`. For cloud
objects this is also the only write gate: with `--mcp-read-only` the
server can read from `s3://` / `az://` / `gs://` URLs but never write
back to them.

```bash
octa --mcp --mcp-read-only
```

The startup banner notes the mode:

```
octa --mcp ready [read-only: write tools disabled] (...)
```

Use it in any client config by appending the flag to `args`, e.g.
`"args": ["--mcp", "--mcp-read-only"]`.

## Claude Desktop

Claude Desktop reads its MCP servers from `claude_desktop_config.json`:

| Platform | Path                                                              |
|----------|-------------------------------------------------------------------|
| Linux    | `~/.config/Claude/claude_desktop_config.json`                     |
| macOS    | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows  | `%APPDATA%\Claude\claude_desktop_config.json`                     |

Open the file (create it if it doesn't exist) and add `octa` to the
`mcpServers` block:

```json
{
  "mcpServers": {
    "octa": {
      "command": "octa",
      "args": ["--mcp"]
    }
  }
}
```

If `octa` isn't on `PATH`, use the full path:

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

The AppImage works the same way: point `command` at the AppImage
file directly. No extraction, no wrapper script, no separate
install step. The same single-file binary that opens the GUI also
serves as the MCP endpoint.

```json
{
  "mcpServers": {
    "octa": {
      "command": "/home/you/Octa-x86_64.AppImage",
      "args": ["--mcp"]
    }
  }
}
```

Save the file and **restart Claude Desktop**. You should see the
**hammer icon** (🔨) in the conversation window; click it to
confirm Octa's tools are listed.

Try a prompt:

> What columns does `/path/to/data.parquet` have?

Claude should call `schema` and report the result.

## Claude Code

Claude Code registers MCP servers with `claude mcp add`. No config file
to edit:

```bash
claude mcp add octa -- octa --mcp
```

The bare `--` separates Claude Code's own flags from the command it
should spawn. Everything after it is the command line Octa is launched
with, so `octa --mcp` is what actually runs.

### Choosing a scope

`claude mcp add` writes to one of three scopes. Without `--scope` you
get **local**, which only applies in the directory you ran the command
from. That is rarely what you want:

```bash
# Every project, just for you (the usual choice)
claude mcp add --scope user octa -- octa --mcp

# This project only, checked into the repo as .mcp.json
claude mcp add --scope project octa -- octa --mcp

# This project, just for you (the default if --scope is omitted)
claude mcp add --scope local octa -- octa --mcp
```

Use **project** scope when you want the server to travel with the
repository, so anyone who clones it picks Octa up automatically. Use
**user** scope for a personal setup that follows you everywhere. `-s` is
accepted as a short form of `--scope`.

### Read-only registration

Append `--mcp-read-only` to drop the file-writing tools (see
[Read-only mode](#read-only-mode) above). It goes after `--mcp`, on
Octa's side of the `--`:

```bash
claude mcp add --scope user octa -- octa --mcp --mcp-read-only
```

### Verify

```bash
claude mcp list
```

This runs a health check against each registered server, so a broken
`command` path shows up immediately rather than at first use:

```text
octa: octa --mcp - Connected
```

Start a new Claude Code session (existing sessions do not pick up newly
registered servers) and the tools appear namespaced as
`mcp__octa__read_table`, `mcp__octa__run_sql`, `mcp__octa__schema`, and
so on. Then ask things like:

> Use the octa MCP server to read the schema of `tests/fixtures/sample.csv`.

### Removing

```bash
claude mcp remove octa --scope user
```

!!! note "Multiple Claude Code configurations"

    `claude mcp add` writes into whichever configuration directory is
    active, which `CLAUDE_CONFIG_DIR` controls. If you run more than one
    Claude Code identity from separate config directories, register Octa
    once per directory. `claude mcp list` always reports the active one.

## MCP Inspector

The [MCP Inspector](https://github.com/modelcontextprotocol/inspector)
gives you a web UI for exploring an MCP server: list tools, fill in
parameters via forms, see raw JSON responses. The best way to verify
a server works without involving an AI client at all.

```bash
npx @modelcontextprotocol/inspector octa --mcp
```

This spawns Octa under Inspector's control, opens a browser tab, and
shows you every tool plus an interactive form for each.

![MCP Inspector with Octa connected](../assets/screenshots/mcp-inspector.png)

Requires Node.js (and `npx`) on your `PATH`. The Inspector is the
fastest path to "does my Octa MCP setup work?"

## Other MCP clients

Any client that supports stdio-spawned MCP servers works with Octa.
The pattern is always:

- **command**: `octa` (or the full path to the binary or AppImage)
- **args**: `["--mcp"]`
- **transport**: stdio (the default; no special config needed)

Refer to your client's documentation for the exact config syntax;
the entries above are representative.

## Distribution formats

`octa --mcp` works with every distribution Octa publishes:

- **Plain binary** off the releases page (`/usr/local/bin/octa`,
  `~/.local/bin/octa`, or anywhere on `PATH`).
- **`install.sh`** install (system-wide or user-local).
- **AUR** packages (`octa`, `octa-bin`).
- **AppImage** (`Octa-*-x86_64.AppImage`), pointed at directly as
  the `command`.

No wrapper script or extra installation step is needed in any
case: the same binary that opens the GUI also serves as the MCP
endpoint.

## After setup

Once Octa's tools show up in your client, configure the limits
under Octa's GUI ([**Settings → MCP**](../reference/settings.md#mcp)):

- **Default response row limit**: 1000 by default. Set higher (or
  Unlimited) for analytics workflows where Claude needs to see
  whole tables.
- **Per-cell byte cap**: 65,536 by default. Lower if a BLOB column
  is consistently bloating responses.

The streaming file-loader cap lives under
[**Settings → Performance → Initial-load row cap**](../reference/settings.md#performance)
and defaults to 5,000,000 rows; an Unlimited checkbox next to
the input disables it entirely. Per-MCP-call, pass `unlimited: true`
to any read-bearing tool to lift this cap for that call only.

Settings are read once at server startup. After changing them in
Octa, restart your MCP client (or just the Octa server process) for
them to take effect.

## See also

- [Tools reference](tools/index.md) covers what each tool does and
  the input schemas.
- [Limits & truncation](limits-and-truncation.md) covers how Octa
  keeps responses bounded.
- [Troubleshooting](troubleshooting.md) covers common setup
  failures.
