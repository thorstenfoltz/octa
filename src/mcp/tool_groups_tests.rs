use super::*;

#[test]
fn no_lists_hides_nothing() {
    assert!(hidden_tools(&[], &[]).unwrap().is_empty());
}

#[test]
fn a_group_keeps_only_its_own_tools() {
    let hidden = hidden_tools(&["core".to_string()], &[]).unwrap();
    assert!(!hidden.contains(&"read_table"), "core survives");
    assert!(hidden.contains(&"fuzzy_join"), "everything else is hidden");
}

#[test]
fn a_tool_name_works_on_its_own_and_mixes_with_groups() {
    let hidden = hidden_tools(&["cloud".to_string(), "run_sql".to_string()], &[]).unwrap();
    assert!(!hidden.contains(&"list_objects"));
    assert!(!hidden.contains(&"run_sql"));
    assert!(hidden.contains(&"read_table"));
}

#[test]
fn without_subtracts_from_what_survived() {
    let hidden = hidden_tools(&["core".to_string()], &["run_sql".to_string()]).unwrap();
    assert!(hidden.contains(&"run_sql"));
    assert!(!hidden.contains(&"read_table"));
    // On its own it just removes that group.
    let hidden = hidden_tools(&[], &["cloud".to_string()]).unwrap();
    assert!(hidden.contains(&"delete_object"));
    assert!(!hidden.contains(&"read_table"));
}

/// Stray whitespace around a name is the shell's, not the user's mistake.
#[test]
fn surrounding_whitespace_is_ignored() {
    let hidden = hidden_tools(&[" quality ".to_string()], &[]).expect("trimmed");
    assert!(!hidden.contains(&"detect_pii"));
}

#[test]
fn an_unknown_word_names_the_groups() {
    let err = hidden_tools(&["qualityy".to_string()], &[]).unwrap_err();
    assert!(err.contains("qualityy"), "{err}");
    assert!(err.contains("quality"), "{err}");
    assert!(err.contains("databases"), "{err}");
}

/// Every tool the server advertises is in the site's tool reference, with
/// the group `--mcp-tools` expects. A tool added without a row there would
/// otherwise be invisible to anyone configuring the server, since the MCP
/// user has no Settings list to look at.
#[test]
fn the_docs_table_lists_every_tool_with_its_group() {
    let server = crate::mcp::OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &[]);
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/mcp/tools/index.md"),
    )
    .expect("the tool reference page");

    for tool in server.tool_router.list_all() {
        let name = tool.name.as_ref();
        let group = ToolGroup::of(name)
            .unwrap_or_else(|| panic!("`{name}` is served but not in the catalogue"))
            .id();
        let row = doc
            .lines()
            .find(|l| l.starts_with(&format!("| **[`{name}`]")))
            .unwrap_or_else(|| panic!("`{name}` has no row in docs/mcp/tools/index.md"));
        assert!(
            row.contains(&format!("`{group}`")),
            "`{name}` is in group `{group}`, but its docs row says otherwise: {row}"
        );
    }
}

/// And nothing in the table is made up.
#[test]
fn the_docs_table_invents_no_tools() {
    let server = crate::mcp::OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &[]);
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/mcp/tools/index.md"),
    )
    .expect("the tool reference page");
    for line in doc.lines().filter(|l| l.starts_with("| **[`")) {
        let name = line
            .trim_start_matches("| **[`")
            .split('`')
            .next()
            .expect("a tool name");
        assert!(
            server.tool_router.has_route(name),
            "docs/mcp/tools/index.md lists `{name}`, which the server does not serve"
        );
    }
}
