//! Keeps a tab's text and its table in step across a view switch.
//!
//! Raw, Markdown, the JSON / YAML trees and the text diff show the file's
//! text; every other view reads the table. Both sides can be edited, and
//! neither needs a save before the other shows the change: entering a text
//! view writes the edited table out as the file's text (exactly what Save
//! would write), leaving one reads edited text back into the table. Text that
//! no longer reads as the file's format refuses the switch, with the reason,
//! so the two sides never silently disagree.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use anyhow::Context;
use octa::data::{CompareMode, DataTable, ViewMode};
use octa::formats::{FormatReader, FormatRegistry, write_options::WriteOptions};
use octa::i18n::t;
use octa::ui::table_view::TableViewState;

use super::state::TabState;

impl TabState {
    /// Does the view on screen show the file's text rather than the table?
    pub(crate) fn shows_text(&self) -> bool {
        shows_text(self.view_mode, self.compare_mode)
    }

    /// Does a save write the raw text rather than the table? The text holds
    /// edits and is either the side on screen or the side the table was read
    /// from. Otherwise the table is the newer side, and it already carries
    /// whatever the text said when it was read.
    pub(crate) fn saves_text(&self) -> bool {
        self.raw_content_modified
            && self.raw_content.is_some()
            && (self.shows_text() || !self.table.is_modified())
    }

    /// Run once per frame for the tab on screen, before it draws. A switch
    /// between a text view and a table view is when the side left behind
    /// catches up; a refused switch goes back to where the user came from.
    pub(crate) fn sync_views(&mut self, registry: &FormatRegistry, opts: &WriteOptions) {
        let now = (self.view_mode, self.compare_mode);
        let before = self.synced_view;
        if now == before {
            return;
        }
        let was_text = shows_text(before.0, before.1);
        let mut result = match (was_text, self.shows_text()) {
            (false, true) => self.text_from_table(registry, opts),
            (true, false) => self.table_from_text(registry),
            _ => Ok(()),
        };
        if result.is_ok() && matches!(now.0, ViewMode::JsonTree | ViewMode::YamlTree) {
            result = self.tree_from_text();
        }
        match result {
            Ok(()) => self.synced_view = now,
            Err(msg) => {
                (self.view_mode, self.compare_mode) = before;
                self.parse_error_banner = Some(msg);
            }
        }
    }

    /// Show the table's unsaved edits in the text, written the way Save
    /// would write them.
    fn text_from_table(
        &mut self,
        registry: &FormatRegistry,
        opts: &WriteOptions,
    ) -> Result<(), String> {
        if self.raw_content.is_none() || !self.table.is_modified() {
            return Ok(());
        }
        if self.table.is_partial() {
            return Err(t("viewsync.partial"));
        }
        let text = self.table_as_text(registry, opts).map_err(|e| {
            t("viewsync.unwritten")
                .replace("{format}", self.format_label())
                .replace("{error}", &format!("{e:#}"))
        })?;
        // The table carries the unsaved state now; the text only mirrors it.
        self.text_sync_hash = Some(text_hash(&text));
        self.raw_content_original = Some(text.clone());
        self.raw_content = Some(text);
        self.raw_content_modified = false;
        self.raw_view_formatted = false;
        Ok(())
    }

    /// Read text edited since the table last matched it into the table. Also
    /// run after the text is saved, so the table then matches the file.
    pub(crate) fn table_from_text(&mut self, registry: &FormatRegistry) -> Result<(), String> {
        let Some(text) = self.raw_content.as_deref() else {
            return Ok(());
        };
        if !self.raw_content_modified {
            return Ok(());
        }
        let hash = text_hash(text);
        if self.text_sync_hash == Some(hash) {
            return Ok(());
        }
        if self.table.is_partial() {
            return Err(t("viewsync.partial"));
        }
        let table = self.text_as_table(registry, text).map_err(|e| {
            t("viewsync.unparsed")
                .replace("{format}", self.format_label())
                .replace("{error}", &format!("{e:#}"))
        })?;
        // Too long to read whole: rows beyond the cap would come from the
        // file on disk, not from this text.
        if table.is_partial() {
            return Err(t("viewsync.partial"));
        }
        self.take_table(table);
        self.text_sync_hash = Some(hash);
        Ok(())
    }

    /// Rebuild the JSON / YAML tree from the text, which a table edit or the
    /// raw editor may have changed. Unchanged data keeps the tree's state.
    // ponytail: parses the whole document on every entry into a tree view;
    // keep a hash of the text the tree was built from if big files stutter.
    fn tree_from_text(&mut self) -> Result<(), String> {
        let Some(text) = self.raw_content.as_deref() else {
            return Ok(());
        };
        let yaml = self.view_mode == ViewMode::YamlTree;
        let parsed = if yaml {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text)
                .map(|v| octa::formats::yaml_reader::yaml_to_json(&v))
                .map_err(|e| e.to_string())
        } else {
            serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string())
        };
        let value = parsed.map_err(|e| {
            t("viewsync.unparsed")
                .replace("{format}", if yaml { "YAML" } else { "JSON" })
                .replace("{error}", &e)
        })?;
        let slot = if yaml {
            &mut self.yaml_value
        } else {
            &mut self.json_value
        };
        if slot.as_ref() == Some(&value) {
            return Ok(());
        }
        let depth = octa::data::json_util::max_json_depth(&value);
        *slot = Some(value);
        self.json_nested_docs.clear();
        self.json_file_max_depth = depth;
        self.json_expand_depth = depth;
        self.json_expand_depth_str = depth.to_string();
        Ok(())
    }

    /// The table, edits applied, as the text Save would write.
    fn table_as_text(
        &self,
        registry: &FormatRegistry,
        opts: &WriteOptions,
    ) -> anyhow::Result<String> {
        let mut table = self.table.clone();
        table.apply_edits();
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(self.scratch_name());
        // Same choice of writer as `do_save_tab_inner`.
        if table.format_name.as_deref() == Some("CSV") && self.csv_delimiter != b',' {
            octa::formats::csv_reader::write_delimited(&path, self.csv_delimiter, &table)?;
        } else {
            let writer = self
                .table
                .source_path
                .as_deref()
                .and_then(|p| registry.reader_for_path(Path::new(p)))
                .or_else(|| self.reader(registry))
                .filter(|r| r.supports_write())
                .context("no writer for this format")?;
            writer.write_file_with_options(&path, &table, opts)?;
        }
        Ok(std::fs::read_to_string(&path)?)
    }

    /// `text` read by the reader that produced the table.
    fn text_as_table(&self, registry: &FormatRegistry, text: &str) -> anyhow::Result<DataTable> {
        let reader = self.reader(registry).context("no reader for this format")?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(self.scratch_name());
        std::fs::write(&path, text)?;
        reader.read_file(&path)
    }

    fn reader<'a>(&self, registry: &'a FormatRegistry) -> Option<&'a dyn FormatReader> {
        self.table
            .format_name
            .as_deref()
            .and_then(|n| registry.reader_by_name(n))
            .or_else(|| {
                self.table
                    .source_path
                    .as_deref()
                    .and_then(|p| registry.reader_for_path(Path::new(p)))
            })
    }

    /// The file's own name, so a reader that looks at the extension sees
    /// the same one.
    fn scratch_name(&self) -> std::ffi::OsString {
        self.table
            .source_path
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "text".into())
    }

    fn format_label(&self) -> &str {
        self.table.format_name.as_deref().unwrap_or("text")
    }

    /// Swap in a table read from the text. Whatever is keyed by row or column
    /// position survives only when the shape did; `sync_column_keys` remaps
    /// the column-keyed state by name on the next frame.
    fn take_table(&mut self, mut table: DataTable) {
        let same_shape = table.row_count() == self.table.row_count()
            && table.col_count() == self.table.col_count();
        table.source_path = self.table.source_path.take();
        if same_shape {
            table.marks = std::mem::take(&mut self.table.marks);
        } else {
            self.table_state = TableViewState::default();
            self.bookmarks.clear();
        }
        self.table = table;
        self.table_state.editing_cell = None;
        self.first_row_is_header = true;
        self.outlier_cells.clear();
        self.retype_kept_as_text.clear();
        // Both key on (row count, edits, ...), which the new table can match.
        self.duplicate_filter_cache = None;
        self.timeline.forget_built();
        self.chart_overlay_cache = None;
        self.filter_dirty = true;
    }
}

fn shows_text(mode: ViewMode, compare: CompareMode) -> bool {
    matches!(
        mode,
        ViewMode::Raw | ViewMode::Markdown | ViewMode::JsonTree | ViewMode::YamlTree
    ) || (mode == ViewMode::Compare && compare == CompareMode::TextDiff)
}

fn text_hash(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

#[cfg(test)]
#[path = "view_sync_tests.rs"]
mod tests;
