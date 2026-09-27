# Refresh a Tab

A tab shows what its source held when Octa read it. **Refresh** reads that
source again: the file on disk, the database table, the cloud object or the
API endpoint the tab came from.

## How to refresh

- Press <kbd>Ctrl</kbd>+<kbd>R</kbd> to refresh the active tab.
- Right-click any tab and choose **Refresh**. The tab does not have to be the
  active one.

By default Octa asks where the fresh data should go:

- **Refresh this tab** replaces the tab's contents with the fresh read. The
  tab keeps its place in the strip, its pin and its name.
- **Open in new tab** leaves the tab as it is and opens the fresh read beside
  it, so you can compare the two.
- **Cancel** does nothing.

## Stop being asked

Tick **Don't ask again** before you click a button, and Octa remembers that
button: from then on Refresh goes straight to the same tab, or straight to a
new one. To be asked again, open **Settings > Files** and set **On refresh**
back to **Ask each time**. The same setting also lets you pick either answer
directly.

A tab with unsaved changes always asks, whatever the setting says, because
refreshing it in place throws those changes away. The button then reads
**Refresh and discard changes**.

## What each kind of tab reads

| Tab came from               | Refresh reads                                                                                                             |
|-----------------------------|---------------------------------------------------------------------------------------------------------------------------|
| A file                      | The file on disk. A tab holding one sheet of a workbook, or one table of a database file, reads just that sheet or table. |
| A database table (sidebar)  | The table's first page from the server, the same as opening it from the sidebar.                                          |
| A cloud object (sidebar)    | The object from the bucket, downloaded again, not the copy Octa downloaded before.                                        |
| An API endpoint             | The endpoint, fetched again.                                                                                              |
| Anything worked out in Octa | Nothing. SQL results, summaries and other result tabs have no source, so **Refresh** is greyed out.                       |

The read happens in the background like any other open, and the tab only
changes once the fresh data has arrived. If the read fails, the tab keeps
what it had.
