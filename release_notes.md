This is a repair release. It fixes a group of defects that could lose or
corrupt your edits without saying anything, several that crashed or wedged the
app, and a handful where a setting did not do what the dialog promised.
Keyboard shortcuts can be rebound properly again, and the release notes you are
reading no longer need a network connection.

## Release notes without a request

The notes for the version you are running now ship inside Octa itself, so the
window opens after an upgrade whether or not you are online, and on a Microsoft
Store copy. It used to appear only as a side effect of the start-up update
check, which meant that turning that check off also silenced the notes, though
the two have nothing to do with each other. **Settings > Updates** now holds two
independent switches:

- **Check for updates at start** asks GitHub once per launch whether a newer
  version exists. It no longer has any say over the notes.
- **Show what a new release brings** is the notes window itself. Turn it off and
  you never see it, for any version.

While that second switch is on, the window opens at every start until you tick
**Do not show these notes again** and close it. The tick covers the version you
are running and nothing else, so the next release opens the window again, and
unticking it brings the window back. Previously the tick box and the setting
were one and the same, so there was no way to say "I have read these" without
also saying "never show me any".

## Edits and rows stay where you put them

**Sorting a database table no longer scrambles which row is which.** Sorting a
table opened from a SQLite or DuckDB file moved the visible rows but left the
row identity behind, so the next save wrote each row's values onto a different
row. Colour marks had the same problem, on every reorder rather than only on
sorting. Rows, their identity and their marks now move together.

**A failed save leaves the tab unsaved.** If writing failed, on a full disk or
a read-only folder, Octa reported the error but cleared the unsaved marker
anyway. Closing the tab then asked nothing and the edits were gone. The tab now
stays modified, and auto-save keeps retrying.

**Column filters follow their column.** Filters and hidden columns were
remembered by column position, so inserting, deleting or moving a column
silently pointed them at a different column, and a filtered **Save As** wrote a
different set of rows than the chips on screen described. They now follow the
column they were set on.

**Undo puts a moved row back completely.** Undoing a row or column move
restored the order but not the edited cells or the database row identity, so a
value could reappear against the wrong row and be written there.

**Saving a database file twice no longer duplicates rows.** A row added during
the session was inserted again on every later save of the same file, which
stayed invisible until the file was reopened.

**Settings no longer reverts what you changed elsewhere.** The dialog takes a
copy of your settings when it opens, and the rest of Octa keeps running behind
it. Applying wrote that copy back wholesale, undoing anything changed in the
meantime: a pinned tab, the model picked in the assistant panel, and, worst of
all, a cloud key you had just cleared from the sidebar, which came back after
Octa had said it was gone.

## Crashes and hangs

- **A `NaN` in a numeric column no longer takes the app down.** R and pandas
  both write `NaN` for a missing number, and finding outliers or filling with
  a median walked straight into it. Non-numbers now count as missing, the way
  `na.rm` does.
- **The SQL history menu survives non-English queries.** A query containing a
  character such as `ü` crashed Octa when the History list tried to shorten it.
- **A background row load that fails now stops.** Bad bytes deep in a large
  CSV, or a drive removed mid-scroll, left the spinner turning and the app
  redrawing every frame for the rest of the session, burning battery and never
  loading another row.
- **Cancelling an assistant turn no longer breaks the chat.** Cancelling while
  a tool was running left the conversation in a state every provider rejects,
  so every later message failed with the same error and only **New chat**
  recovered.
- **A panel whose scan fails can be used again.** A failed clean-up, report,
  drift, harmonise, batch-convert or fuzzy-match run left its panel stuck on
  "Scanning..." for the rest of the session.
- **Quitting during Ollama start-up no longer orphans the server.** Closing
  Octa in the few seconds while a local model server was starting left it, and
  its multi-gigabyte model, running with nothing able to stop them.
- **Ask gives up instead of hanging.** A model endpoint that accepted the
  connection and then said nothing blocked both Ask boxes for the rest of the
  session.

## Settings behave the way the dialog says

- **Reset to defaults keeps your content.** It used to wipe saved database and
  cloud connections, their stored keys, chat profiles and pinned tabs, none of
  which the confirmation mentioned and the keys of which no undo could restore.
  It now resets the settings and leaves that content alone.
- **Reset to defaults resets everything it shows.** Ten values, among them the
  raw-view and decompression caps, the chart limits and the auto-save interval,
  quietly survived a reset and came back on Apply.
- **Clearing a saved key really clears it.** Keys live in the operating
  system's keyring, and `settings.toml` holds one only on a machine with no
  keyring to use. Where that fallback was in play, **Clear API key** deleted
  the keyring entry but dropped the plaintext copy from the dialog's unsaved
  working copy alone, so closing Settings with the window's `x` left the key on
  disk after the message said it was gone. Cloud and database connection
  secrets behaved the same way.
- **A pinned file on a disconnected drive is no longer forgotten.** Pins were
  pruned at start-up whenever the file could not be found, so a network share
  that was not mounted yet emptied the list permanently.
- **An empty `HOME` no longer scatters settings.** An exported but empty
  `HOME`, `XDG_CONFIG_HOME` or `APPDATA` made Octa write `settings.toml`, with
  any plaintext secrets in it, into whatever folder it was started from.
- **Fresh installs and upgrades agree.** Two settings, red negative numbers and
  the length of the recent-files list, had different values depending on
  whether the settings file already existed.

## Changing a keyboard shortcut works

Recording a binding was close to unusable: the key you pressed also ran
whatever it was already bound to, so recording **Ctrl+S** saved the file, and
no combination could be recorded at all, because the press of the modifier
itself was captured as the key and stored as something that could never fire.
While Octa waits for your key, that key now belongs to the recording and
nothing else, and modifiers are recognised as modifiers.

If the combination you press is already taken, Octa says so under the row you
are editing, rather than in a message at the top of the section that was
usually scrolled out of sight, and offers **Take it over**: the key moves to
the action you are recording and the previous owner is left unbound. Two
actions still cannot share a combination.

**Ctrl+C**, **Ctrl+X** and **Ctrl+V** can now be recorded too, and any key you
bind is named properly in the list instead of showing as `?`.

## Smaller things

- **Files are written through a temporary file and then renamed.** Writing a
  Parquet, CSV or compressed file emptied the target before the first byte was
  written, so a failure part-way left a truncated file where your data had
  been. The original now survives until the replacement is complete, and keeps
  its permissions.
- **The container image names its base image explicitly** instead of leaving it
  untagged, and runs as a numeric user id, which lets Kubernetes verify for
  itself that Octa is not running as root.
- **Documentation** for all of the above, in the in-app help and on the
  documentation site.
