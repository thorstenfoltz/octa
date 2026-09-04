# Cloud storage from the CLI

Browse, transfer and delete cloud objects without opening the GUI. The same
code runs behind the sidebar's right-click menu and the MCP tools, so all three
behave identically.

## Credentials

A URL is matched against your saved connections (`Settings > Cloud storage`)
first, so its endpoint, region and credentials apply. If none covers the URL,
Octa falls back to the ambient chain, exactly as `octa --mcp` does:

- **S3**: `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, or a cached SSO session
- **Azure**: `az login`, plus `AZURE_STORAGE_ACCOUNT` (an `az://` URL cannot
  carry the account name)
- **GCS**: Google application-default credentials (`gcloud auth
  application-default login`)

`octa --list-connections` prints what is saved (names and targets only, never
secrets), for both cloud and database connections. See
[Managing connections](#managing-connections) to add and remove them without
the Settings dialog.

## Browsing

```bash
octa --cloud-ls s3://bucket/prefix/        # one folder level
octa --cloud-ls s3://bucket/ --recursive   # flatten everything (cap 100,000)
octa --cloud-ls gs://bucket/ -f json       # any --format works
```

The listing is a normal Octa table: `type`, `name`, `key`, `size`, `modified`,
so `-f json` or `-f csv` pipes straight into other tools.

## Download and upload

```bash
octa --cloud-get s3://bucket/data.parquet --out ./data.parquet
octa --cloud-put ./data.parquet --to s3://bucket/data.parquet
```

Read actions also accept a cloud URL directly, so you rarely need
`--cloud-get`:

```bash
octa --schema s3://bucket/data.parquet
octa --sql s3://bucket/sales.parquet -q 'SELECT count(*) FROM data'
```

## Copy, move, delete

```bash
octa --cloud-copy s3://a/data.csv --to s3://a/backup/data.csv
octa --cloud-copy s3://a/data/  --to gs://b/backup/      # folder, across clouds
octa --cloud-move s3://a/old.csv --to s3://a/archive/old.csv
octa --cloud-delete s3://a/old.csv
octa --cloud-delete s3://a/scratch/ --recursive
```

A source ending in `/` is a folder and the operation is recursive; the folder's
shape is recreated under the destination.

**Within one bucket** the provider copies server-side: no bytes pass through
Octa, so a 100 GB object costs one API call. **Across buckets, accounts or
providers** the object is streamed in 8 MiB blocks into a multipart upload, so
memory stays flat whatever the size. The command reports which path ran:

```
copied 42 object(s), 10485760 bytes (streamed)
```

### Things that are refused

| Situation                                          | Why                                                                |
|----------------------------------------------------|--------------------------------------------------------------------|
| More than 10,000 objects in one run                | A folder move is not resumable; better to refuse than half-finish. |
| `--cloud-delete` on a folder without `--recursive` | A stray trailing slash should not become a recursive delete.       |
| Copying a folder into itself or a descendant       | The result is never what was meant.                                |

!!! warning "Delete cannot be undone"
    Unless the bucket has versioning enabled, a deleted object is gone. There is
    no trash. `--cloud-move` to an archive prefix is the reversible option.

## Exit codes

`0` on success, `1` on any failure (bad URL, missing credentials, refusal). The
human-readable summary goes to **stderr**, so `-f json` output on stdout stays
pipeable.

## Managing connections

The Settings dialog is not the only way to save a connection. `--add-connection`
takes one `key=value,key=value` spec, so provisioning a container or a CI job is
a single line:

```bash
export S3_KEY='AKIAEXAMPLE:wJalrXUtnFEMI/K7MDENG'
octa --add-connection 'kind=s3,name=prod,bucket=my-bucket,region=eu-central-1,allow_writes=true' \
     --secret-env S3_KEY

octa --add-connection 'kind=postgres,name=warehouse,host=db.internal,database=analytics,user=reader' \
     --secret-env PGPASSWORD

octa --remove-connection warehouse
```

**The secret is never a command-line argument.** `--secret-env` names an
environment variable and the value is read from there, so it stays out of `ps`
output, shell history and any process listing.

| Field         | Value                                                                                                                                                                                                                      |
|---------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `kind=`       | Cloud: `s3`, `azure`, `gcs`. Database: `postgres`, `mysql`, `mssql`, `oracle`, `redshift`, `clickhouse`, `exasol`, `trino`, `athena`, `snowflake`, `databricks`, `bigquery`.                                               |
| `name=`       | What you will refer to it as. Required.                                                                                                                                                                                    |
| Cloud keys    | `bucket`, `region`, `endpoint`, `prefix`, `account`, `profile`, `account_level`, `anonymous`, `allow_writes`, `force_path_style`, `allow_http`                                                                             |
| Database keys | `host`, `port`, `database`, `user`, `allow_writes`. On Oracle, `database=` is the service name; on Trino the default catalog; on Athena the Glue database; on Databricks the SQL warehouse id; on BigQuery the project id. |

What the secret variable should hold:

| Connection | Value                                                                             |
|------------|-----------------------------------------------------------------------------------|
| Database   | The password.                                                                     |
| S3         | `ACCESS_KEY_ID:SECRET_ACCESS_KEY`, optionally `:SESSION_TOKEN`.                   |
| Azure      | The storage account key, or a SAS token (recognised by its `sig=`).               |
| GCS        | Nothing: GCS uses application-default credentials. Add it without `--secret-env`. |

Any of them also accepts the JSON form Octa stores internally, for cases the
shorthand does not cover.

### Behaviour worth knowing

- **Adding a name that already exists replaces it**, keeping the id and
  therefore the stored secret. Re-running the same script is idempotent. Keys
  you leave out revert to their defaults, so give the whole spec each time.
- **Unknown keys are an error.** `buckett=` fails with the list of real keys
  rather than quietly creating a connection that points nowhere.
- **`allow_writes` defaults to false**, here as everywhere. A connection added
  from the CLI is read-only until you say otherwise.
- **`--remove-connection` also deletes the stored secret**, so nothing is left
  orphaned in the keyring.
- Only password authentication can be expressed in a spec. AWS IAM, Azure AD,
  key-pair JWT and browser sign-in need fields and interactive steps that do
  not fit in one line; configure those in the Settings dialog.

## Running without a desktop

Octa's CLI and MCP server run fine in a container, but two things a desktop
provides are missing there, and both have an explicit lever.

### Where settings live: `OCTA_CONFIG_DIR`

Everything Octa persists is one file, `settings.toml`. It is normally found via
`XDG_CONFIG_HOME` / `HOME` (Linux), `APPDATA` (Windows) or
`~/Library/Application Support` (macOS). A distroless container usually has
**none** of those set, in which case Octa has nowhere to read or write and says
so instead of pretending:

```
error: no config directory: set OCTA_CONFIG_DIR to a writable path
```

`OCTA_CONFIG_DIR` overrides the lot, on every platform, and is used verbatim (no
`octa` subdirectory appended):

```bash
docker run --rm \
  -e OCTA_CONFIG_DIR=/config -v "$PWD/octa-config:/config" \
  -v "$PWD:/data" octa --list-connections
```

The official image sets `OCTA_CONFIG_DIR=/config` already, so
`-v ./octa-config:/config` is enough.

### No OS keyring: `OCTA_NO_KEYRING`

Secrets prefer the OS keyring and fall back to `settings.toml` (chmod 0600)
when there is none. A container has no D-Bus and therefore no Secret Service,
so the fallback is what always happens; setting `OCTA_NO_KEYRING=1` skips the
lookup rather than waiting for it to fail. The official image sets it.

When a secret lands in the plaintext fallback, every command that stores one
says so:

```
secret stored in settings.toml as plain text (no OS keyring available)
```

If that plaintext matters to you, mount `settings.toml` from a real secret
store (a Kubernetes secret, a bind mount from a host vault) rather than baking
it into an image layer.
