//! Display/mode enums for the various views (Table, Map, Compare, Markdown,
//! Search). Split out of `data/mod.rs`; each carries an English `label` and a
//! translated `label_t`.

use serde::{Deserialize, Serialize};

/// How to display the loaded file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Structured tabular view (default).
    Table,
    /// Raw text view of the file content (like a text editor).
    Raw,
    /// Rendered Markdown view.
    Markdown,
    /// Rendered Jupyter Notebook view.
    Notebook,
    /// Collapsible JSON tree view (like Firefox JSON viewer).
    JsonTree,
    /// Collapsible YAML tree view (mirrors JsonTree, fed by the YAML parser).
    YamlTree,
    /// Side-by-side comparison of two files. The active tab provides the
    /// left side; the right side is loaded via View -> Compare with...
    /// Two sub-modes toggle within the view (Text Diff / Row Hash Diff).
    Compare,
    /// EPUB reading view. Renders the book chapter-by-chapter as Markdown
    /// (converted from each spine entry's XHTML at load time). Embedded
    /// images decode lazily into egui textures.
    EpubReader,
    /// GeoJSON map view. Renders feature geometries on top of slippy-map
    /// tiles (when network is reachable) or a plain canvas (geometry-only
    /// mode / offline fallback).
    Map,
    /// `egui_plot`-backed chart view. The user picks a chart kind
    /// (Histogram / Bar / Line / Scatter / Box) and the X / Y columns
    /// through a control bar at the top of the view; data prep lives in
    /// [`chart::build_chart`](super::chart::build_chart) so the same code path
    /// is integration-tested.
    Chart,
}

/// Tile-rendering mode for the Map view. `Tiles` fetches raster tiles from
/// the configured `map_tile_url_template`; `GeometryOnly` skips the tile
/// fetch and paints just the geometry on a neutral background. The default
/// per-tab choice is set by `AppSettings.map_default_mode`; the user can
/// flip between modes via the Map view's toolbar toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MapMode {
    /// Show OSM (or configured) tiles behind the features. Requires net.
    #[default]
    Tiles,
    /// Geometry only, no tiles. Useful offline or when the tile server is
    /// blocked.
    GeometryOnly,
}

impl MapMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tiles => "Tiles",
            Self::GeometryOnly => "Geometry only",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Tiles => "enum.map_tiles",
            Self::GeometryOnly => "enum.map_geometry",
        })
    }

    pub const ALL: &'static [Self] = &[Self::Tiles, Self::GeometryOnly];
}

/// Sub-mode for the Compare view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CompareMode {
    /// Line-by-line git-style diff of the raw text content.
    #[default]
    TextDiff,
    /// Hash the user-picked columns per row and report uniques + duplicates
    /// across both files. Cross-format because only cell text is hashed.
    RowHashDiff,
    /// Positional row-by-row cell comparison (`compare_ordered`): row N on the
    /// left versus row N on the right, naming the columns that differ.
    Ordered,
    /// Key-matched comparison (`compare_join`): rows paired by user-picked key
    /// column(s), reporting added / removed / changed rows and which columns
    /// changed.
    Join,
}

impl CompareMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::TextDiff => "Text Diff",
            Self::RowHashDiff => "Row Hash Diff",
            Self::Ordered => "Ordered",
            Self::Join => "Join (by key)",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::TextDiff => "enum2.compare_text_diff",
            Self::RowHashDiff => "enum2.compare_row_hash",
            Self::Ordered => "enum2.compare_ordered",
            Self::Join => "enum2.compare_join",
        })
    }
}

/// Layout for the Markdown view: preview-only, side-by-side editor + preview,
/// or edit-only. Toggled via the segmented button in the markdown toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MarkdownLayout {
    /// Render the markdown only. Default - opening a .md file shows the
    /// rendered document; the toolbar toggle switches to Split/Edit.
    #[default]
    Preview,
    /// TextEdit on the left, rendered preview on the right. Live updates.
    Split,
    /// TextEdit only - no preview pane. Useful for full-width editing.
    Edit,
}

/// Search/filter mode for the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchMode {
    /// Plain case-insensitive substring match.
    #[default]
    Plain,
    /// Wildcard: `*` = any chars, `?` = single char. Escape with `\*` and `\?`.
    Wildcard,
    /// Full regular expression (regex crate syntax).
    Regex,
}

impl SearchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Wildcard => "Wildcard",
            Self::Regex => "Regex",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Plain => "enum.search_plain",
            Self::Wildcard => "enum.search_wildcard",
            Self::Regex => "enum.search_regex",
        })
    }
}

/// How the search query affects the display.
///
/// `Filter` is the original behaviour: rows that do not match are hidden.
/// `Highlight` keeps every row visible and paints the matching cells instead.
/// The toggle only governs the **table** view; text-like and tree views
/// (Notebook, Raw, Markdown, JSON/YAML tree) always highlight because hiding
/// free text or collapsing tree nodes is meaningless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchResultMode {
    /// Hide non-matching rows (table only).
    #[default]
    Filter,
    /// Keep all rows; highlight matches in place.
    Highlight,
}

impl SearchResultMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Filter => "Filter",
            Self::Highlight => "Highlight",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Filter => "enum.search_result_filter",
            Self::Highlight => "enum.search_result_highlight",
        })
    }
}
