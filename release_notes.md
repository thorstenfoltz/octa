# Release notes

This release connects Octa to live databases. Alongside the files it already
opens, Octa now talks to nine running database engines: **PostgreSQL**,
**MySQL/MariaDB**, **Microsoft SQL Server**, **Amazon Redshift**,
**ClickHouse**, **Exasol**, **Snowflake**, **Databricks**, and **Google
BigQuery**. Browse their tables in the sidebar, query them in their own SQL
dialect, join server tables against local files, and, when you opt in, edit
rows and write them straight back. The chat assistant also moves to OpenAI's
Responses API and gains per-profile write control.

## Database connections

**Connect to a live database.** A new **Settings > Databases** section holds
your connections (engine, host, port, database, user). **File > Databases**
then shows them as a sidebar tree: expand a connection for its schemas, expand
a schema for its tables, click a table to open its first rows in a tab, exactly
like opening a file.

**Warehouses and managed databases, not just self-hosted servers.** **Amazon Redshift**,
**ClickHouse**, **Exasol**, and the three cloud data warehouses **Snowflake**,
**Databricks**, and **Google BigQuery** are supported. Because the PostgreSQL and MySQL
engines speak the standard wire protocols, they also reach any wire-compatible
managed service: **Amazon RDS and Aurora**, **Azure Database for
PostgreSQL/MySQL**, and **Google Cloud SQL** all connect by pointing the host
at the managed endpoint. For the warehouses that name things in three parts
(Snowflake, Databricks, BigQuery) the sidebar tree gains a matching third
level, **catalog > schema > table**.

**See a table's metadata.** Right-click a table in the sidebar and pick **Show
metadata...** to open a read-only tab with its columns. On Databricks that runs
`DESCRIBE TABLE EXTENDED`, so the tab also carries the detailed table
information (location, format, owner, properties); the other engines return
their column schema.

**Sign in the way your database expects.** A password is only one option, and
several engines never use one at all. Beyond a plain password (kept in the
system keyring, never in a config file), the cloud databases authenticate with
IAM tokens (**AWS IAM** for RDS and Aurora, **Microsoft Entra (Azure AD)**,
**Google Cloud SQL IAM**), and the warehouses bring their own: **Snowflake**
takes a key-pair (JWT), a password, or OAuth; **Databricks** takes a personal
access token or OAuth; **BigQuery** uses Application Default Credentials or a
service-account key. The authentication picker only ever shows the methods the
chosen engine actually supports.

**Sign in through your browser, no CLI required.** For **Azure AD**, **Google**,
**AWS IAM Identity Center**, and **Databricks**, Octa can now sign you in
directly in your web browser and cache the token for the session, so you no
longer need an interactive CLI login (`az login`, `gcloud auth login`,
`aws sso login`) on the machine. Click **Sign in with browser** on the
connection: for Azure and Google you paste a one-time OAuth client id from your
own cloud console; for AWS Identity Center you fill the portal start URL,
account, and role; Databricks needs nothing at all (it uses a built-in client).
For Azure, Google, and Databricks this needs no CLI at all; AWS still uses the
`aws` command to sign the final RDS token. The vendor CLI login also still works
as before, and Octa prefers a cached browser token when one is present.

**Test before you save.** A **Test connection** button opens a throwaway
connection with whatever is in the form right now and runs `SELECT 1`, so a
wrong host or password shows up immediately rather than on first use.

**Edit a table and write it back.** When a connection has **Allow writes** on
and the table has a primary key, its tab opens fully editable: change cells,
insert and delete rows, add columns, with undo, just like a file. Nothing
reaches the server until you press **Save**, which shows a confirmation listing
exactly what will change (so many updates, inserts, deletes, plus any new
columns) against `schema.table @ connection`. Confirm and Octa applies the whole
diff in **one transaction**, keyed by the primary key; if anything fails it
rolls back and your edits stay put. Tables without a primary key stay read-only,
and a banner explains why.

**Query the server or the local copy.** On a database tab the SQL panel gains a
**Run on** toggle: run the query on the server in its native dialect (on a
background thread, with a Cancel button), or against the loaded snapshot in
local DuckDB as before. Server results are streamed and capped at the row limit
so a `SELECT *` on a huge table cannot exhaust memory.

**Cancel a running query at the source.** The Cancel button now stops the
statement on the database itself, not just Octa's wait for it. Each engine is
cancelled the way it expects: PostgreSQL and Redshift through the wire
protocol's own cancel request, MySQL, SQL Server, and Exasol by opening a second
connection and issuing KILL against the running session, ClickHouse with a
targeted `KILL QUERY`, and the three warehouses (Snowflake, Databricks,
BigQuery) by stopping the REST poll and calling the vendor's cancel endpoint. A
runaway query frees the server the moment you press Cancel.

**Join server tables with local files.** The SQL workspace's **Attach
connection** menu attaches a saved database read-only. PostgreSQL and MySQL
attach natively (address tables as `alias.schema.table`); SQL Server tables are
imported individually. Cross-server JOINs and UNIONs against your local files
work in one query, and the SQL panel opens on an empty tab too, so you can query
servers with no file open at all.

**Copy a table between servers.** Right-click a table in the sidebar and pick
**Copy to another connection...** to move it into a different server (say MySQL
to PostgreSQL, or Snowflake to SQL Server). It works between **any two of the
nine engines**, in either direction, and picks the fastest route on its own (the
dialog tells you which one it will use). When both sides are PostgreSQL,
Redshift, or MySQL, the copy streams **server-to-server through DuckDB** in one
`INSERT INTO ... SELECT`, so the data never passes through Octa's table model:
no row cap, no memory blow-up, and writes to PostgreSQL use the fast binary COPY
protocol. For any other pair, including SQL Server and the three warehouses, Octa
pulls the table in batches and writes it to the target: slower, since the data
passes through Octa, but the whole table still lands.

**Read-only by default, everywhere.** Every connection ships read-only. Editing,
write-back, server-side mutations, the CLI `--db-write-table`, and the MCP
`write_db_table` tool are all refused until you switch on **Allow writes** for
that specific connection. A "writes ON" badge marks the ones you have opted in.

**From the command line and for agents, too.** New CLI flags `--db-tables`,
`--db-query`, `--db-write-table`, and `--db-copy` reach your saved connections
from a script (with `--db <name>`, `--db-write-mode` for how a write lands, and
`--db-copy-to` for the copy's target). On the three-level engines a
`--db-catalog` picks the catalog. Agents and the in-app assistant get the
matching tools: `list_db_connections`, `list_db_tables`, `query_db`,
`write_db_table` (dropped under `--mcp-read-only`), and `copy_db_table` for the
server-to-server copy; the catalog engines take a catalog name on each call
(`catalog`, or `source_catalog` and `target_catalog` for a copy). When one of
these writes creates a new table, its columns use the target engine's own SQL
types.

## Chat assistant

**OpenAI now uses the Responses API.** OpenAI requests go through
`/v1/responses`, the endpoint where reasoning and tools work together on current
models. gpt-5.x refuses a reasoning effort alongside tools on the older Chat
Completions endpoint, so this is what lets you set an effort level and still have
the assistant drive Octa's tools. OpenAI-compatible providers and Ollama stay on
Chat Completions.

**Write permission is per model profile now.** Each chat model profile carries
its own **Allow writes** switch, off by default. A profile without it never even
sees the write tools; a profile with it can edit open tabs and write to
databases whose connection also allows writes (both switches must agree). So you
might keep a trusted "editor" profile with writes on and leave your everyday one
read-only. The global **Write protection** setting no longer applies to the
assistant, but still governs GUI file saves and the MCP server default.

## Maintenance

**Dependencies updated** across the tree, and a security advisory in the SQL
Server driver's TLS stack (unreachable in Octa's usage, and with no upstream fix
available) is documented and suppressed rather than left as a noisy CI failure.

**No more stray "Untitled" tab.** Creating a new file while sitting on Octa's
empty startup tab no longer leaves a second blank "Untitled" beside it. New file
now reuses the blank tab you are already on, the same way opening a file does.

