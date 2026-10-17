# Binary Cells

Some cells hold raw bytes instead of text or numbers: a picture stored in a
database, a PDF, a zip. The table shows them as a short placeholder. The
viewer opens them up so you can see what is inside.

## Opening it

Right-click a binary cell and choose **View contents...**. There is also a
**View cell contents...** shortcut for the selected cell. It ships unbound;
give it a key under **Settings > Shortcuts**.

Clicking a picture in the [Notebook view](view-modes/notebook.md) opens it
here too.

## What you see

Octa reads the first bytes of the cell, never a file or column name, to tell
what it holds.

| Kind                      | What the window shows                                                                                                                                                                      |
|---------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| PNG, JPEG, GIF, WebP, SVG | The picture and its size in pixels. A very large picture is shrunk on screen only. An SVG never loads a file or web address it points to.                                                  |
| PDF                       | The page count and the text of all pages.                                                                                                                                                  |
| Zip                       | The files inside, with their sizes and dates. Nothing is unpacked.                                                                                                                         |
| Gzip                      | What the data unpacks to, as text or as a hex dump. Unpacking runs in the background and stops only at the limit under **Settings > Performance** (or never, with Unlimited ticked there). |
| JSON                      | The text, laid out with indentation.                                                                                                                                                       |
| Anything else             | A hex dump of the first 64 KB.                                                                                                                                                             |

Text (from a PDF, a JSON cell or unpacked gzip) shows its first 256 KB.
**Save to file** gets all of it.

A cell that looks like a picture but is damaged shows the reason and falls
back to the hex dump.

## Getting the bytes out

- **Save to file...** writes the cell's bytes to a file you choose. The
  suggested name carries the right extension.
- **Open in default app** writes a copy to a temporary folder and hands it to
  the program your system uses for that kind of file. The copy is deleted
  when Octa closes. Octa runs nothing itself.
