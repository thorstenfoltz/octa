# Database Connections

Connect Octa to twelve live database engines: browse schemas and tables in
the sidebar, open a table read-only, query the server in its own SQL
dialect, join server tables against local files, copy tables between
servers, and (only when you opt in) write data back.

Supported engines:

- **PostgreSQL**
- **MySQL / MariaDB**
- **Microsoft SQL Server**
- **Oracle Database** (12.1 and later)
- **Amazon Redshift** (speaks the PostgreSQL wire protocol)
- **ClickHouse** (HTTP interface)
- **Exasol**
- **Trino** (HTTP statement API)
- **Amazon Athena** (JSON API, requests signed with SigV4)
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

- **Engine** - one of the twelve above. The authentication picker below
  only offers the methods that engine supports.
- **Host / Port / Database / Username** - the port pre-fills the
  engine's default (5432 PostgreSQL, 3306 MySQL, 1433 SQL Server, 1521
  Oracle, 5439 Redshift, 8123 ClickHouse, 8563 Exasol, 8443 Trino, 443
  for Athena / Snowflake / Databricks / BigQuery). For Oracle, Trino and
  the warehouse engines the **Database** field does double duty:
  - **Oracle**: put the **service name** there (`FREEPDB1`,
      `ORCLPDB1`), not a database name. A listener that only accepts an
      old-style SID is not reachable.
  - **Trino**: the **default catalog** (`hive`, `iceberg`, `tpch`), used
      when nothing else names one. The sidebar browses every catalog
      regardless.
  - **Athena**: the **Glue database**. Athena also needs a workgroup and,
      unless the workgroup sets one, an S3 result location; both have
      their own fields in the form.
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
| Oracle                    | Password                                                                                           |
| Trino                     | Password (basic over TLS), OAuth (browser SSO), Personal access token                              |
| Amazon Athena             | AWS IAM (every request is signed)                                                                  |
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

**1. Vendor CLI (the default, and nothing to fill in).** Press **Sign in
with browser** and Octa runs the vendor's own sign-in for you: `az login`,
`gcloud auth login` or `aws sso login`. That command opens your browser,
you sign in there, and from then on Octa asks the CLI for a fresh token on
every connect. The CLI keeps a long-lived session on disk and refreshes it
silently, so you rarely authenticate again.

There is deliberately **no OAuth client ID to supply** on this path. The
vendor CLI is itself a registered OAuth application, which is why nothing
has to be registered by you. This is the same route DBeaver's *Default
credentials* option takes.

- Pros: no setup at all; the CLI refreshes for you; the recommended path
  on a workstation you control.
- Cons: the CLI must be installed. If it is not, Octa says so and shows
  the install command for your operating system, along with the
  alternative below.

**2. Browser sign-in with your own OAuth app (for locked-down machines).**
If you cannot install the vendor CLI, or your organisation blocks it, open
**Advanced** on the connection and paste the client ID of an OAuth app you
register once in your own cloud console. Octa then opens your system
browser itself and captures the credential directly, with no CLI involved.

- Pros: needs no CLI; works anywhere a browser does.
- Cons: a one-time app registration; and in this version the browser
  session lasts about an hour with no background refresh, so you sign in
  again when it expires.

Both can be set up at once. If a connection has a client ID, Octa uses its
own browser flow; otherwise it runs the vendor CLI's sign-in. Either way a
valid cached token is used first.

#### Setting up your own OAuth app

Only needed for path 2, and only once per provider:

- **Google**: in the Google Cloud console, create an OAuth client of type
  **Desktop app**. Put its client ID in **OAuth client ID** and its
  client secret in **OAuth client secret (Google)**, both under
  **Advanced** on the connection.
- **Azure**: in Microsoft Entra ID, register an application as a **public
  client** with the redirect URI `http://localhost` and public-client
  flows enabled. Put its client ID in **OAuth client ID** and your
  directory (tenant) ID in **Azure tenant**, both under **Advanced**.
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

#### What the button does

**Sign in with browser** is always available on Azure AD, GCP IAM and AWS
IAM connections. Next to it Octa shows whether you are signed in and
roughly how long the token has left, with a **Sign out** button that
forgets it. The connection list marks a signed-in connection **Signed in
via browser**.

If the vendor CLI is missing, the button says which one, gives the install
command for your operating system (winget on Windows, Homebrew on macOS, a
link on Linux), and points at **Advanced** as the way that needs no CLI at
all. The message is selectable so you can copy the command straight out of
it.

## Reaching a database through a jump host

Most managed databases inside a company only answer from inside the network,
and the way in is a bastion you can reach over SSH. Without this the answer was
"open a tunnel in a terminal first"; now a connection can carry its own.

Open **SSH tunnel** on the connection and tick **Reach this database through a
jump host**. Fill in the bastion's host, your account on it, and how you sign
in:

- **SSH agent** (the default): uses the keys your agent already holds, so there
  is nothing to fill in and no passphrase to type. Needs an agent running:
  `ssh-agent` on Linux and macOS, Pageant or the OpenSSH agent on Windows.
- **Private key file**: the path to your key, for example
  `~/.ssh/id_ed25519`. Give the private key, not the `.pub` one. If it is
  encrypted, its passphrase goes in the field below and is kept in your system
  keyring, in an entry separate from the database password.
- **Password**: your account password on the bastion, also kept in the keyring.
  Many hardened bastions refuse passwords outright and want a key.

Octa opens the SSH connection when you first use the database connection, binds
a port on `127.0.0.1` and forwards it to the real server. One tunnel is shared
by every tab, query and write on that connection, and it stays up until Octa
closes. **Test connection** exercises the tunnel too, so a bad bastion reports
as an SSH error rather than a confusing database one.

Nothing else changes: `--db-*` on the command line and the database MCP tools
read the same saved connection, so they tunnel as well without any extra flags.

!!! note "The connection still names the real database"
    Only the socket goes to `127.0.0.1`. The **Host** field keeps naming the
    database itself, so TLS certificates, Microsoft Entra token audiences and
    error messages are all unchanged by tunnelling. PostgreSQL, Redshift, MySQL
    and SQL Server verify their certificates against the real hostname exactly
    as they would without a tunnel.

    ClickHouse over HTTPS and Exasol are the exceptions: their clients cannot be
    told to dial one address and validate another, so through a tunnel they
    check the certificate against the tunnel endpoint. ClickHouse over plain
    HTTP, which is the usual internal case, is unaffected.

### Host keys

Octa checks the bastion's key against your `~/.ssh/known_hosts`, the same file
`ssh` uses.

- A **known and matching** key connects.
- An **unknown** host is refused, and the message says so. Connect to it once
  with `ssh` to record its key, or tick **Accept a new host key** to have Octa
  accept and remember it the first time, which is what `ssh`'s
  `StrictHostKeyChecking=accept-new` does.
- A **changed** key is always refused, whatever that tick box says. Either the
  server was rebuilt or something is impersonating it, and Octa will not guess
  which. Check, then remove the stale line from `known_hosts` yourself.

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

### Oracle

| Field                   | Value                |
|-------------------------|----------------------|
| Engine                  | Oracle               |
| Host                    | `oracle.example.com` |
| Port                    | `1521`               |
| Database (service name) | `FREEPDB1`           |
| Username                | `octa`               |
| Authentication          | Password             |

Octa speaks Oracle's TNS protocol directly, in pure Rust, so **no Oracle
Instant Client is needed** - nothing to install beside Octa itself.
Oracle 12.1 and later.

The **Database** field is the **service name** (`FREEPDB1`, `ORCLPDB1`),
not a database in the PostgreSQL sense. A listener that only answers to an
old-style SID cannot be reached.

#### Authentication and transport

Password only, and the connection is **plain TNS, not TLS**: Oracle
wallets, TCPS and Kerberos are not supported yet. In practice that means
**Autonomous Database on OCI is out of reach** (it always requires TLS,
usually with the wallet it hands you), and so is any on-prem listener
configured for TCPS only. A directly reachable on-prem or containerised
Oracle works, and one behind a bastion works through [a jump
host](#reaching-a-database-through-a-jump-host), which also encrypts the
hop.

#### Names are upper case

Oracle folds an unquoted name to **upper case** in its catalogue, so a
schema created as `sales` is listed as `SALES`, and that is the spelling
to type in the sidebar and in SQL. The reverse applies to what Octa
writes: a table it creates keeps the exact case of the source columns
(`id`, not `ID`), because the DDL quotes every identifier, so those
columns have to be addressed quoted afterwards - `SELECT "id" FROM ...`.

The schema list shows only schemas that are **not** Oracle-maintained, so
the three dozen that ship with the database stay out of the way even for
a DBA account. Each schema lists its tables **and its views**.

#### How Oracle types are read

| Oracle type                                       | Read as                 | Note                                                               |
|---------------------------------------------------|-------------------------|--------------------------------------------------------------------|
| `NUMBER(p, 0)`                                    | Whole number            | Beyond about 19 digits it degrades to a decimal                    |
| `NUMBER(p, s)`, `FLOAT`                           | Decimal                 |                                                                    |
| `NUMBER` with no precision                        | Whole number or decimal | A literal or computed column: the first 200 rows decide            |
| `DATE`                                            | Timestamp               | An Oracle `DATE` always carries a time of day                      |
| `TIMESTAMP`, `TIMESTAMP WITH TIME ZONE`           | Timestamp               | The zone offset is kept in the text                                |
| `VARCHAR2`, `CHAR`, `CLOB`                        | Text                    | See the LOB note below                                             |
| `RAW`, `BLOB`                                     | Binary                  | See the LOB note below                                             |
| `JSON` (21c+)                                     | Nested value            |                                                                    |
| `BINARY_FLOAT`, `BINARY_DOUBLE`                   | Not readable            | See the limitations below                                          |
| `VECTOR` (23ai), `REF CURSOR`, object collections | Diagnostic text         | There is no flat rendering for these; the cell shows what it holds |

**LOBs.** A `CLOB` or `BLOB` under 1 MB is fetched and shown in full, at
the cost of one round trip per cell. A larger one is not fetched: the cell
reads `[CLOB of 5242880 characters, too large to read]` so it is clear
that something is there and why you are not seeing it.

#### Writing to Oracle

Write-back and **Copy table** create Oracle types from the source
columns: text becomes `VARCHAR2(4000)`, whole numbers `NUMBER(19,0)`,
decimals `NUMBER`, booleans `NUMBER(1)` (Oracle had no `BOOLEAN` before
23c), dates and timestamps `DATE` and `TIMESTAMP`. Decimals deliberately
do **not** become `BINARY_DOUBLE`, the closer match, because Octa could
not then read back the column it had just written. Two further
consequences worth knowing:

- **Text is capped at 4000 bytes.** A longer value is refused by the
  server (`ORA-12899`) rather than silently truncated. A column that wide
  needs a `CLOB` created by hand, and Oracle will not take a string
  literal longer than 4000 bytes into one either.
- **Binary columns are written as hex text**, into a `VARCHAR2`, because
  that is the form the generated `INSERT` carries. Reading such a column
  back gives you the hex, not the original bytes.

A statement you run yourself in the SQL panel is **committed when it
succeeds**, as on every other engine Octa supports. Oracle's own clients
leave a transaction open until you say `COMMIT`, so if you are used to
typing `ROLLBACK` after a mistaken `UPDATE`, note that there is nothing
left to roll back.

#### Oracle limitations

The Oracle driver is young, and two of its gaps are visible from Octa.
Both are the driver's, not the server's:

- **A rejected statement ends the connection.** Oracle itself keeps the
  session alive after an error, but the driver loses it, and reports its
  own generic text instead of the `ORA-` code and message. Octa
  reconnects on the next action, so a typo in the SQL panel costs a round
  trip; what it cannot do is tell you what the server objected to. Check
  the statement in SQL*Plus or SQL Developer when the reason matters.
- **`BINARY_FLOAT` and `BINARY_DOUBLE` are not decoded.** Those cells say
  so rather than show wrong numbers. `SELECT CAST(the_column AS NUMBER)`
  reads them correctly, because Oracle's own `NUMBER` path works.

A running statement also **cannot be cancelled** on Oracle: see
[Cancelling a running query](#cancelling-a-running-query).

Everything else is exercised against a real Oracle 23ai server on every
run of the test suite that has one: browsing, reading, paging, foreign
keys, primary-key discovery and the full write-back round trip.

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
tables, and click a table to open its first rows in a tab.

A table opens **one page at a time**: Octa reads
[**Settings > Performance > Live database page size**](../reference/settings.md#performance)
rows (100,000 by default), and fetches the next page in the background
as you scroll towards the bottom, exactly as a large Parquet file does.
The status bar shows the count it has so far with a `+`, and says
"Loaded all N rows" once the table runs out. Lower the page size if
opening a table feels slow.

This is a separate setting from the initial-load row cap, which sizes a
local file read. The same number of rows over a database connection is
megabytes of JSON crossing the network, and Databricks refuses any
single result larger than 25 MiB. The CLI and the MCP server cannot
scroll, so they ignore the page size and use the initial-load cap.

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
two-level, and a PostgreSQL / Redshift / SQL Server / Oracle connection
browses the one database it is connected to (for Oracle, the schemas of
the service it connected to, minus the ones Oracle ships).

Octa keeps one live connection per saved connection and reuses it
across sidebar listings, table opens, and server queries (a dead
connection reconnects automatically), so browsing several servers side
by side stays snappy. Editing a connection in Settings drops its cached
connection.

A database tab is **read-only** unless its connection has **Allow
writes** on *and* Octa can discover a row key for the table (a primary
key, or a NOT NULL unique constraint) - see
[Editing and write-back](#editing-and-write-back). Read-only tabs show
the usual `[Read-only]` pill, which stays for as long as the tab is
open, and a status message explaining why, which fades after the time
set in **Settings > Appearance** like every other message.
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

The confirmation dialog is on by default. Turn it off under **Settings >
Databases > Confirm database write-back** if you write back constantly and
the prompt is in the way; Save then applies the diff immediately. Both paths
use a single transaction and both roll back on failure, so the setting
changes whether you are asked, not how safely the write happens.

Things to know:

- **No row key, no editing.** Saving builds an `UPDATE ... WHERE key =`
  per changed row, so Octa needs something that addresses exactly one
  server row. A **primary key** is used when there is one. Failing that,
  a **UNIQUE constraint whose columns are all NOT NULL** is taken
  instead - that is the same one-row guarantee, enforced by the server,
  and plenty of tables have one without ever declaring a primary key.
  The narrowest such constraint wins, so the choice is the same at save
  as it was at load. A *nullable* unique column is not enough:
  `WHERE col = NULL` matches nothing, so the save would quietly touch no
  rows.
- **No key at all? Rows are matched on all their values.** On Postgres,
  MySQL, SQL Server, Oracle, Redshift and Exasol, a table with neither a primary
  key nor a usable unique constraint is still editable: the save builds
  `WHERE col1 = old1 AND col2 = old2 AND ...` from the whole baseline
  row, with `IS NULL` where the original value was NULL. A message says
  this is what is happening when the tab opens, and fades like any other
  status message.

    What makes that safe is that **every such statement is checked to
    have touched exactly one row**, inside the transaction. Two rows
    matched (the table holds duplicates the values cannot tell apart) or
    none matched (someone changed the row on the server since you loaded
    it) both abort the whole save and roll back. It refuses rather than
    guesses, so it cannot quietly rewrite the wrong rows - but it does
    mean a table with genuinely identical rows cannot be edited this way.

    ClickHouse and the three catalog warehouses are excluded: their DML
    is not a plain single-row `UPDATE`, so they stay read-only without a
    key.
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

### Exporting the changes as SQL instead

Some teams cannot let a tool write to production directly: the change
has to be reviewed as a script first. **File > Save SQL...** writes
exactly the statements a save would run to a `.sql` file and sends
nothing to the server. The tab stays modified, so you can still save
normally afterwards.

The script is the same one Confirm would execute, produced by the same
code, so a reviewed script and an applied write-back cannot drift
apart. It is wrapped in a transaction and ordered the way the save
applies it: added columns, deletes, updates, inserts.

Save SQL has no keyboard shortcut by default. Assign one under
**Settings > Shortcuts** if you use it often.

### Generating the change as SQL instead of applying it

`--sync-sql` answers a different question from `--db-query`: what SQL would
make this server table match this file? It reads the table, compares it on the
key columns you name, and prints one transaction. Nothing is written.

```bash
octa --sync-sql users.csv --db prod --sync-table public.users --sync-on id > change.sql
```

The script goes to stdout and the counts to stderr, so it pipes straight into a
file or into `psql`. This is the headless twin of the GUI's **File > Save SQL**,
and both call the same renderer, so a script reviewed here and a write-back
applied there cannot drift apart.

Numbers are compared as numbers, so a file's `120.50` and a `numeric(12,2)`
column's `120.50` do not produce a phantom UPDATE. Columns present only in the
file are reported and skipped; this never emits `ALTER TABLE`. Agents can ask
the same question with the read-only `sync_sql` tool.

## Copying a table between servers

<!-- SCREENSHOT: db-copy-dialog.png: The "Copy table to another connection" dialog: source line "admin.people @ MariaDB-Test", target-connection dropdown showing "Post-Test (PostgreSQL)", target schema "public", target table "people", mode "Create new", Copy button with a green "Copied 3 row(s)." status. -->
![Copy table between servers dialog](../assets/screenshots/db-copy-dialog.png){ .screenshot-placeholder }

Right-click a table in the sidebar tree and pick **Copy to another
connection...** to copy it into a different server (for example MySQL
to Snowflake). Pick the target connection, schema, and table name, and
a mode: **Create new** (error if the table exists), **Append**, or
**Replace** (drop and recreate). Copy works **between any two of the
twelve engines**, in either direction; the dialog annotates which lane a
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
  Oracle, SQL Server on either side). Octa pulls the source in batches and
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
- The other engines (SQL Server, Oracle, Snowflake, Databricks,
  BigQuery, ClickHouse, Exasol) have no native DuckDB extension, so their tables
  are **imported** as plain workspace tables named after themselves
  (`orders`), which you can rename by double-clicking the name.
  Their menu entry therefore opens into the server's tree: pick a
  single table, a schema, or **Attach everything here** for the level
  you are on. The import is **row-capped** at the initial-load limit
  and a level with very many tables is refused - drill in one further,
  or query it with **Run on** instead.

You do not even need a table open: the SQL panel opens on an empty tab
too (Analyse > SQL), attach your connections and query the servers
directly - cross-server JOINs and UNIONs included. Without a table
there is simply no `data` in the workspace. Every result shows a row
counter directly above the grid.

The **Write result to DB...** dialog also lists your connections as
targets, writing the current result rows into a server table.

## Saving an open table as a new database table

You do not have to go through SQL. **File > Save to database...** takes
the table in the active tab - a CSV, a Parquet file, an Excel sheet,
anything Octa can open - and writes it into a database as a new table.
The dialog is the one the SQL panel uses, so the targets are the same:
one of your saved connections, or a DuckDB or SQLite file.

Pick a target, a schema and a table name, and a mode:

- **Create** - make a new table, and fail if that name is taken.
- **Replace** - drop any existing table of that name first.
- **Append** - add the rows to a table that already exists. The column
  names have to match.

The table name is pre-filled from the file name. Column names are
written **exactly as they appear in Octa**, capitals included, so a CSV
with an `FL_DATE` header gets a column called `FL_DATE` and not
`fl_date`. Pending cell edits are included; the file on disk is not
touched.

Two cases are refused rather than half-done. A tab with nothing open has
no table to write. And a tab in [large-file mode](large-files.md) is
showing one page of a much bigger file, so writing it would put a
partial table in your database and report success - use the SQL panel on
that tab instead, which reads the whole file.

Writing to a **connection** still needs **Allow writes** switched on for
it, exactly as below.

## Writes

Every connection is **read-only by default**. Server-side mutations
(INSERT / UPDATE / DELETE / DDL), the write-back target, the CLI
`--db-write-table`, and the MCP `write_db_table` tool are all refused
until you switch on **Allow writes** for that specific connection in
Settings. A "writes ON" badge in the sidebar marks opted-in
connections.

The switch is **per connection, and that is the only switch there is**. Mark
production read-only and leave staging writable; there is no global database
write toggle to turn off for convenience, and every surface routes through the
same check.

!!! note "Write protection in Settings does not cover databases"
    The **Write protection** setting under Settings -> Assistant governs
    *file* saves and the MCP server's default, not database connections. A
    connection with **Allow writes** on stays writable whether that setting is
    on or off, and a connection without it stays read-only either way.

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

The SQL panel's Cancel button stops a running statement on every engine
but Oracle:

| Engine                          | How it cancels                                          |
|---------------------------------|---------------------------------------------------------|
| PostgreSQL, Redshift            | Protocol-level cancel request                           |
| Snowflake, Databricks, BigQuery | The vendor's cancel API, so the warehouse stops billing |
| ClickHouse                      | `KILL QUERY` by query id                                |
| MySQL/MariaDB                   | `KILL QUERY` from a second connection                   |
| Exasol                          | `KILL STATEMENT IN SESSION` from a second connection    |
| SQL Server                      | `KILL` from a second connection                         |
| Oracle                          | Not supported: the statement runs to completion         |

Oracle is the exception: killing a session there needs `ALTER SYSTEM`,
a DBA privilege an ordinary connection has no business holding, so the
Cancel button is not offered.

The same Cancel is offered for a **sidebar table read**: while a table
is opening, or while it is fetching the next page as you scroll, the
status bar shows a spinner, what it is doing, and a Cancel button. It
uses the engine's own cancel from the table above, so the warehouse
stops working (and billing) too. The button appears once the statement
is actually running rather than with the spinner, and not at all on
Oracle, so it is never there without something behind it. A cancelled
read leaves the tab as it is, with the rows already in it; reopen the
table from the sidebar to try again.

SQL Server's `KILL` ends the whole session rather than the single
statement and needs the `ALTER ANY CONNECTION` permission, so Octa
reconnects afterwards. Copying a table between servers still runs to
completion.

### Query timeout

**Settings > Databases**, per connection: how many seconds Octa waits
on a query that is making no progress before giving up. Default 60.

The field appears only for **Trino, Athena, Snowflake, Databricks and
BigQuery**, and that is not an oversight. Those five submit a statement
over HTTP and then ask the server, over and over, whether it has
finished, so where to stop asking is Octa's decision to make. The wire
protocols block on a socket inside their driver instead, and hand that
decision to the driver and the server.

It belongs to the connection rather than to one global number because
the right answer differs per server: a warehouse that cold-starts needs
minutes where a Trino cluster answers in seconds. Athena used to allow
a fixed five minutes; if you query Athena over large scans, raise its
connection to match. The server may hold the very first request open
for up to 30 seconds on top of the timeout, which is time Octa spends
waiting rather than polling.

Timing out is never silent: the message names the number of seconds and
points at this setting.

The CLI can set it too, as `query_timeout=` in an
`--add-connection` spec.

## MCP / Assistant

Agents get six tools: `list_db_connections`, `list_db_tables`,
[`db_relationships`](../mcp/tools/db_relationships.md) (the foreign keys
the server declares, read from its catalog without touching a row),
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
