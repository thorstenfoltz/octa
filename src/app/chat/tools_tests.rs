//! Unit tests for [`tools`](tools). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::mcp::tools::{Source, TableSnapshot};
use octa::data::{CellValue, ColumnInfo, DataTable};

fn sample_ctx() -> ToolContext {
    let mut table = DataTable::empty();
    table.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
    ];
    table.rows = vec![
        vec![CellValue::Int(1), CellValue::String("a".into())],
        vec![CellValue::Int(2), CellValue::String("b".into())],
    ];
    ToolContext {
        large_file_min_bytes: 0,
        open_tabs: vec![TableSnapshot {
            handle: "#1".into(),
            display_name: "demo".into(),
            source_path: None,
            table,
        }],
        active_tab: Some(0),
        default_row_limit: Some(1000),
        cell_byte_cap: 65_536,
        restrict_filesystem: false,
        allowed_read_paths: Vec::new(),
        export_dir: None,
        allow_existing_writes: false,
        allow_schema_changes: false,
        backup_before_modify: true,
        pending_tab_edits: None,
        cloud_settings: None,
        db_connections: Vec::new(),
        read_only: false,
    }
}

/// A single-column (`id`) tab, for multi-tab JOIN tests.
fn id_tab(handle: &str, name: &str, ids: &[i64]) -> TableSnapshot {
    let mut table = DataTable::empty();
    table.columns = vec![ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    table.rows = ids.iter().map(|i| vec![CellValue::Int(*i)]).collect();
    TableSnapshot {
        handle: handle.into(),
        display_name: name.into(),
        source_path: None,
        table,
    }
}

fn multi_ctx() -> ToolContext {
    ToolContext {
        large_file_min_bytes: 0,
        open_tabs: vec![
            id_tab("#1", "a.csv", &[1, 2, 3]),
            id_tab("#2", "b.csv", &[2, 3, 4]),
            id_tab("#3", "c.csv", &[3, 4, 5]),
        ],
        active_tab: Some(0),
        default_row_limit: Some(1000),
        cell_byte_cap: 65_536,
        restrict_filesystem: false,
        allowed_read_paths: Vec::new(),
        export_dir: None,
        allow_existing_writes: false,
        allow_schema_changes: false,
        backup_before_modify: true,
        pending_tab_edits: None,
        cloud_settings: None,
        db_connections: Vec::new(),
        read_only: false,
    }
}

#[test]
fn path_addresses_open_tab_by_handle_or_name() {
    let ctx = sample_ctx();
    // The model often puts the handle / file name in `path` instead of
    // `open_tab` - both resolve to the open tab.
    let by_handle =
        dispatch(&ctx, "schema", serde_json::json!({"path": "#1"})).expect("path handle resolves");
    assert_eq!(by_handle["column_count"], 2);
    let by_name =
        dispatch(&ctx, "schema", serde_json::json!({"path": "demo"})).expect("path name resolves");
    assert_eq!(by_name["column_count"], 2);
}

#[test]
fn run_sql_joins_multiple_open_tabs() {
    let ctx = multi_ctx();
    let out = dispatch(
        &ctx,
        "run_sql",
        serde_json::json!({
            "open_tab": "#1",
            "query": "SELECT count(*) AS n FROM data JOIN b USING(id) JOIN c USING(id)",
            "extra_tables": [{"name": "b", "path": "#2"}, {"name": "c", "path": "#3"}]
        }),
    )
    .expect("3-way join across open tabs");
    assert_eq!(out["kind"], "select");
    // ids common to a{1,2,3}, b{2,3,4}, c{3,4,5} = {3} -> exactly 1 row.
    assert_eq!(out["result"]["rows"][0][0], serde_json::json!(1));
}

#[test]
fn every_tool_has_a_schema_and_description() {
    for def in tool_defs() {
        assert!(!def.name.is_empty());
        assert!(
            !def.description.is_empty(),
            "tool {} has an empty description",
            def.name
        );
        assert_eq!(def.input_schema["type"], "object", "tool {}", def.name);
    }
}

#[test]
fn tool_names_are_unique() {
    let names: Vec<String> = tool_defs().into_iter().map(|d| d.name).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate tool name");
}

#[test]
fn dispatch_schema_against_active_tab() {
    let ctx = sample_ctx();
    let out = dispatch(&ctx, "schema", serde_json::json!({"open_tab": "@active"}))
        .expect("schema dispatch");
    assert_eq!(out["column_count"], 2);
}

#[test]
fn dispatch_run_sql_against_named_tab() {
    let ctx = sample_ctx();
    let out = dispatch(
        &ctx,
        "run_sql",
        serde_json::json!({"open_tab": "demo", "query": "SELECT count(*) AS n FROM data"}),
    )
    .expect("run_sql dispatch");
    assert_eq!(out["kind"], "select");
    // The single result cell is the row count.
    assert_eq!(out["result"]["rows"][0][0], serde_json::json!(2));
}

#[test]
fn run_sql_write_to_csv_file() {
    let ctx = sample_ctx();
    let path = std::env::temp_dir().join("octa_run_sql_write_test.csv");
    let _ = std::fs::remove_file(&path);
    let out = dispatch(
        &ctx,
        "run_sql",
        serde_json::json!({
            "open_tab": "demo",
            "query": "SELECT * FROM data",
            "write_to": { "path": path.to_str().unwrap(), "table": "ignored" }
        }),
    )
    .expect("run_sql write_to csv");
    assert_eq!(out["kind"], "write_back");
    assert_eq!(out["rows_written"], serde_json::json!(2));
    let written = std::fs::read_to_string(&path).expect("csv written");
    // Header + 2 data rows.
    assert_eq!(written.lines().count(), 3);
    assert!(written.contains("id"));
    let _ = std::fs::remove_file(&path);
}

/// A single "Line" column tab, the shape text/code/markdown files load as.
fn text_ctx(lines: &[&str], source_path: Option<&str>) -> ToolContext {
    let mut table = DataTable::empty();
    table.columns = vec![ColumnInfo {
        name: "Line".into(),
        data_type: "Utf8".into(),
    }];
    table.rows = lines
        .iter()
        .map(|l| vec![CellValue::String((*l).into())])
        .collect();
    ToolContext {
        large_file_min_bytes: 0,
        open_tabs: vec![TableSnapshot {
            handle: "#1".into(),
            display_name: "notes.md".into(),
            source_path: source_path.map(|s| s.to_string()),
            table,
        }],
        active_tab: Some(0),
        default_row_limit: Some(1000),
        cell_byte_cap: 65_536,
        restrict_filesystem: false,
        allowed_read_paths: Vec::new(),
        export_dir: None,
        allow_existing_writes: false,
        allow_schema_changes: false,
        backup_before_modify: true,
        pending_tab_edits: None,
        cloud_settings: None,
        db_connections: Vec::new(),
        read_only: false,
    }
}

#[test]
fn read_text_rejoins_lines() {
    let ctx = text_ctx(&["# Title", "", "body line"], None);
    let out = dispatch(
        &ctx,
        "read_text",
        serde_json::json!({"open_tab": "@active"}),
    )
    .expect("read_text dispatch");
    assert_eq!(out["text"], "# Title\n\nbody line");
    assert_eq!(out["line_count"], 3);
}

#[test]
fn write_text_writes_file() {
    let ctx = text_ctx(&["old"], None);
    let path = std::env::temp_dir().join("octa_write_text_test.md");
    let _ = std::fs::remove_file(&path);
    let out = dispatch(
        &ctx,
        "write_text",
        serde_json::json!({"content": "new content\n", "path": path.to_str().unwrap()}),
    )
    .expect("write_text dispatch");
    assert_eq!(out["bytes_written"], serde_json::json!(12));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn write_text_outside_export_dir_is_refused_when_sandboxed() {
    let export = tempfile::tempdir().expect("tempdir");
    let mut ctx = text_ctx(&["old"], None);
    ctx.restrict_filesystem = true;
    ctx.export_dir = Some(export.path().to_path_buf());
    let err = dispatch(
        &ctx,
        "write_text",
        serde_json::json!({"content": "x", "path": "/tmp/escape.md"}),
    )
    .unwrap_err();
    assert!(err.contains("confined"), "{err}");
    // A bare name still works and lands in the export dir.
    let out = dispatch(
        &ctx,
        "write_text",
        serde_json::json!({"content": "x", "path": "ok.md"}),
    )
    .expect("bare name allowed");
    let written = out["path"].as_str().expect("path in response");
    assert!(
        std::path::Path::new(written)
            .starts_with(std::fs::canonicalize(export.path()).expect("canonical export dir")),
        "{written}"
    );
}

#[test]
fn tool_defs_for_filters_the_write_tools() {
    let none = BTreeSet::new();
    let restricted = tool_defs_for(false, &none);
    for name in write_tool_names() {
        assert!(
            !restricted.iter().any(|d| d.name == name),
            "{name} must be hidden without writes"
        );
    }
    // Every write tool actually names a registered tool (no typo drift).
    let all = tool_defs_for(true, &none);
    for name in write_tool_names() {
        assert!(all.iter().any(|d| d.name == name), "{name} unknown");
    }
    assert_eq!(all.len(), tool_defs().len());
    assert_eq!(all.len(), restricted.len() + write_tool_names().len());

    // A tool the user switched off is gone from the pool as well.
    let off = BTreeSet::from(["run_sql".to_string()]);
    let trimmed = tool_defs_for(true, &off);
    assert_eq!(trimmed.len(), all.len() - 1);
    assert!(!trimmed.iter().any(|d| d.name == "run_sql"));
}

/// Every registered tool is in the catalogue and vice versa: the settings
/// list, the `enable_tools` menu and the token sums all read from it, so a
/// tool missing from it would silently become unreachable.
#[test]
fn every_tool_is_catalogued() {
    use crate::app::chat::tool_groups::CATALOG;
    let defs = tool_defs();
    let registered: BTreeSet<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    let catalogued: BTreeSet<&str> = CATALOG.iter().map(|e| e.name).collect();
    assert_eq!(
        registered, catalogued,
        "define_chat_tools! and tool_groups::CATALOG disagree"
    );
    assert_eq!(
        CATALOG.len(),
        catalogued.len(),
        "a tool is listed twice in the catalogue"
    );
    for entry in CATALOG {
        assert!(
            entry.when.len() > 10 && entry.when.ends_with('.'),
            "{} needs a one-line 'when to enable'",
            entry.name
        );
    }
}

/// The first request carries the core group and the menu, and nothing else.
#[test]
fn the_first_request_carries_core_plus_the_menu() {
    use crate::app::chat::tool_groups::{CATALOG, ToolGroup};
    let none = BTreeSet::new();
    let initial = initial_tool_defs(true, &none);
    let core: BTreeSet<&str> = CATALOG
        .iter()
        .filter(|e| e.group == ToolGroup::Core)
        .map(|e| e.name)
        .collect();
    let sent: BTreeSet<&str> = initial
        .iter()
        .map(|d| d.name.as_str())
        .filter(|n| *n != ENABLE_TOOLS)
        .collect();
    assert_eq!(sent, core);
    assert!(initial.iter().any(|d| d.name == ENABLE_TOOLS));
    // The menu names every other tool, so the model knows they exist.
    let menu = &initial
        .iter()
        .find(|d| d.name == ENABLE_TOOLS)
        .expect("menu")
        .description;
    for entry in CATALOG {
        if entry.group == ToolGroup::Core {
            continue;
        }
        assert!(
            menu.contains(entry.name),
            "{} missing from the menu",
            entry.name
        );
    }
}

/// Fetching a group returns its definitions, refuses to duplicate them, and
/// says so when the name is not a group.
#[test]
fn enable_groups_loads_a_group_once() {
    let none = BTreeSet::new();
    let mut live = initial_tool_defs(true, &none);
    let (added, msg) = enable_groups(&json!({ "groups": ["combine"] }), true, &none, &live);
    assert!(added.iter().any(|d| d.name == "fuzzy_join"), "{msg}");
    assert!(msg.contains("fuzzy_join"));
    live.extend(added);

    // Asking again adds nothing: the turn already has them.
    let (again, _) = enable_groups(&json!({ "groups": ["combine"] }), true, &none, &live);
    assert!(again.is_empty());

    // A guessed name is answered with the real ones.
    let (nothing, err) = enable_groups(&json!({ "groups": ["joins"] }), true, &none, &live);
    assert!(nothing.is_empty());
    assert!(err.contains("no such group"), "{err}");
    assert!(err.contains("combine"), "{err}");

    // Without writes, the write group carries nothing.
    let (empty, _) = enable_groups(&json!({ "groups": ["write"] }), false, &none, &[]);
    assert!(empty.is_empty());
}

/// With every non-core tool switched off there is nothing to fetch, so the
/// menu is not sent at all rather than advertising an empty list.
#[test]
fn no_menu_when_nothing_is_left_to_load() {
    use crate::app::chat::tool_groups::{CATALOG, ToolGroup};
    let off: BTreeSet<String> = CATALOG
        .iter()
        .filter(|e| e.group != ToolGroup::Core)
        .map(|e| e.name.to_string())
        .collect();
    let initial = initial_tool_defs(true, &off);
    assert!(!initial.iter().any(|d| d.name == ENABLE_TOOLS));
    assert!(enable_tools_def(true, &off).is_none());
}

#[test]
fn dispatch_refuses_write_tools_on_a_read_only_ctx() {
    let mut ctx = sample_ctx();
    ctx.read_only = true;
    let err = dispatch(
        &ctx,
        "write_table",
        serde_json::json!({"path": "/tmp/never-written.csv", "columns": [{"name": "a"}]}),
    )
    .unwrap_err();
    assert!(err.contains("does not allow writes"), "{err}");
    assert!(!std::path::Path::new("/tmp/never-written.csv").exists());
}

#[test]
fn run_sql_write_to_refuses_on_a_read_only_ctx() {
    // run_sql stays advertised (core read tool), so its write_to branch must
    // gate itself on ctx.read_only.
    let mut ctx = sample_ctx();
    ctx.read_only = true;
    let err = dispatch(
        &ctx,
        "run_sql",
        serde_json::json!({
            "open_tab": "#1",
            "query": "SELECT * FROM data",
            "write_to": {"path": "/tmp/never-written.duckdb", "table": "t", "mode": "create"}
        }),
    )
    .unwrap_err();
    assert!(err.contains("write_to is not available"), "{err}");
}

#[test]
fn unknown_tool_errors_cleanly() {
    let ctx = sample_ctx();
    let err = dispatch(&ctx, "nope", serde_json::json!({})).unwrap_err();
    assert!(err.contains("unknown tool"));
}

#[test]
fn write_back_to_open_tab_is_rejected() {
    let ctx = sample_ctx();
    // The Source enum is part of the shared API the chat layer builds on.
    let _ = Source::ActiveTab;
    let err = dispatch(
        &ctx,
        "write_table",
        serde_json::json!({"path": "/tmp/x.csv", "open_tab": "demo", "columns": [{"name": "a"}]}),
    )
    .unwrap_err();
    assert!(err.contains("not supported"));
}

/// What the first request actually costs, and a ceiling on it.
///
/// The whole point of the split is that a short question stops paying for 62
/// tool schemas. If someone moves a big tool into the core group this fails
/// rather than quietly putting the cost back. Run with `--nocapture` to see
/// the numbers.
#[test]
fn the_core_payload_stays_small() {
    use crate::app::chat::tool_groups::approx_tokens;
    let none = BTreeSet::new();
    let full: usize = tool_defs_for(true, &none).iter().map(approx_tokens).sum();
    let initial: usize = initial_tool_defs(true, &none)
        .iter()
        .map(approx_tokens)
        .sum();
    let read_only: usize = initial_tool_defs(false, &none)
        .iter()
        .map(approx_tokens)
        .sum();
    println!("tool payload: every tool ~{full}, first request ~{initial} (read-only ~{read_only})");
    assert!(
        initial < 10_000,
        "the core group plus the menu grew to ~{initial} tokens"
    );
    assert!(
        initial * 2 < full,
        "the split has to be worth having: ~{initial} of ~{full}"
    );
}
