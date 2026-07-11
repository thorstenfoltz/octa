# Release notes

This release is about reaching your data with less friction: open a file whose
extension lies about its contents, stack a folder of files into one table
without opening a tab for each, and set up as many named chat models as you
like and switch between them with one click. It also fixes a batch of rough
edges in the interface.

## Opening files

**Open a file with the reader you choose.** A `.log` file that actually holds
JSON used to open as plain text, and there was no way to tell Octa otherwise
short of renaming the file. Two new menu entries fix that:

- **File > Open as...** for a file you have not opened yet. Pick the format
  first, then pick one or more files. The file dialog deliberately shows
  *every* file, not just the ones Octa recognises, because those are exactly
  the files worth opening this way. Each opens in its own tab.
- **View > Reopen as** for the file already in front of you, which is re-read
  in place.

Both offer JSON, JSON Lines, CSV, TSV, YAML, TOML, XML, Markdown and plain
text. Pick JSON for that `.log` and it parses as JSON, tree view and all,
exactly as if it had been named `.json`. Log files holding one JSON object per
line want **JSON Lines**. Nothing on disk is renamed or rewritten: this only
changes how Octa reads the file, and if the content does not parse as the
format you chose, the tab is left exactly as it was.

## Combining files

**Union several files straight from the sidebar.** You no longer need a tab per
file. In the folder sidebar, **Ctrl-click** the files you want (**Shift-click**
takes a whole run), then click **Union...** in the bar that appears at the top,
or right-click a selected file. Octa reads them and shows the same
reconciliation plan as before, with one checkbox per file.

This is the quick way to stack a folder of partitioned exports: forty
`part-*.parquet` files become one table without forty tabs. The files do not
even have to share a format, since the columns are reconciled either way. A
plain click still just opens a file.

**The same works for the cloud.** Ctrl-click objects in the cloud sidebar and
click **Union...**; Octa downloads them in the background and opens the same
dialog, so a folder of partitioned parts in S3, Azure Blob or GCS becomes one
table without a tab per object.

## Chat assistant

**Named model profiles.** The assistant used to hold one configuration per
provider. It now holds as many named profiles as you like, **including several
for the same provider**: an Anthropic "Opus, deep" beside an Anthropic "Sonnet,
quick" beside a local Ollama one. The panel picks between them with a single
dropdown, so changing model is one click rather than a trip through settings.

Each profile carries its own:

- **Name** and an optional **description**, both shown in the dropdown.
- **Provider** and **model**.
- **Temperature**.
- **Thinking / reasoning**, a free-text field passed to the provider. OpenAI
  takes an effort level such as `high`; Anthropic and Gemini take a budget in
  tokens such as `8000`. Leave it empty for none. For Anthropic, Octa adjusts
  the rest of the request for you, since the API demands it.
- Optionally its **own API key**. By default every profile of a provider shares
  that provider's key, which is what you want almost always. Tick **Use its own
  API key** to give one profile a key of its own, for a separate account or a
  spend-limited key.

Your existing setup is carried over automatically: on first run your current
provider, model and temperature become a single profile, so nothing changes
until you add more.

## Cloud storage

**Add a connection from the sidebar.** The cloud header has a **+ Add** button
that opens Settings at the Cloud storage section with a blank form. It sits in
the header rather than in the list of connections, so it is there precisely
when you have none yet.

## Interface

**The tab strip scrolls sideways.** When more tabs are open than fit the
window, the mouse wheel over the tab strip scrolls it horizontally, with no
modifier key needed, and a scrollbar appears beneath the tabs. Tabs are never
clipped out of reach again.

**Selecting text past the edge of the screen.** In the text, Markdown and SQL
editors, dragging a selection to the bottom (or top, or either side) of the
view now keeps scrolling, speeding up the further out you drag. A selection is
no longer limited to the lines that happen to be visible, so there is no need
to select, act, scroll, and start again.

**Menu entries say what they do.** An entry now ends in `...` when, and only
when, clicking it opens a new tab or window. Entries that simply do the thing
carry no ellipsis. So **Reopen as**, **Multi-search** and **Export debug
report** lost theirs, while **Chart...**, **Transpose...**, **New File...** and
**About...** gained one.

**Tidier controls.** The chart control bar's dropdowns, text fields and
checkboxes now share one height, so the labels between them line up instead of
drifting. The cloud sidebar's header controls are all the same size as each
other.

## Fixes

**A stray "Untitled" tab no longer appears** when you open a file from the
cloud (or an entry out of an archive). Octa was leaving behind the empty tab it
starts with instead of reusing it, and because the tab strip is hidden until a
second tab exists, that empty tab suddenly became visible next to the file you
actually opened.

## Translations

The new menu items, dialogs and settings are available in all 32 supported
languages, with English text as the fallback for the newest strings.
