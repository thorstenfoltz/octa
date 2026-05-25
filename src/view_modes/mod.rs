mod chart;
pub mod compare;
mod epub_reader;
mod json_tree;
mod map;
// `markdown` is `pub(crate)` so the EPUB reading view can reuse
// `render_pulldown` without duplicating its 150 lines of pulldown-cmark
// event handling.
pub(crate) mod markdown;
mod notebook;
pub mod raw_text;
mod sql;
pub mod text_ops;

pub use chart::render_chart_view;
pub use compare::render_compare_view;
pub use epub_reader::render_epub_view;
pub use json_tree::{render_json_tree_view, render_yaml_tree_view};
pub use map::render_map_view;
pub use markdown::render_markdown_view;
pub use notebook::render_notebook_view;
pub use raw_text::render_raw_view;
pub use sql::{SqlAction, editor_id as sql_editor_id, render_sql_view};
