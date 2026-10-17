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

/// A 2x1 PNG, base64 as nbformat stores it.
const PNG_2X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=";

fn png_bytes() -> Vec<u8> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, PNG_2X1).unwrap()
}

/// One code cell holding `outputs`, then a markdown cell.
fn nb_with_outputs(outputs: &str) -> String {
    format!(
        r##"{{"nbformat": 4, "nbformat_minor": 5, "metadata": {{}}, "cells": [
          {{"cell_type": "code", "execution_count": 1, "metadata": {{}}, "source": ["x"],
            "outputs": {outputs}}},
          {{"cell_type": "markdown", "metadata": {{}}, "source": ["# t"]}}]}}"##
    )
}

#[test]
fn a_picture_with_no_text_reads_as_a_placeholder() {
    let src = write_temp(&nb_with_outputs(&format!(
        r#"[{{"output_type": "display_data", "metadata": {{}}, "data": {{"image/png": "{PNG_2X1}"}}}}]"#
    )));
    let table = JupyterReader.read_file(src.path()).unwrap();
    assert_eq!(
        table.get(0, 3),
        Some(&CellValue::String("[image/png 2x1]".into()))
    );
}

#[test]
fn output_parts_keep_their_order_and_the_picture_wins_over_its_text() {
    let src = write_temp(&nb_with_outputs(&format!(
        r#"[{{"output_type": "stream", "name": "stdout", "text": ["a\n"]}},
            {{"output_type": "display_data", "metadata": {{}},
              "data": {{"image/png": "{PNG_2X1}", "text/plain": ["<Figure size 2x1>"]}}}},
            {{"output_type": "stream", "name": "stdout", "text": "b"}}]"#
    )));
    let parts = read_output_parts(src.path()).unwrap();
    assert_eq!(parts.len(), 1, "only the cell with a picture: {parts:?}");
    assert_eq!(
        parts[&1],
        vec![
            OutputPart::Text("a\n".into()),
            OutputPart::Picture {
                bytes: png_bytes(),
                alt: "<Figure size 2x1>".into()
            },
            OutputPart::Text("b".into()),
        ]
    );
    // The Output column reads exactly as before pictures were kept.
    let table = JupyterReader.read_file(src.path()).unwrap();
    assert_eq!(
        table.get(0, 3),
        Some(&CellValue::String("a\n\n<Figure size 2x1>\nb".into()))
    );
}

#[test]
fn line_broken_base64_and_svg_text_both_decode() {
    let (head, tail) = PNG_2X1.split_at(20);
    let src = write_temp(&nb_with_outputs(&format!(
        r#"[{{"output_type": "execute_result", "execution_count": 1, "metadata": {{}},
              "data": {{"image/png": ["{head}\n", "{tail}\n"]}}}},
            {{"output_type": "display_data", "metadata": {{}},
              "data": {{"image/svg+xml": ["<svg xmlns='http://www.w3.org/2000/svg'\n", " width='3' height='2'/>"]}}}}]"#
    )));
    let parts = read_output_parts(src.path()).unwrap();
    assert_eq!(
        parts[&1],
        vec![
            OutputPart::Picture {
                bytes: png_bytes(),
                alt: "[image/png 2x1]".into()
            },
            OutputPart::Picture {
                bytes: b"<svg xmlns='http://www.w3.org/2000/svg'\n width='3' height='2'/>".to_vec(),
                alt: "[image/svg+xml]".into()
            },
        ]
    );
}

#[test]
fn broken_base64_falls_back_to_the_text() {
    let src = write_temp(&nb_with_outputs(
        r#"[{"output_type": "display_data", "metadata": {},
             "data": {"image/png": "not base64!", "text/plain": "<Figure>"}}]"#,
    ));
    assert!(read_output_parts(src.path()).unwrap().is_empty());
    let table = JupyterReader.read_file(src.path()).unwrap();
    assert_eq!(table.get(0, 3), Some(&CellValue::String("<Figure>".into())));
}
