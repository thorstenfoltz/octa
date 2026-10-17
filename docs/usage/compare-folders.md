# Compare Folders

Compare two or more folders file by file: which files are the same in
every folder, which differ, and which are missing from some. The folders
can be on this computer, in a cloud bucket, or a mix of both, so a local
export can be checked against what was uploaded.

## Opening it

- **Analyse → Compare folders...** opens an empty dialog.
- Right-click a folder in the **folder sidebar** and choose
  **Add to folder comparison...**.
- Right-click a folder (or a connection's top row) in the **cloud
  sidebar** and choose **Add to folder comparison...**.

Each "Add" puts that folder into the same dialog and opens it, so
folders from both sidebars can be collected before comparing. Inside the
dialog, **Choose folder...** adds a folder from this computer and
**Remove** takes one out again.

There is no default keyboard shortcut; one can be assigned under
**Settings → Shortcuts** (**Compare folders...**).

## Options

| Option                 | Default | What it does                                                                   |
|------------------------|---------|--------------------------------------------------------------------------------|
| **Include subfolders** | on      | Compares the files in every folder below too. Off compares only the top level. |

**Compare** stays disabled until the dialog holds at least two folders.
While it runs, the footer shows which folder is being listed and how many
files have been read; **Cancel** stops it, and so does closing the
dialog.

## How files are matched

Files are matched by their **path inside each folder**: `2024/sales.csv`
under one folder is compared with `2024/sales.csv` under every other.

Contents are read only where they decide the answer:

- Files of **different sizes** are different. Nothing is read.
- Files of the **same size** are read in full and compared by their
  contents. Equal size alone never counts as the same.
- A file that exists in **one folder only** is never read.

## Reading the result

The comparison opens as a **Folder comparison** tab with one row per
file, sorted by path, and the status bar sums it up.

| Column             | Meaning                                                  |
|--------------------|----------------------------------------------------------|
| `path`             | The path inside the folders.                             |
| `status`           | `same`, `different` or `missing` (see below).            |
| `in`               | The folders that have the file.                          |
| `missing from`     | The folders that do not.                                 |
| `size: FOLDER`     | Size in bytes, one column per folder.                    |
| `modified: FOLDER` | Last-modified time, one column per folder.               |
| `hash: FOLDER`     | Contents hash, filled only where the contents were read. |

| `status`    | Meaning                                                                 |
|-------------|-------------------------------------------------------------------------|
| `same`      | Every folder has the file and every copy is identical.                  |
| `different` | At least two copies differ. The file may also be missing from a folder. |
| `missing`   | Some folders lack the file, and the copies that exist are identical.    |

Folder columns are named after the last part of each folder's path; when
two folders end the same way, more of the path is added until the names
differ. The tab is an ordinary table, so search, filter on `status`, sort
and save work as on any other.

## Compare these files

Right-click a row and choose **Compare these files...** to open both
copies of that file in the row-by-row [Compare view](view-modes/compare.md).
With three or more folders holding the file, the entry opens a list of
folder pairs to pick from. The entry is greyed out for a file that exists
in one folder only, and Octa says so when the file is not a table or text
file it can read. Cloud copies are downloaded first.

## The file limit

Each folder is listed up to **Settings → Performance → Compare folders:
files per folder** (100,000 by default). When a folder holds more, the
listing stops there and the tab carries a banner naming the folder that
was cut short, so an incomplete result never passes for a complete one.
Tick **Unlimited** to list every file.

## Cloud folders

Cloud folders use the saved connection they were added from, with its
sign-in. Listing a bucket is cheap; reading is not: every file whose size
matches a copy in another folder is **downloaded** to be compared, which
can take a while on a big folder and may cost transfer fees with your
storage provider. Files of different sizes and files in one folder only
are never downloaded.

## Command line and MCP

The same engine runs on the command line and over MCP:

```bash
octa --compare-folders exports/ s3://acme-data/exports/
```

The CLI form exits `1` when any file differs or is missing, so it can gate
a CI step. See [`octa --compare-folders`](../cli/compare-folders.md) and
the [`compare_folders`](../mcp/tools/compare_folders.md) MCP tool, which
the in-app Assistant can also call.

## See also

- [Compare view](view-modes/compare.md): two tables, row by row.
- [Schema drift](schema-drift.md): which files in one folder disagree
  about their columns.
- [Cloud storage](cloud-storage.md): adding a cloud connection.
