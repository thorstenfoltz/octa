## Features

- Add JSON Tree view: collapsible Firefox-style tree viewer for JSON/JSONL files with expand/collapse all, depth control, and inline value editing
- Add depth input field in JSON tree toolbar that accepts Enter key to apply (in addition to the Apply button)
- Add column coloring in aligned raw CSV/TSV view: adjacent columns get distinct colors for readability (6-color cycling palette, theme-aware), enabled by default and configurable in Settings
- Add notebook output layout setting: choose between "Beside" (side by side) and "Beneath" (like Jupyter) in Settings
- Add tab close button hover highlight: × button shows accent background and red color on hover
- Auto-set JSON tree expand depth to file's max depth on open

## Bug Fixes

- Fix Align Columns checkbox in raw CSV/TSV view not responding to clicks (interaction rect overlapped the toolbar)
- Fix Align Columns toggle not reverting: unchecking now restores original file content from disk

## Internal

- Split monolithic view_modes.rs (~836 lines) into separate modules: json_tree.rs, markdown.rs, notebook.rs, pdf.rs, raw_text.rs
- Add data/json_util.rs: path collection, depth calculation, path-based value get/set, and JSON edit parsing utilities
- Add json_tree_tests.rs with test coverage for JSON utility functions
- Add `color_aligned_columns` and `notebook_output_layout` to AppSettings with TOML persistence
