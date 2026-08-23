//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

fn sandbox_ctx(restrict: bool, allowed: &[&str], export: Option<&str>) -> ToolContext {
    ToolContext {
        large_file_min_bytes: 0,
        open_tabs: Vec::new(),
        active_tab: None,
        default_row_limit: Some(100),
        cell_byte_cap: 4096,
        restrict_filesystem: restrict,
        allowed_read_paths: allowed.iter().map(PathBuf::from).collect(),
        export_dir: export.map(PathBuf::from),
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
fn read_sandbox_allows_open_files_only() {
    let c = sandbox_ctx(true, &["/nope/open.csv"], None);
    assert!(c.ensure_readable(Path::new("/nope/open.csv")).is_ok());
    let err = c
        .ensure_readable(Path::new("/nope/secret.csv"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("only read files that are open"));
}

#[test]
fn read_unrestricted_allows_anything() {
    let c = sandbox_ctx(false, &[], None);
    assert!(c.ensure_readable(Path::new("/anywhere/x.csv")).is_ok());
}

#[test]
fn write_path_confined_to_export_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let export = std::fs::canonicalize(tmp.path()).expect("canonical export dir");
    let c = sandbox_ctx(true, &[], export.to_str());
    // Bare + nested-relative names land in the export dir (basename only,
    // which also neutralises `..` components).
    assert_eq!(
        c.resolve_write_path(Path::new("out.csv")).unwrap(),
        export.join("out.csv")
    );
    assert_eq!(
        c.resolve_write_path(Path::new("sub/dir/out.csv")).unwrap(),
        export.join("out.csv")
    );
    assert_eq!(
        c.resolve_write_path(Path::new("../escape.csv")).unwrap(),
        export.join("escape.csv")
    );
    // An absolute path inside the export dir is accepted.
    assert_eq!(
        c.resolve_write_path(&export.join("explicit.csv")).unwrap(),
        export.join("explicit.csv")
    );
    // Any other absolute path is refused: writes are confined.
    let err = c
        .resolve_write_path(Path::new("/tmp/explicit.csv"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("confined"), "{err}");
    let err = c
        .resolve_write_path(Path::new("/etc/passwd"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("confined"), "{err}");
}

#[cfg(unix)]
#[test]
fn write_path_rejects_symlink_escape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let export = std::fs::canonicalize(tmp.path()).expect("canonical export dir");
    let outside = tempfile::tempdir().expect("outside dir");
    let target = outside.path().join("victim.csv");
    std::fs::write(&target, "x").expect("write victim");
    std::os::unix::fs::symlink(&target, export.join("link.csv")).expect("symlink");
    let c = sandbox_ctx(true, &[], export.to_str());
    let err = c
        .resolve_write_path(Path::new("link.csv"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("confined"), "{err}");
    // A symlink that stays inside the export dir is fine.
    std::fs::write(export.join("inside.csv"), "y").expect("write inside");
    std::os::unix::fs::symlink(export.join("inside.csv"), export.join("ok.csv"))
        .expect("symlink inside");
    assert_eq!(
        c.resolve_write_path(Path::new("ok.csv")).unwrap(),
        export.join("ok.csv")
    );
}

#[test]
fn resolve_write_path_allows_existing_when_unlocked() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.csv");
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    // Simulate the chat sandbox with the unlock on.
    ctx.restrict_filesystem = true;
    ctx.export_dir = Some(dir.path().join("exports"));
    ctx.allow_existing_writes = true;
    let resolved = ctx.resolve_write_path(&target).unwrap();
    assert_eq!(resolved, target, "unlocked writes pass the path through");

    // With the lock on, an outside path is confined / rejected.
    ctx.allow_existing_writes = false;
    assert!(ctx.resolve_write_path(&target).is_err());
}

#[test]
fn unlocked_writes_still_put_bare_names_in_the_export_dir() {
    // The unlock lifts *confinement* (an absolute path may target an existing
    // file anywhere). It must not change where a bare or relative name lands:
    // that is still the export dir. Otherwise the name stays relative and the
    // write resolves against the process CWD - the user's home for a GUI
    // launched from the desktop - silently ignoring Settings > Chat.
    let tmp = tempfile::tempdir().unwrap();
    let export = std::fs::canonicalize(tmp.path()).unwrap().join("exports");
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    ctx.restrict_filesystem = true;
    ctx.export_dir = Some(export.clone());
    ctx.allow_existing_writes = true;

    assert_eq!(
        ctx.resolve_write_path(Path::new("out.csv")).unwrap(),
        export.join("out.csv"),
        "a bare name must land in the export dir, not the process CWD"
    );
    assert_eq!(
        ctx.resolve_write_path(Path::new("sub/out.csv")).unwrap(),
        export.join("sub/out.csv"),
        "a relative subpath is kept, resolved under the export dir"
    );
}

#[test]
fn unlocked_writes_pass_absolute_paths_through() {
    // The other half of the contract: with the unlock on, an absolute path
    // outside the export dir is still honoured so the agent can overwrite a
    // file the user already has open.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("outside.csv");
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    ctx.restrict_filesystem = true;
    ctx.export_dir = Some(dir.path().join("exports"));
    ctx.allow_existing_writes = true;
    assert_eq!(ctx.resolve_write_path(&target).unwrap(), target);
}

#[test]
fn unlocked_writes_without_an_export_dir_stay_relative() {
    // Nothing to resolve against when the setting is blank: pass through
    // rather than invent a directory.
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    ctx.restrict_filesystem = true;
    ctx.export_dir = None;
    ctx.allow_existing_writes = true;
    assert_eq!(
        ctx.resolve_write_path(Path::new("out.csv")).unwrap(),
        PathBuf::from("out.csv")
    );
}

#[test]
fn write_path_unrestricted_passthrough() {
    let c = sandbox_ctx(false, &[], None);
    assert_eq!(
        c.resolve_write_path(Path::new("rel.csv")).unwrap(),
        PathBuf::from("rel.csv")
    );
}

/// Settings holding one saved S3 connection for `bucket`, with the given
/// per-connection write permission.
fn settings_with_bucket(allow_writes: bool) -> octa::ui::settings::AppSettings {
    let mut conn = octa::cloud::CloudConnection::ephemeral_s3("bucket");
    conn.id = "test-conn".into();
    conn.name = "Test".into();
    conn.allow_writes = allow_writes;
    octa::ui::settings::AppSettings {
        cloud_connections: vec![conn],
        ..Default::default()
    }
}

#[test]
fn cloud_write_chat_needs_the_connection_to_allow_writes() {
    // Writing is permitted per connection and nowhere else: there is no global
    // cloud-writes switch to also satisfy. A saved connection with the box
    // unticked is refused before any provider is built...
    let mut c = sandbox_ctx(true, &[], None);
    c.cloud_settings = Some(settings_with_bucket(false));
    let err = match c.resolve_write_dest(Path::new("s3://bucket/out.parquet")) {
        Ok(_) => panic!("expected a refusal while the connection disallows writes"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("does not allow writes"), "{err}");
    assert!(err.contains("Test"), "names the connection: {err}");

    // ...and ticking it is the only thing needed to allow it.
    c.cloud_settings = Some(settings_with_bucket(true));
    assert!(
        c.resolve_write_dest(Path::new("s3://bucket/out.parquet"))
            .is_ok(),
        "an allow_writes connection is sufficient on its own"
    );
}

#[test]
fn cloud_write_mcp_uses_ambient() {
    // MCP (no settings) is trusted, like its local writes: a cloud URL resolves
    // to a temp file + provider with ambient creds (no network until finish()).
    let c = sandbox_ctx(false, &[], None);
    let dest = c
        .resolve_write_dest(Path::new("s3://bucket/out.parquet"))
        .expect("mcp cloud write should resolve");
    assert!(dest.is_cloud());
    assert_eq!(
        dest.path().extension().and_then(|e| e.to_str()),
        Some("parquet")
    );
}

#[test]
fn cell_from_json_coerces_by_type() {
    assert_eq!(cell_from_json(&Value::Null, "Int64"), CellValue::Null);
    assert_eq!(
        cell_from_json(&json!(true), "Boolean"),
        CellValue::Bool(true)
    );
    assert_eq!(cell_from_json(&json!(42), "Int64"), CellValue::Int(42));
    // Integer JSON into a float column promotes to Float.
    assert_eq!(
        cell_from_json(&json!(42), "Float64"),
        CellValue::Float(42.0)
    );
    assert_eq!(
        cell_from_json(&json!(1.5), "Float64"),
        CellValue::Float(1.5)
    );
    // Float JSON into an int column cannot be an int -> Float.
    assert_eq!(cell_from_json(&json!(1.5), "Int64"), CellValue::Float(1.5));
    assert_eq!(
        cell_from_json(&json!("hi"), "Utf8"),
        CellValue::String("hi".to_string())
    );
    assert_eq!(
        cell_from_json(&json!("2024-01-02"), "Date32"),
        CellValue::Date("2024-01-02".to_string())
    );
    assert_eq!(
        cell_from_json(
            &json!("2024-01-02T03:04:05"),
            "Timestamp(Microsecond, None)"
        ),
        CellValue::DateTime("2024-01-02T03:04:05".to_string())
    );
}

#[test]
fn cell_from_json_binary_hex_roundtrip() {
    assert_eq!(
        cell_from_json(&json!("00ff10"), "Binary"),
        CellValue::Binary(vec![0x00, 0xff, 0x10])
    );
    // Non-hex falls back to a plain string rather than erroring.
    assert_eq!(
        cell_from_json(&json!("zzz"), "Binary"),
        CellValue::String("zzz".to_string())
    );
}

#[test]
fn cell_from_json_nested_for_containers() {
    assert_eq!(
        cell_from_json(&json!([1, 2]), "Utf8"),
        CellValue::Nested("[1,2]".to_string())
    );
}

#[test]
fn build_data_table_validates_arity() {
    let cols = vec![
        ("id".to_string(), "Int64".to_string()),
        ("name".to_string(), "Utf8".to_string()),
    ];
    let rows = vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]];
    let t = build_data_table(&cols, &rows).unwrap();
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.col_count(), 2);
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("b".to_string())));

    // Wrong arity is rejected.
    let bad = vec![vec![json!(1)]];
    assert!(build_data_table(&cols, &bad).is_err());
    // Empty columns rejected.
    assert!(build_data_table(&[], &[]).is_err());
}
