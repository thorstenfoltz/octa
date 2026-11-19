Mostly fixes. Octa no longer crashes on a few kinds of damaged file, on a
very large step in Fill series or on a custom font that is not a font, and
it no longer offers a crash report for a crash that never happened.
Deleting several rows deletes them all, a CSV with line breaks inside quoted
cells opens with the right delimiter, and about 300 texts that stayed
English now follow your language. New are tab colours, a paste that grows
the table, **Fit columns** in SQL results, and cloud folder jobs that work
on several objects at once.

## Fixes

### Crashes and crash reports

- **A very large step in Fill series ended Octa.** The preview updates as
  you type, so typing a date step in the tens of millions crashed it, and a
  huge step in working days made it hang. Octa now says the series would go
  too far.
- **Date/Time calculation crashed on a very large amount** to add or
  subtract. It now leaves those cells empty instead.
- **A damaged MATLAB, QlikView, SAS transport or Arrow file could crash
  Octa** instead of showing an error. They show an error now.
- **A custom font that was not a font crashed Octa at every start**, so the
  setting could not even be undone in the app. Octa now falls back to the
  built-in font, and **Settings > Appearance** says the file cannot be read.
- **A second Octa window offered a crash report** for a crash that never
  happened. Only an Octa that really ended without closing triggers the
  offer now.
- **A PDF or other file Octa could not read made the next start offer a
  crash report**, although Octa had shown an error and carried on.

### Editing

- **Delete row deleted only one of the selected rows.** Edit > Delete row
  and the right-click menus deleted the row under the cursor and left the
  rest. They delete every selected row now, the same rows **Hide rows**
  would take, and one Ctrl+Z brings them all back.
- **Undoing a paste took one Ctrl+Z per cell.** A paste is one undo step
  now.
- **New table opened a second Untitled tab** beside the empty one Octa
  starts with. It now fills that empty tab. Its row and column counts are
  typed, and a count out of range turns red and says why.

### Opening files

- **A CSV with line breaks inside quoted cells could open with the wrong
  delimiter.** A semicolon file with decimal commas then showed two garbled
  columns. Octa now reads whole records when it guesses the delimiter.
- **A DuckDB table or schema with a dot in its name**, such as
  `"sales.2026"`, could not be opened. It opens now.
- **CSV and other text files with much plain text open faster**, because
  each cell is checked for a date only when it could hold one.

### Saving

- **Excel files keep unusual text as written.** Text such as `_x0041_` in a
  sheet name, column header or formula, and a tab or line break in a column
  header, could come out changed in Excel. An updated library writes them
  correctly.

### Display

- **From about 150% zoom, the column title covered its type line.** The
  header now grows with the zoom.
- **SQL results ignored the thousand separators and the number style** set
  under **Settings > Table View**. They follow them now, display only:
  Copy, Export and Write result to DB carry the plain values.
- **A cloud folder with tens of thousands of objects slowed the whole app**,
  because the sidebar drew every row on every frame. It draws only the rows
  in view now.
- **Two browsers on the same side of the window got half of it each, for
  good.** The line between the cloud, database and folder browsers can be
  dragged now.

### Languages

- **About 300 texts stayed English in every language**: labels, status
  messages and errors in windows and dialogs, chart kinds, and the window
  title. They follow your language now. The API connection form also showed
  the bare key `api.headers` instead of a label, and **Harmonise schemas**
  showed `harmonise.plan_hint` instead of its hint. Both are fixed.

### Documentation

- **The page on supported formats said to edit the structure of JSON, YAML
  and other documents in a text editor.** Octa can do that itself: the Raw
  view saves your text exactly as typed, and the JSON and YAML tree keeps
  the nesting. The page and **Help > Documentation** say so now.

## What's new

### Tab colours

Right-click a tab, choose **Colour** and pick one of the twelve colours tab
groups use. The tab fills with it, faintly in the background and fully
while active, and **No colour** takes it off. A tab in a group shows both,
its own colour as the fill and the group's as the line under it. The colour
stays when the tab is refreshed and comes back with **Reopen last
session**. The new shortcut **Next tab colour** is unbound by default.

### Paste grows the table

A pasted block with more rows or columns than the table now makes the table
grow to fit it, the way a spreadsheet does: new rows at the end, new
columns named `col4`, `col5` and so on. Before, everything past the edge
was dropped without a word. A database tab and a table whose rows are
still loading stop at the edge as before.

### SQL results

- A click on a **column header** now marks only the column's name, and
  Ctrl+C copies it, ready to paste into the query. **Double-click** a header
  to select the whole column, as a single click did before.
- Right-click a header for **Copy column name(s)**.
- **Fit columns** in the panel's toolbar makes every result column as wide
  as its name and its longest value need. The shortcut **Auto-fit SQL
  result columns** does the same and is unbound by default.

### Cloud folder jobs in parallel

A folder copy, move or delete and a folder comparison now work on several
objects at once, as a cloud union already did. Before, they went one
object after another. Each cloud connection can set its own number, for a
server that allows only a few sessions, as many SFTP servers do. The
command line takes it as `parallel=N` in `octa --add-connection`.

### New and changed settings

New:

- **Parallel connections** in the form of a cloud connection, empty by
  default, which uses the general setting below.

Changed:

- **Settings > Performance > Simultaneous downloads** is now called
  **Parallel cloud connections** and also covers folder copy, move, delete
  and comparison. Its value and default (8) stay.
- **Reopen last session** also brings back tab colours.
