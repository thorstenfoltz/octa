A release of fixes, for Find & Replace, Markdown and PDF, notebooks, Settings
and database connections. Along the way Find & Replace became a box of its
own, laid out as in VS Code, and the SQL editor got one too.

## Fixes

### Find & Replace

- **Replace all replaced only the first match in a cell.** `a, a, a` became
  `b, a, a`. Every match is replaced now, and a single Ctrl+Z takes the whole
  run back instead of one cell at a time.
- **A `$` in the replacement vanished.** Outside Regex mode, `$5` became
  nothing, because it was read as capture group five. The replacement now goes
  in as typed, and only Regex mode reads `$1` and `$2`.
- **Replace changed cells you could not see.** It ignored the column filters
  and the column the search was limited to, and reached into hidden columns.
  It now changes only the matches on screen.
- **Replace did nothing visible in the Raw, Markdown and tree views.** It
  changed the table behind the view instead of the text on screen. It now
  replaces in the text, and a replacement that would leave a JSON or YAML
  tree unreadable is refused.
- **Replacing the next match skipped the one you were on.** It began after
  the selected cell. It now replaces the selected match and then selects the
  one after it.
- **Ctrl+H in a text editor deleted a character.** Opening Find & Replace
  from the Raw, Markdown or SQL editor also removed the character before the
  caret.
- **A match tinted the whole cell.** Only the matched text is marked now, in
  the toolbar's Highlight mode too.

### Markdown and PDF

- **A Markdown file printed as its source.** **File > Export to PDF...** put
  the raw `#`, `**` and `|` of a `.md` file on paper, one table row per line.
  It now prints the document the preview shows: headings, lists, quotes, code
  blocks and tables, wrapped to the page, with the table header repeated on
  every page a table runs onto. In the Table view the file still prints as
  the table you see there.
- **The Markdown preview stopped growing at 900 points.** Making the window
  wider left the document where it was and wrapped table cells instead of
  widening the table. The preview now uses the whole width of its pane.
- **PDF exports printed in whatever font the system had for Arial**, on some
  Linux machines a narrow one. Every PDF export now carries Octa's own fonts
  and looks the same on every machine.
- **One emoji failed a whole PDF export**, of a table, a chart or the
  relationship map. The export now goes through and leaves the emoji out,
  unless a plain font has it.

### Notebooks and HTML

- **A Jupyter notebook showed a greyed-out Table view and no JSON tree.**
  The View menu now leaves Table out for a notebook, which never had one, and
  offers the JSON tree, since a `.ipynb` file is JSON. It still opens in the
  Notebook view.
- **A notebook lost edits made in its text.** Metadata or outputs changed in
  the Raw view were dropped as soon as a cell was then edited in the Notebook
  view, because the cells were written back into the file as it was on disk.
  They are now written into the notebook as the tab holds it.
- **The tables of an HTML or PDF file opened as "Untitled".** A page's
  tables showed as "Untitled - Products" instead of "products.html -
  Products", and Save and Refresh had no file to go back to. Every table now
  carries its file.

### Settings

- **Apply dropped a connection you had not saved yet.** Editing a cloud,
  database or API connection, or a chat profile, and clicking **Apply**
  without its own Save button first lost the edit without a word. Apply now
  saves it; when it cannot be saved yet (no name, say), Settings stays open
  with the reason in the form.
- **Long texts in Settings ran off the window.** The SFTP agent note and
  other long descriptions stayed on one line however the window was sized,
  and nothing in Settings could be marked or copied. They wrap at the
  window's edge now, and every text there can be marked and copied.
- **A long connection name pushed Edit and Remove out of view.** In the
  database, cloud, API and chat profile lists the name ran on in one line.
  It wraps now and the buttons stay beside it.
- **Save connection sat above fields it saves.** A cloud connection's secret
  came after the button, and so did a database connection's SSL mode, query
  timeout and engine options. The button is the form's last row now. The
  form's heading says **Add connection**, or **Edit connection** while it
  holds one from the list, instead of "Add or edit a connection". The
  secret's field sits beside its label too, instead of two rows under a
  **Secret** heading.

### Databases

- **A statement Octa refused read like a database error.** A typo such as
  `SELCT` on a read-only connection looked as if the server had rejected it.
  Every database error now starts with one sentence naming who stopped it:
  **Databricks returned an error.** (or PostgreSQL, SQL Server and so on)
  when the server refused it, followed by its own words; **Octa did not send
  this to** a connection when Octa refused it first, quoting the word it read;
  and **Octa could not connect to** or **could not finish talking to** a
  connection for the network, a timeout or a cancel. This holds for every
  engine, in the SQL editor, the sidebar, the command line and the MCP server.
- **A read-only Databricks connection refused WITH queries that only read.**
  Any `WITH` query that so much as mentioned `INSERT`, `UPDATE`, `DELETE` or
  `MERGE`, in text, a comment or a column name, was refused, so filtering a
  table's history on `operation = 'MERGE'` needed **Allow writes**. Octa now
  reads what the `WITH` leads into: a query runs, an `INSERT` or `MERGE` is
  still refused.
- **Read-only SQL Server and Redshift connections refused queries over a
  word in text.** SQL Server refused `COMMIT`, `ROLLBACK`, `EXEC` and
  `EXECUTE` even in `WHERE step = 'EXECUTE'` or a comment, Redshift refused
  `INTO` even in `'moved into archive'`. Both now only refuse those words
  where they would run.

### Command line

- **`man octa` left out four actions and an option**: `--api`,
  `--check-references`, `--compare-distributions`, `--hash-columns` and
  `--harmonise-schema --combine`, 18 options in all. It now covers every
  option, and is sorted by task (look at a file, analyse, compare and check,
  clean, combine, convert, databases, cloud), with each action's options
  listed right under it and a section on which actions exit 1 on a finding.

## What's new

### Find & Replace, as in VS Code

**Ctrl+H** opens a box over the top right of the table or text, with the find
field on top and the replace field under it. It has its own find text: the
toolbar's search box only searches now, so a filter set there keeps its rows
while you replace in them. Matches are highlighted and counted, **Enter** and
**Shift+Enter** step through them while you stay in the box, **Aa**, **W** and
**.\*** switch to matching capitalisation, whole words and regular
expressions, and **Replace** or **All** swap them. **Escape** closes the box.

### Find in the SQL editor

The SQL editor had no find at all. **Ctrl+F** opens the same box over the
editor, and **Ctrl+H** opens it with the replace row. Text marked on one line
becomes the search, and every editor has its own box.
