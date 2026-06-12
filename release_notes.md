# Release notes

New analysis and formatting tools (conditional formatting, pivot / unpivot,
key-matched comparison, a rebuilt Summary tab), saveable SQL snippets and chat
prompts, a richer search bar, a read-only mode for the MCP server, and a long
list of smaller conveniences and fixes.

## Analysis and formatting

**Conditional formatting.** **Edit -> Conditional formatting...** colours cells
automatically from rules you define (equals, contains, greater-than, is empty,
and so on, with numeric comparison when both sides are numbers). Rules apply
live as you edit them, and explicit colour marks still win over a rule.

**Pivot / Unpivot.** **Analyse -> Pivot / Unpivot...** reshapes a table between
long and wide form the way a spreadsheet pivot table does, powered by DuckDB's
`PIVOT` / `UNPIVOT`. The result opens in its own detached tab.

**Key-matched comparison.** The Compare view gains two new modes alongside the
existing text and row-hash diffs. **Ordered** lines rows up positionally and
reports exactly which cells changed; **Join** matches rows on a key column and
reports added, removed, and changed rows. Changed rows carry the names of the
differing columns. The same `ordered` / `join` modes are available from
`octa --diff` and the MCP `diff_tables` tool.

**Rebuilt Summary tab.** **Analyse -> Summary...** now shows localised column
titles in your chosen language, with a hover description on each. You can choose
which statistics appear under **Settings -> Summary** (min, max, mean, median,
standard deviation, quartiles, not-null and null counts, null percentage,
unique count, distinct ratio, total rows); Column and Type are always shown. The
**Unique** count is now exact (`COUNT(DISTINCT)`), so it never exceeds the row
count. The older **Column Inspector** has been removed, as the Summary tab now
covers everything it did and more.

**Copy as Markdown table.** **Edit -> Copy as Markdown table** (also on the cell
and row context menus) copies the current selection as a GitHub-flavoured
Markdown table, ready to paste into a README or an issue.

## Search

**Case-sensitive and whole-word toggles.** The search bar gains an **Aa**
(match case) toggle and a whole-word toggle, both applied to the filter and the
in-place highlight.

**Search scope.** A scope dropdown limits the search to a single column instead
of every column.

**Persistent search history.** A **Recent** dropdown beside the search box
recalls your recent queries across sessions. The number kept is configurable
under **Settings -> Search & Editor** (set it to 0 to disable).

## SQL panel

**Saved snippets.** The **Snippets** button opens a manager window for a named,
persistent query library. Save the current query under a name and description,
then insert or delete saved snippets. The window has the usual minimise /
maximise / close controls and is resizable. Snippets are shared across tabs and
survive restarts.

**Query history.** A **History** dropdown recalls the recent queries run in the
current tab (session-only).

**Change highlight after a mutation.** After an `INSERT` / `UPDATE` / `DELETE`,
the cells and rows the query changed are briefly highlighted in green so the
effect is visible. The duration is configurable under **Settings -> SQL**, and
the feature can be turned off.

## Chat assistant

**Saved prompts.** The **Prompts** button opens a manager window for a small
library of reusable prompts: save whatever is in the input box under a name,
then insert or delete saved prompts later. Like SQL snippets, prompts persist
across sessions.

**Tool-call audit log (opt-in).** Turn on **Settings -> Chat / Assistant ->
Tool-call audit log** to record every tool the assistant runs as one JSON line
per call (tool name, argument and result sizes, duration, error flag,
timestamp). Cell contents are never written. Octa warns once at startup when
these logs grow past a configurable size.

**Smoother input.** The `@`-mention autocomplete now responds to the Up / Down
arrows and to clicking a suggestion, the panel opens a little wider so the
header buttons no longer overlap, and the History window gained the standard
minimise / maximise / close controls.

## Everyday conveniences

**Open the right view per file type.** A `.json` file now opens in the JSON
Tree, and a `.yml` / `.yaml` file opens in Raw text. You can still switch from
the View menu; this just picks a sensible starting point.

**Tidier folder sidebar.** The directory tree now lists only sub-folders and
files Octa can open, so a folder full of unrelated files stays readable. Turn it
off under **Settings -> Directory Tree** to list every file.

## CLI and MCP

**Read-only MCP mode.** `octa --mcp --mcp-read-only` starts the MCP server with
the data-writing tools (`write_table`, `edit_table`, `convert`) removed, for
agent frameworks that should only ever read.

## Fixes

**Menus no longer wrap in other languages.** Longer menu-item labels in
non-English locales (German, Russian, and others) no longer break onto a second
line; the menu widens to fit instead.
