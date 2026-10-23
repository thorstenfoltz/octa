# Markdown view

For `.md` (and `.markdown`, `.mdown`, `.mkd`) files Octa renders a
proper CommonMark preview with bold / italic / strong / headings /
lists / code blocks / blockquotes, plus a live-editing layout so
you can author Markdown right in Octa.

![Markdown view in Split layout](../../assets/screenshots/markdown-view-split.png){ .screenshot-placeholder }

## Layout: Preview / Split / Edit

A segmented toggle at the top of the Markdown view picks the
layout. Each option carries a small icon next to the label:

- 👁 **Preview** (default) shows rendered output only. Read-mode.
  Links in the preview open in your system browser when clicked.
- 🔀 **Split** shows the TextEdit on the left, live preview
  on the right. Edits update the preview every keystroke.
- 📝 **Edit** shows the TextEdit only, full window width. Useful for
  distraction-free writing.

The default is **Preview** so Markdown files open as a rendered
document straight away. Switch to Split or Edit whenever you need
to write or edit the content.

## Width

The preview uses the whole width of its pane and follows the window when you
make it wider, so a table spreads out instead of wrapping its cells. Tables
wrap their text only when the pane is too narrow for them.

## Text size

The preview follows the app's body text size, so both the
[**Font size**](../../reference/settings.md) setting and **Ctrl+Plus** /
**Ctrl+Minus** zoom scale the rendered document along with everything else.
Headings, code and list markers scale with it.

## Saving edits

Edits to the buffer are tracked just like [Raw view](raw-text.md)
content: the tab shows `*` when modified, and **Ctrl+S**
([`SaveFile`](../../reference/shortcuts.md#file-operations)) saves
back to the original `.md` file. The preview is for display only;
the disk content matches the editor pane.

Pressing **Tab** in the editor inserts spaces (it indents in place
rather than jumping to the next control), using the
[**Tab size**](../../reference/settings.md#search-editor) setting, the
same as the Raw text editor. Only the tab you type is converted: tabs the
file already contained stay as they are.

The editor can have several carets, so one keystroke types on many lines at
once: see [Typing on several lines at once](raw-text.md#typing-on-several-lines-at-once).

**Ctrl+H** replaces text in the document: see
[Find & Replace](../search-and-filter.md#find-replace).

## Printing

**File -> Export to PDF...** prints the document the way the preview shows it,
with headings, lists, tables, quotes and code, wrapped to A4 or Letter: see
[Export to PDF](../pdf-export.md#markdown-documents).

## Limitations

- **No images yet.** `![]()` references aren't rendered as actual
  images in the preview (they fall through silently).
- **No footnotes / definition lists.** Plain CommonMark only.
- **No syntect on code blocks.** Code blocks render in monospace
  with a background but without language-specific colours.

For a richer Markdown preview with all these features, open the
file in your text editor of choice and use a dedicated previewer.
Octa's Markdown view is intentionally light.

## See also

- [Notebook view](notebook.md) for `.ipynb`, which also uses the
  same Markdown renderer for text cells.
- [EPUB reader](epub-reader.md) reuses `render_pulldown` for
  chapter rendering after converting XHTML to Markdown.
