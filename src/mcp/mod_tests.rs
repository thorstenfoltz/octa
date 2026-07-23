//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `read_only_tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn read_only_drops_write_tools() {
    let ro = OctaMcpServer::new(Some(1000), 65536, true, false, true);
    for name in [
        "write_table",
        "edit_table",
        "convert",
        "transform_columns",
        "anonymize",
        "partition_table",
        "write_db_table",
        "copy_db_table",
    ] {
        assert!(
            !ro.tool_router.has_route(name),
            "read-only server should not expose `{name}`"
        );
    }
    // Read tools (including the read-only analytics tools) are still present.
    for name in [
        "read_table",
        "pivot",
        "correlation",
        "grep_files",
        "list_objects",
        "list_db_connections",
        "list_db_tables",
        "query_db",
        "fuzzy_duplicates",
        "union_tables",
        "join_tables",
        "drop_duplicates",
        "fill_missing",
        "detect_outliers",
        "detect_pii",
    ] {
        assert!(ro.tool_router.has_route(name), "`{name}` should be present");
    }
}

#[test]
fn default_keeps_write_tools() {
    let rw = OctaMcpServer::new(Some(1000), 65536, false, false, true);
    for name in [
        "write_table",
        "write_db_table",
        "edit_table",
        "convert",
        "read_table",
        "transform_columns",
        "anonymize",
        "pivot",
        "correlation",
        "grep_files",
        "union_tables",
        "join_tables",
        "drop_duplicates",
        "fill_missing",
        "detect_outliers",
        "detect_pii",
        "partition_table",
    ] {
        assert!(rw.tool_router.has_route(name), "`{name}` should be present");
    }
}
