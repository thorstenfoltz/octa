# `octa --compare-folders`

Compare two or more folders file by file and print one row per file. Exit
code is `0` when every file is the same in every folder and `1` otherwise,
so it can gate a CI step the same way [`--validate-schema`](validate-schema.md)
does.

## Synopsis

```bash
octa --compare-folders FOLDER FOLDER [FOLDER ...] [--no-recursive] [-f FORMAT]
```

| Flag                    | Required | Meaning                                                 |
|-------------------------|----------|---------------------------------------------------------|
| `--compare-folders`     | yes      | Compare the FOLDER arguments that follow (two or more). |
| `--no-recursive`        | no       | Compare only the top level of each folder.              |
| `-f`, `--format FORMAT` | no       | Output format: `tsv` (default), `json`, or `csv`.       |

A FOLDER is a local path or a cloud URL: `s3://bucket/exports/`,
`az://container/prefix/`, `gs://bucket/prefix/`, `sftp://host/path/` or
`davs://host/path/`. Local and cloud folders mix freely.

## How files are matched

Files are matched by their **path inside each folder**: `2024/sales.csv`
under one folder is compared with `2024/sales.csv` under every other.
Subfolders are included unless you pass `--no-recursive`.

Contents are read only where they decide the answer:

- Two copies of **different sizes** are different. Nothing is read.
- Two or more copies of the **same size** are read in full and compared by
  a BLAKE3 hash of their bytes. Equal size alone never counts as the same.
- A file that exists in **one folder only** is never read.

## Output

One row per file path, sorted by path, on **stdout**:

| Column             | Meaning                                                |
|--------------------|--------------------------------------------------------|
| `path`             | The path inside the folders.                           |
| `status`           | `same`, `different` or `missing` (see below).          |
| `in`               | The folders that have the file.                        |
| `missing from`     | The folders that do not.                               |
| `size: FOLDER`     | Size in bytes, one column per folder.                  |
| `modified: FOLDER` | Last-modified time, one column per folder.             |
| `hash: FOLDER`     | BLAKE3 hash, filled only where the contents were read. |

| `status`    | Meaning                                                                 |
|-------------|-------------------------------------------------------------------------|
| `same`      | Every folder has the file and every copy is identical.                  |
| `different` | At least two copies differ. The file may also be missing from a folder. |
| `missing`   | Some folders lack the file, and the copies that exist are identical.    |

The status words are stable English, so a script can match on them.
Folder columns are named after the last part of each folder's path; when
two folders end the same way, more of the path is added until the names
differ.

Progress goes to **stderr**, so a pipe stays parseable: a `listing FOLDER`
line per folder and, on a terminal, a count while contents are read.

## Exit codes

| Code | When                                                                    |
|------|-------------------------------------------------------------------------|
| `0`  | Every file is the same in every folder.                                 |
| `1`  | A file differs or is missing, a folder hit the file limit, or an error. |

An incomplete comparison is not a pass: a folder that stopped at the file
limit exits `1` even when every listed file matched.

## The file limit

Each folder is listed up to the **Compare folders** file limit from
Settings (100,000 files per folder by default). When a folder holds more,
the listing stops there, stderr warns which folder was cut short, and the
exit code is `1`. Raise `folder_compare_max_files` in `settings.toml`, or
set `folder_compare_max_files_unlimited = true`, to list everything.

## Cloud folders and credentials

A cloud URL uses the saved cloud connection that covers it, the same one
the app's sidebar uses. Without one, the ambient credentials are used:
`AWS_*` variables or a cached SSO login for S3, `az login` for Azure
(plus `AZURE_STORAGE_ACCOUNT`), `gcloud` application-default credentials
for GCS. WebDAV always needs a saved connection. See
[Cloud storage: Credentials](cloud.md#credentials).

## Examples

### Two local folders

```bash
$ octa --compare-folders exports/before exports/after
listing before
listing after
path       status     in              missing from  size: before  ...
new.csv    missing    after           before                      ...
sales.csv  different  before, after                 1204          ...
stock.csv  same       before, after                 880           ...
$ echo $?
1
```

### A local folder against a bucket

```bash
octa --compare-folders ./exports/ s3://acme-data/exports/ -f csv > drift.csv
```

### Top level only, in a CI step

```yaml
- name: Check the published folder matches the build
  run: octa --compare-folders dist/ s3://acme-site/ --no-recursive
```

## See also

- [`octa --diff`](diff.md): two tables, row by row.
- [`octa --schema-drift`](schema-drift.md): which files in one folder
  disagree about their columns.
- In the app: [Compare folders](../usage/compare-folders.md)
- MCP: [`compare_folders`](../mcp/tools/compare_folders.md)
