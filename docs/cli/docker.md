# Docker / Podman

Octa ships a headless container image for the command-line actions and the
[MCP server](../mcp/index.md). The GUI is not available inside the container;
the image is built for automation: running one-shot CLI actions in a pipeline
or hosting `octa --mcp` as a stdio server.

The same `Dockerfile` works with Podman; swap `docker` for `podman` in every
command below.

## Why there is no GUI in the image

Octa is a single binary that is both the desktop app and the CLI/MCP server.
The windowing libraries (GTK, X11, Wayland) are loaded lazily at runtime, and
the headless paths (`--mcp` and the CLI flags) never touch them. The runtime
image therefore drops the entire GUI library stack and ships only what the
data engine needs (glibc, the C++ runtime, and liblzma). The result is a small
image based on
[distroless](https://github.com/GoogleContainerTools/distroless)'s `cc`
variant.

!!! note "Why not Alpine?"

    Octa bundles DuckDB and SQLite, which compile C/C++ from source. Those
    builds target glibc and are not tested against Alpine's musl libc, so the
    image is built on a glibc base (`distroless/cc-debian12`) for reliability
    rather than chasing the smallest possible Alpine image.

## Pull the prebuilt image

Released images are published to the GitHub Container Registry, so you do not
need the Rust toolchain or a local build. Pull `latest`, or pin an exact
release version (the tag matches the release version exactly):

```bash
docker pull ghcr.io/thorstenfoltz/octa:latest
docker pull ghcr.io/thorstenfoltz/octa:0.7.9
```

The image is published for `linux/amd64`. A new image is pushed automatically
with every release, so `latest` always tracks the newest release.

The examples below use `octa` as the image name for brevity. After pulling,
either reference `ghcr.io/thorstenfoltz/octa:latest` directly, or tag it once:

```bash
docker tag ghcr.io/thorstenfoltz/octa:latest octa
```

## Build

If you want an unreleased build (for example from a branch), build the image
yourself instead of pulling:

```bash
docker build -t octa .
```

The build is a multi-stage build: a `rust:1-bookworm` stage compiles the
release binary (this is the slow part - DuckDB is compiled from source), then
the binary is copied into the distroless runtime stage.

## Run a one-shot CLI action

Mount a directory of data files and pass any CLI flag. The container's
entrypoint is the `octa` binary, so flags go straight after the image name:

```bash
docker run --rm -v "$PWD:/data" octa --schema /data/file.parquet
docker run --rm -v "$PWD:/data" octa --describe /data/sales.csv
docker run --rm -v "$PWD:/data" octa --sql "SELECT COUNT(*) FROM data" /data/sales.csv
docker run --rm -v "$PWD:/data" octa --convert /data/in.csv /data/out.parquet
```

`--rm` cleans up the container after the action finishes. `-v "$PWD:/data"`
makes the current directory available inside the container at `/data`.

!!! warning "Writing output as a non-root user"

    The container runs as the non-root user `octa` (uid 65532), not root.
    Read-only actions (`--schema`, `--describe`, `--sql`, `--mcp`, ...) work
    against any mounted directory the host makes readable. Actions that
    **write** back to a mounted directory (`--convert`, `--sql-write-to`, or
    saving an edited file) need that directory writable by uid 65532. If the
    write fails with a permission error, either make the host directory
    group/other-writable, or run the container as your own user so the output
    file is owned by you:

    ```bash
    docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/data" \
      octa --convert /data/in.csv /data/out.parquet
    ```

## Run the MCP server

The MCP server speaks JSON-RPC over stdio, so it needs an interactive stdin
(`-i`):

```bash
docker run --rm -i -v "$PWD:/data" octa --mcp
```

Point your MCP client at that command. A minimal client config:

```json
{
  "mcpServers": {
    "octa": {
      "command": "docker",
      "args": ["run", "--rm", "-i", "-v", "/abs/path/to/data:/data", "octa", "--mcp"]
    }
  }
}
```

Inside the container, refer to files by their mounted path (e.g.
`/data/file.parquet`). See the [MCP setup guide](../mcp/setup.md) for the tool
list and caps.

## Use with Claude Code

The container is the easiest way to give [Claude
Code](https://claude.com/claude-code) the Octa MCP tools **without installing
Octa**. You do not need the Rust toolchain or an `octa` binary on your `PATH`,
only Docker and the image built above. Claude Code launches the container as a
stdio MCP server and talks to it over stdin/stdout.

Register it with one command:

```bash
claude mcp add octa -s user -- docker run --rm -i -v /ABS/PATH/TO/DATA:/data octa --mcp
```

Everything after `--` is the exact command Claude Code runs for the server. The
trailing `octa` is the **image tag** (from `docker build -t octa .`), not a
local binary; `--mcp` is the flag passed to the binary inside the image.

Then start Claude Code and run `/mcp` (or `claude mcp list`) to confirm the
server connected. Ask something like *"use octa to show the schema of
/data/sales.parquet"*.

The flags matter:

- **`-i`** keeps stdin open so the JSON-RPC stream works. It is required. Do
  **not** add `-t`, a TTY corrupts the protocol.
- **`--rm`** removes the container when the session ends.
- **`-v /host/dir:/data`** mounts your data. The model must reference files by
  their **in-container** path (`/data/...`), not the host path.

The server starts **once** and stays up for the whole session (a single
current-thread runtime that serialises tool calls), so individual tool calls do
not spawn new containers.

!!! tip "Make host and container paths line up"

    Because the model only sees `/data/...`, mounting at the same absolute path
    as the host avoids path confusion, then a file's host path and container
    path are identical:

    ```bash
    claude mcp add octa -s user -- docker run --rm -i \
      -v /home/me/data:/home/me/data octa --mcp
    ```

**Scopes.** `-s local` (default) registers it for the current project only;
`-s project` writes a committable `.mcp.json` you can share with your team;
`-s user` makes it available across all your projects. Inspect or remove it with
`claude mcp get octa` / `claude mcp remove octa`.

The equivalent `.mcp.json` entry (what `-s project` writes) is the same generic
config shown above under [Run the MCP server](#run-the-mcp-server).

## Podman

Identical commands, `podman` instead of `docker`:

```bash
podman build -t octa .
podman run --rm -i -v "$PWD:/data" octa --mcp
```

## Image contents

- `/usr/local/bin/octa` - the binary (entrypoint).
- `/usr/share/octa/` - `LICENSE`, `THIRD_PARTY_LICENSES.md`, and the
  `licenses/` directory, mirroring what `install.sh` ships.
- The container runs as the non-root user `octa` (uid 65532) with home
  `/home/octa`, not root.
