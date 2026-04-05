use octo::data::{CellValue, ColumnInfo, DataTable};
use octo::formats::markdown_reader::MarkdownReader;
use octo::formats::FormatReader;
use std::io::Write;

#[test]
fn test_read_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.md");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# Heading").unwrap();
        writeln!(f).unwrap();
        write!(f, "Some **bold** text").unwrap();
    }
    let table = MarkdownReader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.format_name, Some("Markdown".to_string()));
    assert_eq!(
        table.get(0, 0),
        Some(&CellValue::String("# Heading".to_string()))
    );
    assert_eq!(
        table.get(2, 0),
        Some(&CellValue::String("Some **bold** text".to_string()))
    );
}

#[test]
fn test_write_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.md");

    let mut table = DataTable::empty();
    table.columns = vec![ColumnInfo {
        name: "Line".to_string(),
        data_type: "Utf8".to_string(),
    }];
    table.rows = vec![
        vec![CellValue::String("# Title".to_string())],
        vec![CellValue::String("paragraph".to_string())],
    ];

    MarkdownReader.write_file(&path, &table).unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "# Title\nparagraph");
}

#[test]
fn test_markdown_reader_metadata() {
    assert_eq!(MarkdownReader.name(), "Markdown");
    assert!(MarkdownReader.extensions().contains(&"md"));
    assert!(MarkdownReader.extensions().contains(&"markdown"));
    assert!(MarkdownReader.extensions().contains(&"mdown"));
    assert!(MarkdownReader.extensions().contains(&"mkd"));
    assert!(MarkdownReader.supports_write());
}

#[test]
fn test_markdown_preserves_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fmt.md");
    let content = "# Title\n\n- item 1\n- item 2\n\n```rust\nfn main() {}\n```\n\n> blockquote";
    std::fs::write(&path, content).unwrap();

    let table = MarkdownReader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 10);
    assert_eq!(
        table.get(0, 0),
        Some(&CellValue::String("# Title".to_string()))
    );
    assert_eq!(
        table.get(2, 0),
        Some(&CellValue::String("- item 1".to_string()))
    );
    assert_eq!(
        table.get(5, 0),
        Some(&CellValue::String("```rust".to_string()))
    );
    assert_eq!(
        table.get(9, 0),
        Some(&CellValue::String("> blockquote".to_string()))
    );
}

#[test]
fn test_markdown_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt.md");

    let mut table = DataTable::empty();
    table.columns = vec![ColumnInfo {
        name: "Line".to_string(),
        data_type: "Utf8".to_string(),
    }];
    table.rows = vec![
        vec![CellValue::String("## Section".to_string())],
        vec![CellValue::String("".to_string())],
        vec![CellValue::String("Content here.".to_string())],
    ];

    MarkdownReader.write_file(&path, &table).unwrap();
    let loaded = MarkdownReader.read_file(&path).unwrap();
    assert_eq!(loaded.row_count(), 3);
    assert_eq!(
        loaded.get(0, 0),
        Some(&CellValue::String("## Section".to_string()))
    );
    assert_eq!(loaded.get(1, 0), Some(&CellValue::String("".to_string())));
    assert_eq!(
        loaded.get(2, 0),
        Some(&CellValue::String("Content here.".to_string()))
    );
}

#[test]
fn test_markdown_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.md");
    std::fs::write(&path, "").unwrap();

    let table = MarkdownReader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 0);
    assert_eq!(table.col_count(), 1);
    assert_eq!(table.format_name, Some("Markdown".to_string()));
}
