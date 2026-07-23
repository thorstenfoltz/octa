# Database Connections

Connect Octa to nine live database engines: browse schemas and tables in
the sidebar, open a table read-only, query the server in its own SQL
dialect, join server tables against local files, copy tables between
servers, and (only when you opt in) write data back.

Supported engines:

- **PostgreSQL**
- **MySQL / MariaDB**
- **Microsoft SQL Server**
- **Amazon Redshift** (speaks the PostgreSQL wire protocol)
- **ClickHouse** (HTTP interface)
- **Exasol**
- **Snowflake** (SQL API)
- **Databricks** (SQL warehouse Statement Execution API)
- **Google BigQuery** (REST `jobs.query`)

Any managed or self-hosted service that speaks one of these wire
protocols works too, through the matching engine. In particular the
**PostgreSQL** and **MySQL / MariaDB** engines cover **Amazon RDS and
Aurora** (PostgreSQL- or MySQL-flavoured), **Azure Database for
PostgreSQL / MySQL**, **Google Cloud SQL**, and plain self-hosted
servers - pick PostgreSQL or MySQL and point the host at the managed
endpoint. These managed services also unlock their cloud IAM sign-in
options (AWS IAM, Microsoft Entra, Google Cloud SQL IAM) in the
authentication picker.

## Setting up a connection

![Database connection settings](../assets/screenshots/db-settings-connection.png)

Connections live under **Settings > Databases**. Each one stores:

- **Engine** - one of the nine above. The authentication picker below
  only offers the methods that engine supports.
- **Host / Port / Database / Username** - the port pre-fills the
  engine's default (5432 PostgreSQL, 3306 MySQL, 1433 SQL Server, 5439
  Redshift, 8123 ClickHouse, 8563 Exasol, 443 for Snowflake /
  Databricks / BigQuery). For the warehouse engines the **Database**
  field does double duty:
  - **Snowflake**: the account identifier is taken from the **Host**
      (the label before the first dot).
  - **Databricks**: put the **SQL warehouse id** in the Database
      field (the Statement API targets a warehouse).
  - **BigQuery**: put the **GCP project id** in the Database field;
      BigQuery datasets show up as schemas.
- **Authentication** - the methods offered depend on the engine (see
  the table below).
- **Allow writes** - off by default; see [Writes](#writes).

### Authentication methods by engine

| Engine                    | Methods                                                                                            |
|---------------------------|----------------------------------------------------------------------------------------------------|
| PostgreSQL, MySQL/MariaDB | Password, AWS IAM (RDS), Microsoft Entra (Azure AD), Google Cloud SQL IAM                          |
| SQL Server                | Password, Microsoft Entra (Azure AD)                                                               |
| Amazon Redshift           | Password, AWS IAM                                                                                  |
| ClickHouse, Exasol        | Password                                                                                           |
| Snowflake                 | Key-pair (JWT), Password, OAuth (browser SSO), OAuth (client credentials)                          |
| Databricks                | Personal access token, Microsoft Entra (Azure AD), OAuth (client credentials), OAuth (browser SSO) |
| Google BigQuery           | Application Default Credentials, Service-account key                                               |

A password is only one of the options. Several engines never use one at
all: they authenticate with a token, a private key, a browser sign-in, or
ambient cloud credentials. What each method needs:

- **Password**: a username and password. Stored in the system keyring,
  never in `settings.toml`. Offered by most engines but not all (BigQuery,
  for example, has no password mode).
- **AWS IAM (RDS)**: a token minted per connection via the aws CLI
  (`aws rds generate-db-auth-token`). Sign in first with
  `aws sso login`. An optional region overrides your aws CLI default.
  You can instead fill the **IAM Identity Center** fields (start URL,
  account ID, IAM role) to sign in with your browser from inside Octa
  (no `aws sso login`); Octa runs the Identity Center device flow, mints
  role credentials, and uses them to generate the RDS token. The aws CLI
  is still needed for that last signing step.
- **Microsoft Entra (Azure AD)**: no password. Octa gets a token either
  from the `az` CLI (`az account get-access-token`, after `az login`) or
  by signing you in through your browser from inside Octa (see
  [Two ways to sign in](#two-ways-to-sign-in-cli-or-browser)). It requests
  the right token audience per engine (SQL Server, Azure Database for
  PostgreSQL / MySQL, or Databricks).
- **Google Cloud SQL IAM** (PostgreSQL / MySQL): no password. Octa gets a
  token either from the `gcloud` CLI (`gcloud sql generate-login-token`,
  after `gcloud auth login`) or via browser sign-in from inside Octa. The
  username must be the IAM principal (for example `user@example.com`, or
  the service-account name without the `.gserviceaccount.com` suffix for
  MySQL).
- **Key-pair (JWT)** (Snowflake): point **Private key path** at an
  unencrypted PKCS#8 RSA private key. Octa mints a signed login JWT
  locally on each connect.
- **OAuth (browser SSO)** (Snowflake, Databricks): sign-in opens in your
  browser and the redirect is caught on a local port. For Databricks this
  is user-to-machine OAuth against the workspace; the built-in
  `databricks-cli` public client is used by default, so no client ID or
  secret is required (set **OAuth client ID** only for a custom app).
- **OAuth (client credentials)** (Snowflake / Databricks): a
  machine-to-machine grant. Supply the **Client ID** (and client
  secret in the keyring); the token URL defaults per engine when left
  blank.
- **Personal access token** (Databricks): a Databricks PAT, stored in
  the keyring.
- **Application Default Credentials** (BigQuery): uses gcloud ADC. Sign
  in first with `gcloud auth application-default login`.
- **Service-account key** (BigQuery): point **Service-account key
  path** at the JSON key file; Octa exchanges it for an access token.

The **Test connection** button connects with the values currently in
the form (saved or not) and runs `SELECT 1`, so a wrong host, password,
or database name surfaces immediately instead of on first use.

### Two ways to sign in: CLI or browser

For **Microsoft Entra (Azure AD)**, **Google Cloud SQL IAM** and **AWS
IAM (RDS)** connections, Octa can obtain the credential it needs in two
ways. Both end up connecting the same way; they differ only in how you
authenticate and what has to be installed. (**Databricks** also offers a
browser sign-in as its own **OAuth (browser SSO)** auth mode, described
below; it has no vendor-CLI path.)

**1. Vendor CLI (the default).** Octa shells out to `az` / `gcloud` to
mint a token. You sign in once inside the CLI (`az login`,
`gcloud auth login`), and the CLI keeps a long-lived session on disk and
silently refreshes it. Octa just asks it for a fresh token on each
connect.

- Pros: no setup in Octa; the CLI refreshes for you, so you rarely
  re-authenticate; this is the recommended path for a workstation you
  control.
- Cons: the CLI must be installed and signed in. On machines where you
  cannot install it (or in locked-down environments) this is a
  dead end.

**2. Browser sign-in (the fallback).** Octa opens your system browser,
you sign in there, and it captures the credential directly, with no CLI
at all. For Azure AD and Google you register a small OAuth client once in
your own cloud console and paste its ID into the connection; for AWS IAM
Identity Center and Databricks no registration is needed (Identity Center
is your organisation's own portal, and Databricks uses a built-in
client).

- Pros: needs no CLI; works anywhere a browser does; the same mechanism
  covers identity-provider logins that no CLI handles.
- Cons: a one-time app registration; and in this first version the
  browser session lasts about an hour with no background refresh, so you
  sign in again when it expires. For long unattended sessions, prefer the
  CLI.

You can set up both: if a browser token is present and still valid Octa
uses it, otherwise it falls back to the CLI.

#### Setting up browser sign-in

Setup is a one-time registration per provider:

- **Google**: in the Google Cloud console, create an OAuth client of type
  **Desktop app**. Put its client ID in **OAuth client ID** and its
  client secret in **OAuth client secret (Google)** on the connection.
- **Azure**: in Microsoft Entra ID, register an application as a **public
  client** with the redirect URI `http://localhost` and public-client
  flows enabled. Put its client ID in **OAuth client ID** and your
  directory (tenant) ID in **Azure tenant**.
- **AWS IAM Identity Center**: no registration. On an **AWS IAM (RDS)**
  connection, fill the **Identity Center start URL** (for example
  `https://acme.awsapps.com/start`), **AWS account ID** and **IAM role
  name** (plus the Identity Center region if it differs from the DB
  region). Octa runs the Identity Center device sign-in in your browser,
  mints temporary role credentials, and uses them to generate the RDS
  token (the aws CLI is still needed for that final signing step).
- **Databricks**: no registration and no fields. On a Databricks
  connection pick **OAuth (browser SSO)**; Octa signs in against the
  workspace with the built-in `databricks-cli` public client. Set an
  **OAuth client ID** only if you registered a custom app.

Once browser sign-in is configured, a **Sign in with browser** button
appears. It opens your browser, catches the redirect on a local port, and
caches the resulting token for this session. Every connection then uses
that token, and the connection is marked **Signed in via browser** in the
connection list.

## Connection examples

One example per engine, plus a managed-service variant. The values are
made up; substitute your own.

### PostgreSQL

| Field          | Value            |
|----------------|------------------|
| Engine         | PostgreSQL       |
| Host           | `db.example.com` |
| Port           | `5432`           |
| Database       | `analytics`      |
| Username       | `reporting`      |
| Authentication | Password         |

### Amazon RDS / Aurora (PostgreSQL, AWS IAM)

| Field          | Value                                           |
|----------------|-------------------------------------------------|
| Engine         | PostgreSQL                                      |
| Host           | `mydb.abc123xyz.eu-central-1.rds.amazonaws.com` |
| Port           | `5432`                                          |
| Database       | `analytics`                                     |
| Username       | `iam_user`                                      |
| Authentication | AWS IAM (RDS)                                   |

The engine is plain **PostgreSQL** - Aurora and RDS speak the PostgreSQL
wire protocol (use **MySQL / MariaDB** for the MySQL-flavoured ones).
**AWS IAM (RDS)** mints a short-lived token instead of a password, so no
secret is stored. Sign in with `aws sso login` first, or fill the
**IAM Identity Center** fields to sign in from your browser inside Octa.
The database user must be enabled for IAM auth (`GRANT rds_iam TO
iam_user` on PostgreSQL; the `AWSAuthenticationPlugin` on MySQL), and an
optional **region** overrides your aws CLI default. The same shape works
for **Azure Database** (Microsoft Entra) and **Google Cloud SQL** (Cloud
SQL IAM) by picking that authentication method.

### MySQL / MariaDB

| Field          | Value               |
|----------------|---------------------|
| Engine         | MySQL / MariaDB     |
| Host           | `mysql.example.com` |
| Port           | `3306`              |
| Database       | `shop`              |
| Username       | `app`               |
| Authentication | Password            |

### Microsoft SQL Server

| Field          | Value                         |
|----------------|-------------------------------|
| Engine         | SQL Server                    |
| Host           | `mssql.example.com`           |
| Port           | `1433`                        |
| Database       | `Sales`                       |
| Username       | `svc_octa`                    |
| Authentication | Password (or Microsoft Entra) |

### Amazon Redshift

| Field          | Value                                                      |
|----------------|------------------------------------------------------------|
| Engine         | Amazon Redshift                                            |
| Host           | `my-cluster.abc123xyz.eu-central-1.redshift.amazonaws.com` |
| Port           | `5439`                                                     |
| Database       | `prod`                                                     |
| Username       | `analyst`                                                  |
| Authentication | AWS IAM (region `eu-central-1`)                            |

Sign in first with `aws sso login`.

### ClickHouse

| Field          | Value                    |
|----------------|--------------------------|
| Engine         | ClickHouse               |
| Host           | `clickhouse.example.com` |
| Port           | `8123` (HTTP interface)  |
| Database       | `metrics`                |
| Username       | `default`                |
| Authentication | Password                 |

### Exasol

| Field             | Value                |
|-------------------|----------------------|
| Engine            | Exasol               |
| Host              | `exasol.example.com` |
| Port              | `8563`               |
| Database (schema) | `SALES`              |
| Username          | `sys`                |
| Authentication    | Password             |

### Snowflake

| Field          | Value                                              |
|----------------|----------------------------------------------------|
| Engine         | Snowflake                                          |
| Host           | `xy12345.eu-central-1.snowflakecomputing.com`      |
| Port           | `443`                                              |
| Database       | (optional; browse databases in the sidebar)        |
| Username       | `SVC_OCTA`                                         |
| Authentication | Key-pair (JWT), private key `~/.snowflake/octa.p8` |

The **account** is taken from the host label before the first dot
(`xy12345` here). Key-pair, password, browser SSO and OAuth client
credentials are all offered.

### Databricks

| Field          | Value                                         |
|----------------|-----------------------------------------------|
| Engine         | Databricks                                    |
| Host           | `dbc-a1b2c3d4-e5f6.cloud.databricks.com`      |
| Port           | `443`                                         |
| Database       | `1234567890abcdef` (the SQL **warehouse id**) |
| Username       | (leave blank)                                 |
| Authentication | Personal access token                         |

The Database field holds the SQL warehouse id, not a database name; the
warehouse's httpPath is `/sql/1.0/warehouses/<warehouse id>`, and only
the id goes here. Catalogs, schemas and tables appear in the sidebar
tree once connected. Besides a personal access token, Databricks also
offers Microsoft Entra (Azure AD), OAuth client credentials (M2M) and
**OAuth (browser SSO)** for browser sign-in with the built-in client.

### Google BigQuery

| Field          | Value                                     |
|----------------|-------------------------------------------|
| Engine         | Google BigQuery                           |
| Host           | (not used)                                |
| Port           | `443`                                     |
| Database       | `my-gcp-project` (the GCP **project id**) |
| Username       | (not used)                                |
| Authentication | Application Default Credentials           |

Sign in first with `gcloud auth application-default login`. The Database
field is the default project; the sidebar can browse other projects your
credentials can access.

## Browsing

![Databases sidebar tree](../assets/screenshots/db-sidebar-tree.png)

**File > Databases** toggles a sidebar tree of your connections.
Expand a connection to list its schemas, expand a schema to list its
tables, and click a table to open its first rows in a tab (the
initial-load row cap applies, like opening a large file).

**Right-click a table** for **Copy to another connection...** and
**Show metadata...**. "Show metadata" opens a read-only tab with the
table's columns; on Databricks it runs `DESCRIBE TABLE EXTENDED`, so the
tab also carries the detailed table information (location, format, owner,
properties). Other engines return their column schema
(`information_schema.columns` or the engine's `DESCRIBE`).

Snowflake, Databricks and BigQuery have a three-level namespace, so
their tree has an extra top level: **catalog > schema > table** (a
Snowflake database, a Databricks catalog, or a BigQuery project). Each
level loads when you expand it. Browsing every BigQuery project needs
the `resourcemanager.projects.list` permission; the connection's token
uses the cloud-platform scope, which covers it. The other engines stay
two-level: MySQL/MariaDB, ClickHouse and Exasol are genuinely
two-level, and a PostgreSQL / Redshift / SQL Server connection browses
the one database it is connected to.

Octa keeps one live connection per saved connection and reuses it
across sidebar listings, table opens, and server queries (a dead
connection reconnects automatically), so browsing several servers side
by side stays snappy. Editing a connection in Settings drops its cached
connection.

A database tab is **read-only** unless its connection has **Allow
writes** on *and* Octa can discover a primary key for the table - see
[Editing and write-back](#editing-and-write-back). Read-only tabs show
the usual `[Read-only]` pill and a dismissible note explaining why.
**ClickHouse** and **BigQuery** tables have no discoverable primary key,
so their tabs always open read-only; query and copy them, and write with
**Run on** the server or **Write result to DB...**.

## Editing and write-back

When a connection has **Allow writes** on and the opened table has a
primary key, its tab opens fully editable: edit cells, insert and
delete rows, add columns - the same tools as any file tab, undo
included. Nothing reaches the server until you save.

**Ctrl+S** (Save) diffs your edits against the loaded baseline and
shows a confirmation dialog listing exactly what would change on the
server: how many updates, inserts, and deletes, plus any added columns,
and the target `schema.table @ connection`. Confirm and Octa applies
the whole diff in **one transaction**, keyed by the primary key
(`ALTER TABLE ADD` for new columns, `DELETE` / full-row `UPDATE` /
`INSERT` per row). If anything fails the transaction rolls back and
your edits stay in the tab, so you can fix the problem and save again.

Things to know:

- **No primary key, no editing.** Without one, edits could not be
  addressed to server rows; the tab stays read-only and a banner says
  so.
- **Only the loaded rows are compared.** The tab holds the initial-load
  window; rows beyond it are never touched by a save. Inserts always
  append.
- **Last writer wins.** Changes made on the server between your load
  and your save are overwritten by the full-row update. Reload the tab
  before editing if others write to the table.
- **Local SQL mutations lose row identity.** Running a local DuckDB
  mutation on the tab rewrites the snapshot; a later save refuses with
  a "row identity lost" message. Reload the table, or use **Run on**
  the server for mutations.
- **Save As detaches.** Saving the tab to a file exports it and turns
  it into an ordinary file tab; it no longer writes back to the
  server.

## Copying a table between servers

<!-- SCREENSHOT: db-copy-dialog.png: The "Copy table to another connection" dialog: source line "admin.people @ MariaDB-Test", target-connection dropdown showing "Post-Test (PostgreSQL)", target schema "public", target table "people", mode "Create new", Copy button with a green "Copied 3 row(s)." status. -->
![Copy table between servers dialog](../assets/screenshots/db-copy-dialog.png){ .screenshot-placeholder }

Right-click a table in the sidebar tree and pick **Copy to another
connection...** to copy it into a different server (for example MySQL
to Snowflake). Pick the target connection, schema, and table name, and
a mode: **Create new** (error if the table exists), **Append**, or
**Replace** (drop and recreate). Copy works **between any two of the
nine engines**, in either direction; the dialog annotates which lane a
given pair uses.

There are two lanes, chosen automatically:

- **Fast lane** - when *both* engines are DuckDB-attachable
  (PostgreSQL, MySQL/MariaDB, Redshift). DuckDB attaches the source
  read-only and the target writable and runs one
  `INSERT INTO ... SELECT`. The data never passes through Octa's table
  model, so there is no row cap and no memory blow-up, and writes to
  PostgreSQL use the binary COPY protocol - far faster than row-by-row
  INSERTs. The DuckDB `postgres` / `mysql` extensions install over the
  network on first use (then cached).
- **Universal lane** - any other pair (a warehouse, ClickHouse, Exasol,
  SQL Server on either side). Octa pulls the source in batches and
  writes each batch to the target. It is slower because the data passes
  through Octa, but it works for every engine combination.

Either way, the target connection needs **Allow writes**. Agents can do
the same via the `copy_db_table` MCP / Assistant tool.

## SQL: server or local

On a database tab the [SQL panel](sql.md) gains a **Run on** toggle:

- **The connection name** (default): the query runs on the server, in
  the engine's native SQL dialect, on a background thread. A Cancel
  button appears while it runs, and it works on every engine (see
  [Cancelling a running query](#cancelling-a-running-query)).
- **local DuckDB**: the query runs against the loaded snapshot as
  `data`, exactly like any other tab.

Mutations run on the server report rows affected; they are refused
unless the connection allows writes.

Query results are **streamed and capped** at the initial-load row limit
(Settings > Performance, default 5,000,000), so a `SELECT *` on a huge
table cannot exhaust memory; the row counter notes when the cap was
reached. The CLI lifts it with `--rows N|all`, agents with
`unlimited: true`.

## Joining server tables with local files

The SQL workspace's **Attach connection** menu attaches a saved
database read-only, so its tables join against local files:

- PostgreSQL, MySQL/MariaDB and Redshift attach natively through
  DuckDB's `postgres` / `mysql` extensions (installed over the network
  on first use); address tables as `alias.schema.table`. The alias is
  the connection name lowercased with spaces and punctuation as `_`
  ("Post-Test" becomes `post_test`); you never have to guess it - the
  **Attached connections** box next to the Inspector lists each alias
  with a one-click example query, and clicking any attached table in
  the workspace tree offers Copy / Insert / Run for its qualified name.
- The other engines (SQL Server, Snowflake, Databricks, BigQuery,
  ClickHouse, Exasol) have no native DuckDB extension, so their tables
  are **imported** individually as `alias__schema__table` workspace
  tables. The import is **row-capped** at the initial-load limit and
  servers with very many tables are refused - query those with
  **Run on** instead, or copy the table you need first.

You do not even need a table open: the SQL panel opens on an empty tab
too (Analyse > SQL), attach your connections and query the servers
directly - cross-server JOINs and UNIONs included. Without a table
there is simply no `data` in the workspace. Every result shows a row
counter directly above the grid.

The **Write result to DB...** dialog also lists your connections as
targets, writing the current result rows into a server table.

## Writes

Every connection is **read-only by default**. Server-side mutations
(INSERT / UPDATE / DELETE / DDL), the write-back target, the CLI
`--db-write-table`, and the MCP `write_db_table` tool are all refused
until you switch on **Allow writes** for that specific connection in
Settings. A "writes ON" badge in the sidebar marks opted-in
connections.

## CLI

```bash
octa --db-tables --db warehouse
octa --db-query "SELECT * FROM public.users LIMIT 10" --db warehouse
octa --db-write-table staging.users --db warehouse users.parquet --db-write-mode replace
```

`--db` takes the connection's name (case-insensitive) or id. See the
[man page](../cli/man-page.md) for details.

### Catalogs from the command line

Snowflake, Databricks and BigQuery have a catalog level above the
schema. Pass it with `--db-catalog`:

```bash
# list the catalogs
octa --db warehouse --db-tables

# list the tables inside one
octa --db warehouse --db-tables --db-catalog sales_prod

# write into a table in one
octa --db warehouse --db-write-table analytics.daily \
     --db-catalog sales_prod rows.parquet
```

Without `--db-catalog`, `--db-tables` on those three engines lists the
catalogs rather than recursing into every schema of every catalog. On
the other six engines `--db-catalog` is an error, because they have no
catalog level.

### Copying a table between servers

```bash
octa --db source_conn --db-copy analytics.orders \
     --db-copy-to target_conn \
     --db-copy-target reporting.orders \
     --db-write-mode replace
```

The target table defaults to the source schema and table, so
`--db-copy-target` is optional. On a three-level engine name the
catalogs with `--db-catalog` (source) and `--db-copy-target-catalog`
(target). The target connection's **Allow writes** switch must be on.
PostgreSQL, MySQL/MariaDB and Redshift copy directly server to server;
the other engines stream through Octa, exactly as the dialog's two
lanes do.

## Cancelling a running query

The SQL panel's Cancel button stops a running statement on every
engine:

| Engine                          | How it cancels                                          |
|---------------------------------|---------------------------------------------------------|
| PostgreSQL, Redshift            | Protocol-level cancel request                           |
| Snowflake, Databricks, BigQuery | The vendor's cancel API, so the warehouse stops billing |
| ClickHouse                      | `KILL QUERY` by query id                                |
| MySQL/MariaDB                   | `KILL QUERY` from a second connection                   |
| Exasol                          | `KILL STATEMENT IN SESSION` from a second connection    |
| SQL Server                      | `KILL` from a second connection                         |

Two limits are worth knowing. SQL Server's `KILL` ends the whole
session rather than the single statement and needs the
`ALTER ANY CONNECTION` permission, so Octa reconnects afterwards. And
cancellation covers the SQL panel: opening a large table from the
sidebar and copying a table both run to completion.

## MCP / Assistant

Agents get five tools: `list_db_connections`, `list_db_tables`,
`query_db` (native-dialect SQL; mutations gated on Allow writes),
`write_db_table`, and `copy_db_table` (the last two dropped entirely
under `--mcp-read-only`). The in-app [Assistant](chatbot.md) has the
same tools against your saved connections.

On Snowflake, Databricks and BigQuery the catalog level is a parameter:
`list_db_tables` and `write_db_table` take `catalog`, and
`copy_db_table` takes `source_catalog` and `target_catalog`. Calling
`list_db_tables` on one of those engines without `catalog` returns
`kind: "catalogs"` and the catalog list, so the agent calls it again
with one of them to drill down. Passing a catalog to any of the other
six engines is an error.

<!-- screenshot placeholder: Settings > Databases with a connection form -->
<!-- screenshot placeholder: Databases sidebar tree with schemas and tables -->
<!-- screenshot placeholder: SQL panel "Run on" toggle on a database tab -->
