# Release notes

This release adds a way to build a column from if / else-if / else rules, a
live preview for pivot and unpivot, a CSV repair that keeps stray fields instead
of dropping them, default keyboard shortcuts for the newer tools, and a no-admin
PowerShell installer for Windows.

## Analysis and formatting

**Conditional column (if / else-if / else).** **Edit -> Conditional column...**
builds a new column whose value depends on conditions, like a spreadsheet
`IF` / `IFS` or a SQL `CASE`. Add an ordered list of rules, for example "if
amount > 100 then high, else if amount > 50 then medium, else low". Each rule
tests one column with an operator (equals, contains, greater than, is empty, and
so on) and writes its output value when it matches. Rules are checked top to
bottom and the first match wins; reorder them with the up / down buttons, and a
final **Else** value covers the rows no rule matches. Outputs that look like
numbers become numeric cells; everything else is text. The result is a new
column (name and position configurable) and is undoable with Ctrl+Z. It shares
its operators with Conditional formatting, the difference being that conditional
formatting *colours* matching cells while a conditional column *sets a value*.

**Conditional formatting: explicit rule order.** Rules have always been checked
top to bottom with the first match winning. That order is now adjustable with
**^** / **v** buttons on each rule, and a one-line hint makes the
if / else-if / else behaviour clear.

**Pivot / Unpivot live preview.** The Pivot / Unpivot dialog now explains itself.
It shows a plain-language sentence of what the current settings do (for example
"Spreads the distinct values of `month` into new columns, using sum of `sales`,
grouped by `region`.") and a small preview table of the first result rows, so you
can see the shape of the result before committing. To stay fast on large tables
the preview runs against a sample of the first 1,000 source rows and shows at
most 10 result rows; press **Run** to reshape the full table into a new tab.

## Opening files

**Malformed-CSV repair keeps your data.** When the repair prompt
(**Settings -> File-Specific -> Offer repair on malformed files**) detects rows
with *more* fields than the header, it now offers **Keep extra values (add
columns)**. With it on, the table is widened so every extra field keeps its own
column (named `column_4`, `column_5`, ...) instead of being silently dropped;
rows that are too short pad with empty cells. This is on by default for ragged
files, because dropping values is rarely the fix you want. The file on disk is
never changed.

## Keyboard shortcuts

**Default shortcuts for the newer tools.** Several features that previously had
no keyboard shortcut now ship with one, all rebindable under
**Settings -> Shortcuts** (which refuses to let two actions share a key):

| Action | Default |
|--------|---------|
| Pivot / Unpivot... | Ctrl+Shift+P |
| Transform column... | Ctrl+Shift+R |
| Conditional formatting... | Ctrl+Shift+L |
| Conditional column... | Ctrl+Shift+J |
| Data validation... | Ctrl+Shift+G |
| Sort by columns... | Ctrl+Shift+O |
| Summary tab | Ctrl+Shift+M |
| Number format... | Ctrl+Shift+N |
| Copy as Markdown table | Ctrl+Shift+B |

## Windows

**No-admin PowerShell installer.** A new `install.ps1` installs Octa for the
current user without administrator rights. It downloads the latest release,
verifies its SHA256 checksum, installs into `%LOCALAPPDATA%\Programs\Octa`, and
unblocks the binary so the unsigned executable usually launches without the
SmartScreen prompt. Run it from PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

Pass `-Version v1.2.3` for a specific release or `-InstallDir <path>` to install
elsewhere. The existing `install.bat` (system-wide, requires administrator) is
unchanged.

## Under the hood

**Clearer window-size setting.** The **Initial window size** setting only sizes
the window when it is *not* maximised; a maximised window always fills the
screen, so on a maximised window every size looks the same. The setting's hint
and the documentation now say this plainly.

**Internal tidy-up.** The toolbar and the Settings dialog were split into smaller
source modules for maintainability. There is no change in behaviour.
