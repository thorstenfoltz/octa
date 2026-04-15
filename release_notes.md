## New Formats

- Add ORC format support: read and write Apache ORC files (.orc) via `orc-rust` crate
- Add HDF5 format support: read HDF5 files (.h5, .hdf5, .hdf) with compound datasets and nested groups

## Features

- Add JSON Tree view: collapsible Firefox-style tree viewer for JSON/JSONL files with expand/collapse all, depth control, and inline value editing
- Add depth input field in JSON tree toolbar that accepts Enter key to apply (in addition to the Apply button)
- Add column coloring in aligned raw CSV/TSV view: adjacent columns get distinct colors for readability (6-color cycling palette, theme-aware), enabled by default and configurable in Settings
- Add notebook output layout setting: choose between "Beside" (side by side) and "Beneath" (like Jupyter) in Settings
- Add tab close button hover highlight: x button shows accent background and red color on hover
- Auto-set JSON tree expand depth to file's max depth on open
- Add recent files menu: File > Recent Files shows previously opened files, configurable max count in Settings
- Add new file shortcut: Ctrl+N opens a new empty tab in raw text mode
- Add welcome screen with high-resolution logo centered on the empty tab
- Add Tab key support in text editor: inserts spaces (default 4), configurable tab size in Settings
- Change mouse cursor to default pointer when hovering over tab close button

## Bug Fixes

- Fix Align Columns checkbox in raw CSV/TSV view not responding to clicks (interaction rect overlapped the toolbar)
- Fix Align Columns toggle not reverting: unchecking now restores original file content from disk
- Fix Ctrl+S for unsaved new files: now triggers Save As dialog when raw content is modified

## Settings

- Add "Editor" section with configurable tab size (number of spaces per Tab key press)
- Add "Max recent files" setting to control how many files appear in the Recent Files menu
- Reorganize settings dialog into grouped sections: Appearance, Table View, Search, Editor, Format-Specific, Files
- Add tooltips to all settings fields

## UI Improvements

- Increase selection highlight contrast in both dark and light themes
- Render welcome screen with large centered logo and "Octa" text instead of plain message
- Add Ctrl+N to keyboard shortcuts help
