# Cloud Storage

Browse and open files directly from Amazon S3 (and S3-compatible providers
such as IONOS, MinIO, and Cloudflare R2), Azure Blob Storage, and Google Cloud
Storage. Saving back to the cloud is **off by default** and must be turned on.

<!-- SCREENSHOT: cloud-sidebar.png: The cloud connections sidebar (File >
Cloud connections) docked on the left. Two or three saved connections, each
labelled with its provider in brackets (e.g. "prod (S3)", "media (Azure)",
"open-data (GCS)"). One connection expanded showing a couple of folder rows
and a file row with size + date, e.g. "sales.parquet  (12.4 MB, 2026-06-20)".
Under one connection a small status line "Saved keys  reachable" ("reachable"
in green); a "Sign in" button on a sign-in connection and a "Sign out" button
on the saved-keys one. -->
![Cloud connections sidebar](../assets/screenshots/cloud-sidebar.png){ .screenshot-placeholder }

## Add a connection

<!-- SCREENSHOT: cloud-settings.png: The Settings > Cloud storage section. At
the top the "Allow writing to cloud storage" checkbox. Below it the list of
saved connections with aligned Edit / Remove buttons. Below that the "Add
connection" form filled in for an S3 connection: Name, Provider = "S3 /
S3-compatible", Bucket, Region, the Path-style / Allow HTTP / Public-anonymous
checkboxes, and the Secret section with Access key ID + Secret fields and a
"Save secret" button. -->
![Settings cloud storage section](../assets/screenshots/cloud-settings.png){ .screenshot-placeholder }

Open **Settings → Cloud storage** and click **Add connection**.

You can also start from the sidebar: the **Cloud connections** header has a
**+ Add** button that opens Settings at the Cloud storage section with a
blank form. It sits in the header rather than the connection list, so it is
there even when you have no connections yet and the list is empty.

The form fields:

| Field                  | Meaning                                                                                                                                                          |
|------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Name**               | Label shown in the sidebar.                                                                                                                                      |
| **Provider**           | S3, Azure Blob, or GCS.                                                                                                                                          |
| **Bucket / Container** | The S3 bucket, Azure container, or GCS bucket.                                                                                                                   |
| **S3 endpoint**        | Leave empty for real AWS. Set it for an S3-compatible provider; those usually also need **Path-style addressing** on, and a local MinIO may need **Allow HTTP**. |
| **Region**             | S3 region (real AWS).                                                                                                                                            |
| **AWS profile**        | A named profile for SSO sign-in (resolved through the AWS CLI). Leave empty for ambient credentials.                                                             |
| **Storage account**    | Azure only.                                                                                                                                                      |

### Connection scope

When you add a connection, the **Scope** field controls what it targets:

- **Whole bucket** (default) - the connection targets one specific bucket or
  container. The sidebar roots at that bucket.
- **Path prefix** - confine the connection to a folder inside the bucket (for
  example `team-a/`). Useful when you only have read access to part of a
  bucket; the browser roots at that folder and cannot navigate above it.
- **Account level** - the connection lists every bucket or container in the
  account; you pick one to browse. This needs broader permissions (S3
  `ListAllMyBuckets`, Azure container list, GCS bucket list) **and** the
  provider CLI installed (`aws` / `az` / `gcloud`). If the CLI cannot
  enumerate buckets, add a bucket-scoped connection instead.

Because each provider scopes bucket listing differently, an account-level
connection sees only one account/project at a time. To cover several, make one
connection per scope:

- **AWS / S3** - buckets belong to the credential's account. Set the **Profile**
  field to a named AWS profile (for example an SSO profile); one connection per
  account.
- **Azure** - containers belong to a storage account. Set the **Account** field
  to the storage account name; one connection per account.
- **Google Cloud** - buckets belong to a **project**. For an account-level GCS
  connection, set the **GCP project** field to the project id (leave empty for
  your active `gcloud` project), and optionally the **gcloud account** email if
  you have several logged-in accounts. Make one connection per project.

### Credentials

Octa resolves credentials in this order:

1. A **secret you save** on the connection.
2. The **ambient** environment: `AWS_*` variables, a cached SSO session, an
   Azure CLI login, or Google application-default credentials.

- **S3 / S3-compatible**: save an **Access key ID** + **Secret** for static
  keys, or use a profile / `aws sso login` for AWS SSO.
- **Azure**: save an account key or a **SAS token**, or sign in with the Azure
  CLI.
- **GCS**: uses application-default credentials
  (`gcloud auth application-default login`) or `GOOGLE_*` environment
  variables. There is no static-key field.

Saved secrets are stored in your operating system keyring when available,
otherwise in `settings.toml`. **Clear secret** removes a stored secret.

### Public / anonymous buckets

For a **public, read-only** bucket or container, tick **Public / anonymous
access** in the connection form. Octa then skips request signing entirely, so
it opens with no credentials and no sign-in. Without it, a public Azure
container would redirect to a login and fail. The sidebar shows the connection
as `(public)`.

## Sign in (browser SSO)

A **Sign in** button is only needed for **browser SSO** sign-in, and only
appears for connections that use it. It shells out to the cloud's official CLI:

| Provider | Command                                   |
|----------|-------------------------------------------|
| S3       | `aws sso login` (plus `--profile` if set) |
| Azure    | `az login`                                |
| GCS      | `gcloud auth application-default login`   |

You do **not** need any CLI for static keys, a SAS token, ambient environment
credentials, a GCS service-account key, or a public connection - only for the
in-app browser sign-in. When the CLI is missing, the connection shows a
**"Sign in needs CLI"** note instead of the button (hover it for the full
reason). Octa never implements the OAuth flow itself.

!!! note "Windows: no WSL required"
    All three CLIs ship native Windows installers (the AWS CLI MSI, the Azure
    CLI MSI, the Google Cloud SDK installer). If your CLI only lives inside
    WSL, native-Windows Octa will not see it - install it on Windows, or use
    static keys / a SAS token instead. As an alternative install Octa and the
    CLI in WSL2 and start Octa from there.

## Browse and open

Open the sidebar with **File > Cloud connections**. Click a connection to list
its bucket root, expand folders to drill in, and click a file to open it.

- Listings load on a background thread and are cached, so re-expanding a folder
  is instant.
- Clicking a file downloads it to a temporary copy and opens it in a new tab,
  exactly like a local file, so every supported format works.
- **Refresh** re-lists a connection (for example after signing in, or after the
  bucket changed underneath you).
- **Sort** (next to the Connections header) orders the files in every folder by
  name, last-modified date (newest / oldest), or size (largest / smallest).
  Folders always sort by name and stay at the top.

## Union several objects

**Ctrl-click** objects to select them instead of opening them. A **_N_
selected** bar appears at the top of the cloud section with a **Union...**
button.

Octa downloads the selected objects in the background and opens the
[Union](union-tables.md) dialog over them, with the same column
reconciliation as any other union. A folder of partitioned `part-*.parquet`
files becomes one table without opening a tab per object, and the objects
need not share a format.

A plain click still just opens the object, and clears the selection.

## Saving back

By default, cloud-opened files are **read-only**: pressing **Save** shows a
reminder and does nothing. **Save As** to a local path always works and
detaches the tab from the cloud.

To save back to the object, turn on **Allow writing to cloud storage** in
**Settings > Cloud storage**. Then **Save** writes the tab back to its original
object. Uploads run in the background; the status bar reports success or
failure.

!!! note "Why writing is off by default"
    The write toggle mirrors Octa's other write-protection switches. Cloud
    objects are often shared and versioned, so an accidental overwrite is worth
    a deliberate opt-in.

### Writing from the assistant and MCP

The same **Allow writing to cloud storage** switch lets the in-app
[assistant](chatbot.md) write to the cloud: ask it to save a result to a cloud
URL (`s3://bucket/out.parquet`, `gs://...`, `az://...`) and tools like
`write_table`, `convert`, and `run_sql` (with `write_to`) upload it, to buckets
you have saved as a connection.

The headless [MCP server](../mcp/index.md) (`octa --mcp`) also writes to cloud
URLs, using ambient credentials (the same chain its reads use). There is no
in-app switch for the MCP server; run it with `--mcp-read-only` to drop every
write tool entirely.

## Connection status and signing out

Each connection's name carries its provider in brackets - `(S3)`, `(Azure)`,
or `(GCS)` - so you can tell them apart at a glance. Under the name the sidebar
shows how it authenticates - **Public**, **Saved keys**, or **Sign-in** - and,
once you have expanded it at least once, whether the bucket was **reachable**
(green) or **not reachable** (red). The status reflects the last listing, not a
live connection.

A connection that uses **saved keys** has a **Sign out** button that removes
its stored credentials from this computer (the same as **Clear secret** in
Settings), after a confirm. That is local only; a browser SSO session lives in
the cloud CLI, so you end that there (for example `aws sso logout`).

## Is it always connected?

No. Object storage is not a persistent session - every list, open, and save is
an independent request. A saved connection is just **configuration** (the
bucket plus how to authenticate), like a bookmark; it stays in the list across
restarts but nothing is "connected" in between, and nothing drains while idle.
