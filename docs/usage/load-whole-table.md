# Load Whole Table

A tab does not always hold every row of its source:

- A **database table** opens one page at a time. How big a page is comes from
  **Settings > Performance > Live database page size**, and scrolling to the
  bottom fetches the next one.
- A **file** stops at **Initial-load row cap** in the same section.

Whatever works on the tab's rows, such as Summary, Value frequency, a chart or
the relationship map, then sees only those rows. **Data > Load whole table...**
fetches the rest.

## How it works

1. Octa counts first. For a database it asks the server with one
   `SELECT COUNT(*)`, which reads no rows. A file that knows its own size
   (Parquet, for example) says so straight away. A CSV cannot know without
   reading all of it, and the dialog says that instead of guessing.
2. The dialog shows how many rows the source has and how many the tab holds
   now. Nothing is downloaded until you click **Load all**.
3. The rest arrives in large batches. The status bar shows the progress and a
   **Cancel** button. Cancelling keeps every row that already arrived, and the
   tab can still scroll for more later.

While a whole table loads, Octa does not drop rows from the start of the tab
to save memory, as it otherwise does past three million rows. You asked for
all of them.

## Where to find it

- **Data > Load whole table...**
- Right-click a tab, then **Load whole table...**
- A keyboard shortcut you can set under **Settings > Shortcuts** (none by
  default).

The entry is greyed out when the tab already holds every row.

## Good to know

- A large table takes time and memory. The count in the dialog is there so
  you know what you are asking for.
- A database tab stays a database tab. Edits and **Save** still write back to
  the server exactly as before.
- Another database read already running (opening a table, the next page)
  has to finish first. The dialog says so.
