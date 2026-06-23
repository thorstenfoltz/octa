//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

fn sandbox_ctx(restrict: bool, allowed: &[&str], export: Option<&str>) -> ToolContext {
    ToolContext {
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
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true);
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
fn write_path_unrestricted_passthrough() {
    let c = sandbox_ctx(false, &[], None);
    assert_eq!(
        c.resolve_write_path(Path::new("rel.csv")).unwrap(),
        PathBuf::from("rel.csv")
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
