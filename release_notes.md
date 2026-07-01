# Release notes

This release focuses on cloud storage, comparing files, and staying responsive
on big files. Cloud connections can now span whole accounts or a single folder,
cope with several accounts or GCP projects, and sort a bucket's files by date or
size. The git-style diff view is now fully selectable, Dockerfiles open with
highlighting, and large files load without freezing the window.

## Cloud storage

**Browse a whole account, or just one folder.** A cloud connection now has a
**Scope**: target one bucket as before, confine it to a **path prefix** (a
folder inside a bucket, handy when you only have access to part of it), or go
**account level** to list every bucket or container in the account and pick one
to browse. Account-level browsing uses the provider CLI (`aws` / `az` /
`gcloud`) and needs broader list permissions.

**Several accounts or projects.** Buckets are scoped differently by each
provider, so an account-level connection covers one account or project at a
time. To see them all, make one connection per scope: an **AWS profile** per
account, an Azure **storage account** per account, or, for Google Cloud, a
**GCP project** per project. GCS connections gained a **GCP project** field
(buckets belong to a project) and an optional **gcloud account** for when you
have several logged-in identities.

**Sort files by name, date, or size.** A **Sort** menu next to the Connections
header orders the files in every folder by name (A-Z / Z-A), last-modified date
(newest / oldest), or size (largest / smallest). Folders always sort by name and
stay at the top.

## Comparing files

**Select and copy from a diff.** In the side-by-side **Text Diff** you can now
mark text with the mouse (drag, double-click a word, triple-click a line) and
copy it with **Ctrl+C** or right-click **Copy selection**. The same menu still
offers **Copy left side**, **Copy right side**, and **Copy as unified diff** for
the whole comparison, and the Row Hash, Ordered, and Join modes offer **Copy
table**. Long lines now scroll sideways within each pane instead of wrapping, so
the line numbers stay aligned.

## More file types

**Dockerfiles open as text.** Files named `Dockerfile`, `Containerfile`, and
their variants (for example `Dockerfile.dev`) have no extension, but Octa now
recognises them by name, opens them with syntax highlighting, and lists them in
the sidebar file browser.

## Performance

**Large files stay responsive.** Opening a single-table file over about 8 MB now
reads it on a background thread and shows a **"Loading file..."** spinner in the
status bar, so the window keeps responding while the read runs. Smaller files
still load instantly inline.

## Fixes

**Maximised dialogs restore correctly.** A dialog that you maximise now returns
to its previous size when you restore it, instead of getting stuck full-screen.

## Translations

The new cloud, sort, and diff labels are available in all 32 supported
languages.
