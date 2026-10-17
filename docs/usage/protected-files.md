# Password-protected files

A password-protected file is not a corrupt file, it is a locked one, and the
useful response is to ask for the passphrase rather than report damage.

Octa **opens and writes encrypted zips**: a `.zip` whose entries are
AES-encrypted.

It also **recognises** password-protected workbooks (an `.xlsx` saved with
*Encrypt with password*) but cannot open one yet. See
[Protected workbooks](#protected-workbooks) below for why.

## Saving with a password

**File → Save with password…** writes the open table into a new
password-protected zip. Pick the file name as for Save as, with the format you
want (`report.csv`); Octa then asks for the passphrase twice.

Only a zip file is written, and the dialog says so every time: for
`report.csv` the file on disk is `report.csv.zip`, with `report.csv` inside,
encrypted with AES-256. Octa opens it on any system (File → Open). Other tools
that open it: 7-Zip (Windows) or Keka (Mac). The zip tools built into Windows
and Mac cannot open AES-256 zips, so a double-click will not work.

Tick **Remember** to store the passphrase in this computer's keychain, so
Octa opens the zip without asking. Otherwise it is kept nowhere. If
`report.csv.zip` already exists, the dialog warns that it will be replaced.

The command line (`--password-env VAR` on `--convert` and `--batch-convert`,
see [`--convert`](../cli/convert.md)) and the **Protect with password**
checkbox in [Batch convert](batch-convert.md) write protected zips too, one
zip per output.

## Detection happens first

Whether a file is protected is readable **without** the passphrase: for a zip
it is a flag in the central directory, for a workbook it is the first eight
bytes. So Octa knows before it asks, and a file that needs nothing is never
interrupted by a prompt.

## The prompt

Opening a protected file raises a small window naming the file, with a
passphrase field, a **Remember this passphrase** checkbox, and Open and
Cancel. Enter opens it.

A rejected passphrase clears the field and says so, in text you can select
and copy, and asks again. The message never repeats the passphrase you
tried.

## Remembering

Ticking **Remember this passphrase** stores it in your operating system's
keyring (Keychain on macOS, the Secret Service on Linux, Credential Manager
on Windows), keyed by the file's full path. Two files with the same name in
different folders are different secrets.

It is **never** written to `settings.toml`, and a stored passphrase that no
longer works, because the file was re-protected, falls through to the prompt
rather than failing the open with a message about a secret you had forgotten
existed.

## Editing and saving

Edit the table as usual and press **Ctrl+S**: Octa writes your changes back
into the same encrypted zip, with the same passphrase. Any other entries in
the zip are copied across untouched, still encrypted, and never decrypted.

The plain copy Octa reads and writes lives in a private temporary folder that
only your user account can open. It is removed when you close the tab, when
Octa exits, and, after a crash, the next time Octa starts.

**Save As** to another file writes a plain, unprotected file, and the tab
moves to it: from then on Ctrl+S writes that file, not the zip.

If writing back into the zip fails, the status bar names the zip and the
reason, and the tab stays marked as unsaved, so closing it asks first, and
choosing Save there leaves the tab open while the zip still cannot be written.
Fix the cause (a full disk, the zip open in another program) and save again.

Earlier versions and backups are kept of the zip file itself, never of the
plain copy.

## Where a passphrase is never written

- Not in `settings.toml`, only in the OS keyring, and only if you asked.
- Not in the list of tabs Octa reopens at the next start: a protected tab is
  remembered as its zip and asks for the passphrase again.
- Not in any error message. The decryption paths are written to report *that*
  a passphrase was rejected, never *what* was tried.
- Not in the [debug report](../reference/diagnostics.md). The redactor masks
  anything following a `password` or `passphrase` label, as a second lock on
  the same door.

## Protected workbooks

Opening one tells you it is password-protected rather than reporting a
corrupt file, but Octa cannot decrypt it.

The reason is a dependency, not the feature. The one maintained pure-Rust
crate that implements ECMA-376 agile encryption, `office-crypto` 0.4, pins
`quick-xml ^0.38.4`, which carries two advisories: a quadratic parse
(RUSTSEC-2026-0194) and an unbounded allocation (RUSTSEC-2026-0195). Both are
reachable here, because the vulnerable code runs on XML read straight out of
the workbook being opened, and that is a file somebody else sent you. The fix
is quick-xml 0.41 or later and the pin cannot reach it.

Shipping a known denial of service on attacker-supplied input to read a
locked spreadsheet is a bad trade, so the dependency was dropped. This will
come back when that pin moves, or when another maintained implementation
appears.

To read one now: open it in Excel or LibreOffice and save an unprotected
copy.

## Limits

- Encrypted zips only. Save writes back into the zip you opened; a copy
  saved with Save As is plain, so the passphrase never silently travels with
  it.
- The first readable table inside the archive is opened. An encrypted
  archive holding several tables is not yet offered as a listing.
- Archives (`.zip`, `.tar`, `.tgz`, `.tar.gz`) and compressed files (`.gz`,
  `.zst`) inside an encrypted zip are not opened: opening one would write its
  contents outside the private folder. A zip holding nothing else says it
  found no readable table.
- The decrypted copy lives in a private folder in your system temp directory
  while the tab is open.

## See also

- [Supported formats](../getting-started/supported-formats.md)
- [Diagnostics](../reference/diagnostics.md) for what a debug report contains
