## Features

- Add a dockable directory tree sidebar (left or right) that opens files into new tabs for quick browsing without leaving the app
- Add SQL autocomplete with keyword and column-name suggestions, keyboard navigation, and dismiss behavior
- Add top and left SQL panel placements with a resizable editor/result split for flexible workspace layouts
- Start new SQL tabs with an empty query and show the configured row limit as placeholder hint text
- Expand remappable table shortcuts with jump-to-edge, selection growth, and SQL result export actions
- Extend upper/lowercase transforms to focused SQL and raw text editors
- Add a configurable startup window size setting
- Show a warning in SQL results when queries run against a partially loaded table
- Always show the tab bar, add full-path tab tooltips, and improve insert-column name autofill

## Fixes

- Invalidate cached row heights after filtering, zooming, cell edits, and column resizing so wrapped rows render correctly
- Replace fixed row-position math with cached per-row height offsets for accurate navigation with variable row heights
- Reserve space for the horizontal scrollbar so the last row is not clipped
- Adjust the SQL editor/result split based on panel position to keep the resize handle accessible
- Improve table scrolling by accounting for resize handles and the vertical scrollbar
- Prevent table navigation and clipboard shortcuts from firing while a text field is focused

## Chores

- Remove Checkov and TruffleHog from MegaLinter config to reduce unused linting overhead
- Reorder plugin declarations in MegaLinter config for clarity
