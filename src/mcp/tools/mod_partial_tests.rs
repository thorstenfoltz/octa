//! An answer computed from part of a file has to say so, whichever tool gave it.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::*;

fn ctx() -> ToolContext {
    ToolContext::for_mcp(Some(1000), 65536, true, false, vec![], false, 0)
}

fn long_csv(dir: &Path) -> PathBuf {
    let path = dir.join("long.csv");
    std::fs::write(&path, "id,name\n1,a\n2,b\n3,c\n4,d\n5,e\n").unwrap();
    path
}

fn params<P: serde::de::DeserializeOwned>(value: Value) -> P {
    serde_json::from_value(value).unwrap()
}

#[test]
fn a_tool_that_read_part_of_a_file_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let f = long_csv(dir.path());
    let _cap = octa::formats::ThreadRowCap::new(3);
    let answer =
        say_if_partial(|| outliers::run(&ctx(), &params(json!({"path": f, "columns": ["id"]}))))
            .unwrap();
    assert_eq!(answer["initial_load_capped"], json!(true), "{answer}");
    let note = answer["initial_load_note"].as_str().unwrap_or_default();
    assert!(note.contains("long.csv"), "{note}");
    assert!(note.contains('3'), "{note}");
    assert!(note.contains("unlimited"), "{note}");
}

#[test]
fn a_tool_that_read_everything_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let f = long_csv(dir.path());
    let answer =
        say_if_partial(|| outliers::run(&ctx(), &params(json!({"path": f, "columns": ["id"]}))))
            .unwrap();
    assert!(answer.get("initial_load_note").is_none(), "{answer}");
}

/// Worker threads are reused, and a note must not reach the next call on one.
#[test]
fn a_note_does_not_outlive_its_call() {
    let dir = tempfile::tempdir().unwrap();
    let f = long_csv(dir.path());
    {
        let _cap = octa::formats::ThreadRowCap::new(3);
        read_with_registry(&f, None).unwrap();
    }
    let answer = say_if_partial(|| Ok::<_, anyhow::Error>(json!({"a": 1}))).unwrap();
    assert!(answer.get("initial_load_note").is_none(), "{answer}");
}

/// The columns are the same however many rows were read.
#[test]
fn a_tool_about_the_columns_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let f = long_csv(dir.path());
    let _cap = octa::formats::ThreadRowCap::new(3);
    let answer = say_if_partial(|| schema::run(&ctx(), &params(json!({"path": f})))).unwrap();
    assert!(answer.get("initial_load_note").is_none(), "{answer}");
}

/// A new tool copies the handler of the last one, so this is where it gets missed.
#[test]
fn every_handler_goes_through_the_note() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mcp/tools");
    let bare = "spawn_blocking(move || run(&ctx, &p))";
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".rs") && !name.ends_with("_tests.rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(!text.contains(bare), "{}", path.display());
        }
    }
}
