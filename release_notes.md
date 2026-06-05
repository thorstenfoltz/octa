# Release notes

This release builds on the **Chat Assistant** shipped in 0.11.0. The assistant
now also works on text, code, and Markdown files, its model list is hand-editable,
and the **Assistant** entry is reachable from any view. Alongside that, Octa now
reads non-UTF-8 text files, adds five more interface languages and a macOS Intel
build, and brings a batch of editor and usability fixes.

## Assistant

**Now helps with text, code, and Markdown.** Beyond tables, you can ask the
assistant to read, summarise, explain, refactor, or edit the plain-text, source
code, and Markdown files you have open. It writes changes either to a new file or
back to the open file on disk (reload with <kbd>Ctrl</kbd>+<kbd>R</kbd> to see
them in Octa).

**Hand-editable model list.** The provider model presets now live in a plain
`models.toml` next to your settings file, so you can add or remove model names by
hand without waiting for an update. A **Reload models.toml** button in Settings
picks up your edits without a restart.

**Always reachable.** The **Assistant** entry under the **Analyse** menu is now
present whatever view you are in, not just on table views.

## File support

**Reads non-UTF-8 text files.** Text, code, and Markdown files saved in
Windows-1252 / Latin-1 or UTF-16 (common on non-English Windows, and from
Excel's "Unicode text" export) now open correctly instead of failing or showing
garbled characters. Octa detects the encoding automatically and decodes to text.

## Languages

**Five more interface languages.** Added Indonesian, Vietnamese, Romanian,
Hungarian, and Czech, bringing the total to 17 translated languages. Pick yours
in Settings -> Appearance -> Language.

## Platforms

**A macOS Intel build.** Releases now include a separate `x86_64` macOS download
alongside the Apple Silicon (`aarch64`) one.

## Editor and usability

- **Line numbers in the Markdown editor.** The Markdown view's Edit and Split
  panes now show a line-number gutter, matching the Raw and SQL editors.
- **Grouped keyboard shortcuts.** Settings -> Shortcuts is now organised into
  labelled sections (File, Tabs, Search, Navigation, and so on) with left-aligned
  columns, so a binding is much easier to find.
- **Clearer load banners.** The date-format and whitespace-trim banners now have
  an explicit **Okay** button next to **Dismiss**, and hovering either explains
  exactly what it does (on the date banner, Okay keeps the dates while Dismiss
  reverts them to text).
- **Confirm before deleting a key.** Clearing a saved chat API key now asks for
  confirmation first, so a stray click can't wipe it.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> no longer selects all text when
  you open the assistant from inside the Markdown editor.
