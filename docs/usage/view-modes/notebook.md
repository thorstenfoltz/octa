# Notebook View

For `.ipynb` files Octa renders the notebook the way Jupyter does:
code cells with syntect syntax highlighting, Markdown cells through
the Markdown renderer, and outputs (stdout, stderr, plots, HTML)
underneath each code cell.

<!-- SCREENSHOT: notebook-view.png: A notebook view with code cells (Python), a Markdown heading + paragraph, output text below a cell, and a small image output. -->
![Notebook view](../../assets/screenshots/notebook-view.png){ .screenshot-placeholder }

## What gets rendered

| Cell type                    | Renderer                                                             |
|------------------------------|----------------------------------------------------------------------|
| `markdown`                   | The same pulldown-cmark renderer as the [Markdown view](markdown.md) |
| `code` (Python, Julia, R, …) | Syntect with the notebook's declared language                        |
| `raw`                        | Plain monospace                                                      |

Each code cell shows its execution count (`[1]`, `[2]`, etc.) in
the left margin. Cells without an execution count display `[ ]`.

## Output rendering

Outputs are listed below each code cell:

- **Text output** (`stdout`, `stderr`): monospace, no truncation.
- **HTML output**: rendered as plain text in v1 (Octa doesn't
  embed an HTML renderer).
- **Plot output** (image/png base64): decoded and shown inline at
  natural size.
- **Error output**: traceback in red.

## Layout

The notebook view shows cells in document order from top to bottom
in a vertical scroll area. Each cell is rendered in its own bordered
block.

The **Output layout** under
[**Settings → File-Specific → Notebook output layout**](../../reference/settings.md#file-specific)
picks between:

- **Below cell** (default) shows outputs directly under their
  source cell.
- **Side-by-side** shows outputs to the right of the source cell
  (better use of wide screens, but compresses long outputs).

## Editing cell source

Cell **source is editable**. Click into any code or Markdown cell and
type; code cells keep their syntax highlighting while you edit. The
output blocks below each cell stay read-only.

- Edits flow through the normal table machinery, so the **modified
  marker**, **undo / redo** (`Ctrl+Z` / `Ctrl+Y`), and **Save** all
  work exactly as in the table view.
- **Read-only mode** (`F8`) disables editing, as it does everywhere
  else in Octa.

### Saving preserves outputs

When you save an edited notebook, Octa reuses the original `.ipynb` as
a template and rewrites only the cell **source** (and cell type),
leaving each cell's **outputs**, **execution count**, and per-cell
**metadata** intact, along with the notebook's top-level metadata
(kernelspec, `nbformat`, etc.). Earlier versions rebuilt the file from
scratch and dropped all outputs on save; that no longer happens.

As in Jupyter, editing a cell does **not** re-run it, so an edited
code cell keeps its previous (now possibly stale) output until you run
it again in a kernel.

## What you can't do

The notebook view still can't:

- Run cells (Octa has no kernel; use Jupyter / VS Code to execute).
- Add or delete cells from the Notebook view itself.

If you need to inspect the notebook's table-of-values structure
(e.g. to filter all cells matching a pattern), or to add / delete
cells, switch to [**Table view**](../table-view.md). Each cell becomes
a row with `cell_type`, `source`, `outputs`, etc. as columns. From
there the [search bar](../search-and-filter.md) or
[SQL panel](../sql.md) can filter the cells, and the usual row
insert / delete operations apply.

## Languages with syntax highlighting

The cell's language comes from the notebook's
`metadata.kernelspec.language` (or `metadata.kernelspec.name` as a
fallback). Octa maps common names to syntect language packs:

| Notebook language          | Syntect grammar         |
|----------------------------|-------------------------|
| `python`, `python3`        | Python                  |
| `julia`                    | Julia                   |
| `r`                        | R                       |
| `rust`                     | Rust                    |
| `bash`, `sh`               | Bash                    |
| `javascript`, `typescript` | JavaScript / TypeScript |

Unknown languages fall back to plain monospace.

## Limitations

- **No interactive widgets.** ipywidgets, Plotly, etc. don't render.
- **No cell execution.** Editing source does not run the cell or
  refresh its output.
- **No diffing**, though the [Compare view](compare.md) handles
  text-diffing two `.ipynb` files via the Table representation.

## See also

- [Markdown view](markdown.md) shares the renderer that powers
  notebook Markdown cells.
- [Settings → File-Specific](../../reference/settings.md#file-specific)
  is where you change the output layout.
