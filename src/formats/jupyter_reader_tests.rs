//! Unit tests for [`jupyter_reader`](jupyter_reader). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use std::io::Write;

const NB: &str = r##"{
 "nbformat": 4,
 "nbformat_minor": 5,
 "metadata": {"kernelspec": {"name": "python3", "display_name": "Python 3", "language": "python"}},
 "cells": [
  {"cell_type": "code", "execution_count": 7, "metadata": {"tags": ["keepme"]},
   "source": ["print('hi')\n"],
   "outputs": [{"output_type": "stream", "name": "stdout", "text": ["hi\n"]}]},
  {"cell_type": "markdown", "metadata": {}, "source": ["# Title\n"]}
 ]
}"##;

fn write_temp(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".ipynb")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn edit_preserves_outputs_and_metadata() {
    let src = write_temp(NB);
    let reader = JupyterReader;
    let mut table = reader.read_file(src.path()).unwrap();

    // Edit cell 0's source via the normal overlay.
    table.set(0, 2, CellValue::String("print('bye')\n".to_string()));

    let out = tempfile::Builder::new()
        .suffix(".ipynb")
        .tempfile()
        .unwrap();
    write_notebook(out.path(), &table).unwrap();

    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(out.path()).unwrap()).unwrap();
    let cells = written["cells"].as_array().unwrap();

    // Edited source landed.
    assert_eq!(cells[0]["source"][0].as_str().unwrap(), "print('bye')\n");
    // ...but the output, execution_count and per-cell metadata survived.
    assert_eq!(cells[0]["outputs"][0]["text"][0].as_str().unwrap(), "hi\n");
    assert_eq!(cells[0]["execution_count"].as_i64().unwrap(), 7);
    assert_eq!(cells[0]["metadata"]["tags"][0].as_str().unwrap(), "keepme");
    // Top-level metadata preserved (the original kernelspec, not our default).
    assert_eq!(
        written["metadata"]["kernelspec"]["name"].as_str().unwrap(),
        "python3"
    );
}

#[test]
fn type_flip_to_markdown_drops_code_keys() {
    let src = write_temp(NB);
    let table = JupyterReader.read_file(src.path()).unwrap();

    // No edits: code cell stays code with its output.
    let out = tempfile::Builder::new()
        .suffix(".ipynb")
        .tempfile()
        .unwrap();
    write_notebook(out.path(), &table).unwrap();
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(out.path()).unwrap()).unwrap();
    let cells = written["cells"].as_array().unwrap();
    assert!(cells[0].get("outputs").is_some());
    // Markdown cell carries no code-only keys.
    assert!(cells[1].get("outputs").is_none());
    assert!(cells[1].get("execution_count").is_none());
}

#[test]
fn no_source_path_emits_fresh_notebook() {
    // A table not backed by a file falls through to the fresh path.
    let mut table = DataTable::empty();
    table.columns = vec![
        ColumnInfo {
            name: "Cell".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "Type".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "Source".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "Output".into(),
            data_type: "Utf8".into(),
        },
    ];
    table.rows = vec![vec![
        CellValue::Int(1),
        CellValue::String("code".into()),
        CellValue::String("x = 1".into()),
        CellValue::String(String::new()),
    ]];
    let out = tempfile::Builder::new()
        .suffix(".ipynb")
        .tempfile()
        .unwrap();
    write_notebook(out.path(), &table).unwrap();
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(out.path()).unwrap()).unwrap();
    assert_eq!(written["cells"][0]["source"][0].as_str().unwrap(), "x = 1");
    assert_eq!(written["nbformat"].as_i64().unwrap(), 4);
}
