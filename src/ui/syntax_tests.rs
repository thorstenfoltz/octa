//! Unit tests for [`syntax`](syntax). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::HIGHLIGHT_WHITELIST;
use crate::formats::FormatRegistry;

/// Every extension we bother to syntax-highlight must also be a *supported*
/// file - i.e. claimed by some registered reader so it shows up in the
/// open dialog's "All Supported" filter and isn't silently routed through
/// an unadvertised fallback. This pins `HIGHLIGHT_WHITELIST` and the format
/// registry's extension set together: adding to one without the other
/// fails here.
#[test]
fn highlight_whitelist_is_supported() {
    let registry = FormatRegistry::new();
    let supported = registry.all_extensions();
    let missing: Vec<&str> = HIGHLIGHT_WHITELIST
        .iter()
        .copied()
        .filter(|&ext| !supported.iter().any(|s| s == ext))
        .collect();
    assert!(
        missing.is_empty(),
        "highlighted but not registered as a supported format: {missing:?} \
             - add them to a FormatReader (TextReader for source code)"
    );
}

#[test]
fn structured_formats_resolve_to_a_syntax() {
    for ext in ["json", "yaml", "yml", "xml", "toml"] {
        assert!(
            super::syntax_for_extension(ext).is_some(),
            "raw view should colour .{ext}"
        );
    }
}
