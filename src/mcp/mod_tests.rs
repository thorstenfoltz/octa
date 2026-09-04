//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `read_only_tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn read_only_drops_write_tools() {
    let ro = OctaMcpServer::new(Some(1000), 65536, true, false, true, 0, &[]);
    for name in [
        "write_table",
        "edit_table",
        "convert",
        "transform_columns",
        "anonymize",
        "partition_table",
        "write_db_table",
        "copy_db_table",
        "copy_object",
        "move_object",
        "delete_object",
        "harmonise_schemas",
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
        "diagnose_join",
    ] {
        assert!(ro.tool_router.has_route(name), "`{name}` should be present");
    }
}

#[test]
fn default_keeps_write_tools() {
    let rw = OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &[]);
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
        "harmonise_schemas",
        "diagnose_join",
    ] {
        assert!(rw.tool_router.has_route(name), "`{name}` should be present");
    }
}

/// `--mcp-tools core` leaves the core group and nothing else. The client reads
/// the tool list once and carries it in every request to its model, so this is
/// the lever that actually shrinks an agent's context.
#[test]
fn a_tool_filter_hides_everything_it_did_not_name() {
    let hidden = tool_groups::hidden_tools(&["core".to_string()], &[]).expect("valid group");
    let server = OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &hidden);
    for name in ["read_table", "schema", "run_sql", "profile"] {
        assert!(server.tool_router.has_route(name), "`{name}` is core");
    }
    for name in ["fuzzy_join", "detect_pii", "list_objects", "write_table"] {
        assert!(
            !server.tool_router.has_route(name),
            "`{name}` was not asked for"
        );
    }
}

/// The filter stacks with read-only rather than fighting it.
#[test]
fn a_filter_and_read_only_both_apply() {
    let hidden = tool_groups::hidden_tools(&["core".to_string(), "write".to_string()], &[])
        .expect("valid groups");
    let server = OctaMcpServer::new(Some(1000), 65536, true, false, true, 0, &hidden);
    assert!(server.tool_router.has_route("read_table"));
    assert!(
        !server.tool_router.has_route("write_table"),
        "read-only still wins over an explicit --mcp-tools write"
    );
}
