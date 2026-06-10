# Release notes

Editor and SQL-panel polish, richer file comparison in the CLI and MCP server,
a sequential row-number column for filtered tables, and nine more interface
languages.

## Editor

**Tab indents in the Markdown editor.** Pressing Tab in the Markdown Edit or
Split editor now inserts spaces (honouring your **Tab size** setting) instead
of jumping focus to the next control, matching the Raw text editor.

## SQL panel

**Keyboard-driven autocomplete.** While the suggestion popup is open, Up and
Down move the highlight and Enter or Tab accepts it (Esc dismisses). The keys
are only intercepted while the popup is showing, so ordinary typing keeps Enter
and the arrow keys behaving normally.

**Focus on open.** The SQL editor now takes keyboard focus the moment the panel
opens, so you can start typing straight away without clicking into it first.

## File comparison

**Ordered and join diffs.** `octa --diff` and the MCP `diff_tables` tool gain a
mode selector. The original whole-row `set` diff stays the default; `ordered`
lines rows up positionally and reports exactly which cells changed; `join`
matches rows on a key column (`--diff-on` on the CLI, `on` over MCP) and reports
added, removed, and changed rows. Changed rows carry the names of the differing
columns, so you see precisely what moved rather than just which whole rows are
unique to each side.

## Table view

**Sequential row numbers when filtered.** Filtering a table now adds a second
row-number column counting the visible rows from 1, alongside the original data
row numbers, so you can see both the source position and the filtered position
at a glance. It only appears while a filter is active and can be turned off
under **Settings -> Table View**.

## Languages

**Nine new interface languages.** The interface can now be shown in Ukrainian,
Bulgarian, Serbian, Croatian, Slovenian, Slovak, Lithuanian, Latvian, and
Estonian, bringing the total to 31 languages. Change it under
**Settings -> Appearance -> Language**; the switch is live.
