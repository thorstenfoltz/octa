//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `read_only_tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn read_only_drops_write_tools() {
    let ro = OctaMcpServer::new(Some(1000), 65536, true);
    for name in ["write_table", "edit_table", "convert"] {
        assert!(
            !ro.tool_router.has_route(name),
            "read-only server should not expose `{name}`"
        );
    }
    // A read tool is still present.
    assert!(ro.tool_router.has_route("read_table"));
}

#[test]
fn default_keeps_write_tools() {
    let rw = OctaMcpServer::new(Some(1000), 65536, false);
    for name in ["write_table", "edit_table", "convert", "read_table"] {
        assert!(rw.tool_router.has_route(name), "`{name}` should be present");
    }
}
