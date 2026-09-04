# Auto-save

Octa can save your open files automatically on a timer, so you do not have to
remember to press Save. It is **off by default**.

## Turning it on

Open **Settings > Files**:

- **Auto-save**: the on/off switch.
- **Auto-save every (minutes)**: how often a save happens, in whole minutes
  (minimum 1). For example, set 5 and Octa saves every five minutes.

The timer starts fresh whenever you apply Settings, so you always get a full
interval before the first save.

## What it saves

Each time the timer fires, Octa writes every open tab that has **unsaved changes**
and already exists as a **file on disk**, using the same Save you would run by
hand. When it writes at least one file, the status bar shows a brief
"Auto-saved N files" note.

## What it skips

Auto-save never interrupts you with a dialog. It quietly skips:

- Tabs that have never been saved to disk (they have no file yet, so use
  **Save As** once first).
- Cloud-backed tabs while cloud writing is turned off.
- A save that would normally ask a question first, namely a tab with a per-column
  rounding format, an `.xlsx` tab carrying colours, frozen columns or number
  formats (see [Formatting in Excel files](saving.md#formatting-in-excel-files)),
  or a database file where you added or removed columns. Save those by hand so
  you can answer the prompt.
- A tab you are editing at that exact moment; it saves on the next tick once the
  edit is committed.
- A tab whose file has been changed on disk by something else since you opened
  it (see [When the file changed underneath you](saving.md#when-the-file-changed-underneath-you)).
  Auto-save leaves it alone so the question is put to you when you next save by
  hand.
