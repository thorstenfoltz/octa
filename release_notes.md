## Features

- Apply a color to **every cell in a free multi-cell selection** from the Edit > Mark menu — previously only the anchor cell was colored
- New **Mark** keyboard shortcut (default Ctrl+M, remappable) that paints the current selection (rows, columns, multi-cell, or single cell) with the configured default color
- New **Default mark color** setting (Settings > Table, Yellow by default) — drives the Mark shortcut
- Surface **Undo / Redo in the Edit menu**, with the current keybinding shown next to each entry and disabled state when the stacks are empty
- Move **Undo / Redo into the customizable shortcut system** (Settings > Shortcuts) so the default Ctrl+Z / Ctrl+Y bindings can be rebound; they now appear in the auto-generated shortcut documentation
- Add two new UI themes — **Manga** (cream-paper light theme with sakura-pink and sky-blue accents on bold ink-black text) and **Gentleman** (deep walnut and burgundy dark theme with champagne-gold accents on warm parchment text)
- A few hidden surprises are now lurking in the app — explore the status bar, the About dialog, and the SQL view to find them

## Fixes

- Make the **Settings dialog draggable** — it now opens centered but can be moved freely, matching the Documentation dialog's behavior; the About dialog gets the same treatment
- Replace the awkward Settings **font size drag-arrow** with a dropdown listing every integer 8–32 pt
- Strengthen window **close-X hover highlighting** with an accent-tinted fill and thicker stroke so the button reads clearly
- Widen the status-bar **Go to R:C** input from 120 to 180 px so the hint text and short inputs are no longer clipped
- Scope **Ctrl+Z / Ctrl+Y** to the focused TextEdit (SQL editor, raw editor, search bar) so text-undo no longer triggers table undo

## CI

- Drop the redundant `cargo build` step in the test job — `cargo test` already compiles every workspace target, so the previous setup compiled the same code twice
- Cache the custom MegaLinter Docker image's layers via the GitHub Actions cache backend (Buildx + `type=gha`), turning warm runs of the lint job from minutes into seconds
