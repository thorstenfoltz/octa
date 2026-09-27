# Password-protected files

A password-protected file is not a corrupt file, it is a locked one, and the
useful response is to ask for the passphrase rather than report damage.

Octa **opens encrypted zips**: a `.zip` whose entries are AES-encrypted.

It also **recognises** password-protected workbooks (an `.xlsx` saved with
*Encrypt with password*) but cannot open one yet. See
[Protected workbooks](#protected-workbooks) below for why.

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

## Where a passphrase is never written

- Not in `settings.toml`, only in the OS keyring, and only if you asked.
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

- Encrypted zips only, and reading only. Octa does not write protected
  files, so the passphrase never silently travels with a copy you save.
- The first readable table inside the archive is opened. An encrypted
  archive holding several tables is not yet offered as a listing.
- The decrypted copy lives in your system temp directory for the session.

## See also

- [Supported formats](../getting-started/supported-formats.md)
- [Diagnostics](../reference/diagnostics.md) for what a debug report contains
