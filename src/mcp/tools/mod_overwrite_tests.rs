//! A tool that writes back over the file it read must have read all of it. Written
//! back, a capped read shortens the file and still reports success.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::*;

fn ctx() -> ToolContext {
    ToolContext {
        allow_existing_writes: true,
        ..ToolContext::for_mcp(Some(1000), 65536, true, false, vec![], false, 0)
    }
}

/// Five rows, against a cap of three that the caller holds.
fn long_csv(dir: &Path) -> PathBuf {
    let path = dir.join("long.csv");
    std::fs::write(&path, "id,name\n1,a\n2,b\n3,c\n4,d\n5,e\n").unwrap();
    path
}

fn params<P: serde::de::DeserializeOwned>(value: Value) -> P {
    serde_json::from_value(value).unwrap()
}

type Overwrite = fn(&ToolContext, &Path) -> anyhow::Result<Value>;

/// Every tool that can write over its own input.
fn overwriting_tools() -> Vec<(&'static str, Overwrite)> {
    vec![
        ("edit_table", |ctx, f| {
            edit_table::run(
                ctx,
                &params(json!({"path": f, "set": [{"row": 0, "col": "name", "value": "z"}]})),
            )
        }),
        ("transform_columns", |ctx, f| {
            transform_columns::run(
                ctx,
                &params(json!({"path": f, "rename": [{"from": "name", "to": "n"}]})),
            )
        }),
        ("anonymize", |ctx, f| {
            anonymize::run(
                ctx,
                &params(json!({"path": f, "rules": [
                    {"columns": "name", "strategy": {"type": "hash", "algo": "sha256"}}
                ]})),
            )
        }),
        ("convert", |ctx, f| {
            convert::run(ctx, &params(json!({"input": f, "output": f})))
        }),
        ("write_table append", |ctx, f| {
            write_table::run(
                ctx,
                &params(json!({
                    "path": f,
                    "columns": [{"name": "id"}, {"name": "name"}],
                    "rows": [[6, "f"]],
                    "mode": "append",
                })),
            )
        }),
    ]
}

#[test]
fn no_tool_shortens_the_file_it_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = long_csv(dir.path());
    let before = std::fs::read(&path).unwrap();
    let _cap = octa::formats::ThreadRowCap::new(3);
    for (name, tool) in overwriting_tools() {
        let result = tool(&ctx(), &path);
        let err = format!(
            "{:#}",
            result.err().unwrap_or_else(|| panic!("{name} wrote"))
        );
        assert!(err.contains("unlimited"), "{name}: {err}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
    }
}

/// The same path by another spelling is the same file.
#[test]
fn a_relative_path_to_the_same_file_is_refused_too() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = long_csv(dir.path());
    let relative = path
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap()
        .to_path_buf();
    let before = std::fs::read(&path).unwrap();
    let _cap = octa::formats::ThreadRowCap::new(3);
    let p = params(json!({"input": path, "output": relative}));
    assert!(convert::run(&ctx(), &p).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// Refusing is about the input. A file read whole is edited as before, and writing
/// part of it somewhere else leaves the input alone.
#[test]
fn a_whole_file_and_another_target_are_still_written() {
    let dir = tempfile::tempdir().unwrap();
    for (name, tool) in overwriting_tools() {
        let path = long_csv(dir.path());
        tool(&ctx(), &path).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    }

    let path = long_csv(dir.path());
    let before = std::fs::read(&path).unwrap();
    let out = dir.path().join("elsewhere.csv");
    let _cap = octa::formats::ThreadRowCap::new(3);
    let answer = transform_columns::run(
        &ctx(),
        &params(json!({"path": path, "output_path": out, "drop": ["name"]})),
    )
    .unwrap();
    assert_eq!(answer["rows_written"], 3);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
