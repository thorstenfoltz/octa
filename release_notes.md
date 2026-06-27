# Release notes

This release brings Octa to the cloud. You can now browse and open files
straight from object storage, and the CLI assistant and MCP server can read and
write cloud objects too. It also adds git-aware comparison, correlation
analysis, chat transcript export, and a tidier toolbar.

## Cloud storage

**Open files from S3, Azure, and Google Cloud.** A new **File > Cloud
connections** sidebar lists your saved buckets, lets you expand folders, and
opens a file with a click, exactly like a local file, so every supported format
works. Connections cover Amazon S3 (and S3-compatible providers such as IONOS,
MinIO, and Cloudflare R2), Azure Blob Storage, and Google Cloud Storage.

**Your choice of credentials.** Save a static key or SAS token on a connection,
sign in through the cloud's own CLI (browser SSO), or fall back to whatever is
already in your environment (`AWS_*` variables, a cached SSO session, an Azure
CLI login, or Google application-default credentials). Saved secrets go into
your operating system keyring when one is available. Public, anonymous buckets
need no credentials at all.

**Saving back is off by default.** Cloud-opened files are read-only until you
turn on **Allow writing to cloud storage** in **Settings > Cloud storage**.
After that, **Save** writes the tab back to its original object in the
background. The same switch lets the in-app assistant write to cloud URLs.

**The assistant and MCP server speak cloud URLs.** Point the assistant or an MCP
tool at an `s3://`, `az://`, or `gs://` URL and Octa reads it like a local file.
A new `list_objects` tool browses a bucket. The headless MCP server writes to
cloud URLs using your ambient credentials; run it with `--mcp-read-only` to drop
every write tool, cloud and local alike.

## Comparing and analysing

**Compare against git history.** For a file in a git repository you can now diff
the current version against a recent commit, or open an older revision in its own
tab, without leaving Octa.

**Correlation analysis.** A new correlation dialog produces a Pearson or Spearman
correlation matrix across the numeric columns of the active table, opened in a
detached result tab.

## Assistant

**Export a chat transcript.** Save a chat session to Markdown for a readable
record to share, or to JSON for an exact, machine-readable archive.

**Tune how much the assistant sees.** The number of result rows a tool hands to
the model is now a setting (**default 200**). Truncation only limits what the
model reads, never the underlying query result, so large tables no longer flood
the conversation.

## Toolbar

**Reorganised menus.** Toolbar actions are grouped into dedicated **Columns** and
**Data** sections, and menu items now carry hover hints so each action is easier
to find.
