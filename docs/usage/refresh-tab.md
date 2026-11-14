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
new one. To be asked again, open **Settings > Opening files** and set **On refresh**
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

## Follow a growing file

A program that appends to a file, such as a log writer or a data logger, can be
watched live. Open the file, then choose **View > Follow file**.

- Works for a CSV, TSV, JSON Lines or log file read from a plain file on this
  computer. Where it does not apply, the entry is greyed out and its tooltip
  says why: not a local file, another format, large-file mode, or a tab that
  holds only part of the file (load the whole table first).
- Octa checks the file every second and adds only the new rows. A half-written
  last line waits until its line break arrives, so only complete lines appear.
- New rows are highlighted with the same colour and duration as the SQL change
  highlight (**Highlight SQL changes** and **Highlight duration (seconds)** in
  **Settings > SQL**).
- The view jumps to the new rows only when it was already at the bottom. If you
  scrolled up to read something, it stays where it is.
- If the file gets shorter, or is replaced by another file, the tab reloads
  from the start. A tab with unsaved edits does not reload, because that would
  lose them. It stops following and says so instead.
- Following stops at the row limit from Settings and shows a message.
- A **Following** pill appears in the status bar. Click it, or choose
  **Follow file** again, to stop.

Known limitation: when a followed file is replaced and the tab reloads, rows
written while the file is being read again can be missed. Refresh the tab once
the writer has finished to be sure of the full content.

## Auto-refresh

Choose **View > Auto-refresh...** on a tab read from a database or an API
connection to read it again at a fixed interval. The entry is greyed out for
any other tab.

In the dialog, type the number of seconds into **Every (seconds)** and click
**Start**. **Stop** ends it, **Cancel** closes the dialog.

- The interval counts from the end of one read to the start of the next, so
  reads never overlap, even when the server is slow.
- Intervals below the minimum are refused. The minimum is **Minimum
  auto-refresh interval (seconds)** in **Settings > Databases** (5 by default).
  BigQuery, Athena and Snowflake bill every read, so they use **Minimum on
  billed engines (seconds)** (60 by default). For those engines the dialog also
  warns once per session that every read is billed.
- Changed rows are highlighted, using the same setting as above. Octa matches
  rows by the connection's key columns when it has them, and by position
  otherwise.
- While the tab has unsaved edits, auto-refresh pauses, and a result that
  arrives after you started editing is dropped with a message. Your edits are
  never overwritten.
- If a read fails, Octa simply waits for the next one.
- The status bar always says where the tab stands. A filled dot and
  **Auto-refresh N s** while it runs, or a paused message while edits are
  pending: click to stop. A hollow dot and **Auto-refresh off** otherwise: click
  to set an interval. On a tab with no database or API behind it the entry is
  greyed, and hovering it says why.

Neither Follow file nor Auto-refresh is saved with the session. After a
restart, switch them on again.
