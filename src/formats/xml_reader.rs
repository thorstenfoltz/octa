use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlFileReader;
use std::collections::HashSet;
use std::path::Path;

pub struct XmlFormatReader;

impl FormatReader for XmlFormatReader {
    fn name(&self) -> &str {
        "XML"
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        parse_xml_to_table(&content, path)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        use std::io::Write;
        let mut out = String::new();
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<data>\n");
        for row_idx in 0..table.row_count() {
            out.push_str("  <row");
            let mut child_elements = Vec::new();
            for (col_idx, col) in table.columns.iter().enumerate() {
                let val = table
                    .get(row_idx, col_idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                if val.is_empty() {
                    continue;
                }
                let escaped = quick_xml::escape::escape(&val);
                if col.name.starts_with('@') {
                    let attr_name = quick_xml::escape::escape(&col.name[1..]);
                    out.push_str(&format!(" {}=\"{}\"", attr_name, escaped));
                } else {
                    child_elements.push((col.name.clone(), escaped.into_owned()));
                }
            }
            if child_elements.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(">\n");
                for (name, val) in &child_elements {
                    let escaped_name = quick_xml::escape::escape(name);
                    out.push_str(&format!(
                        "    <{}>{}</{}>\n",
                        escaped_name, val, escaped_name
                    ));
                }
                out.push_str("  </row>\n");
            }
        }
        out.push_str("</data>\n");
        let mut file = std::fs::File::create(path)?;
        file.write_all(out.as_bytes())?;
        Ok(())
    }
}

#[derive(Clone)]
struct XmlElement {
    #[allow(dead_code)]
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<XmlElement>,
}

fn parse_xml_to_table(content: &str, path: &Path) -> Result<DataTable> {
    let mut reader = XmlFileReader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut elements: Vec<XmlElement> = Vec::new();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attrs: Vec<(String, String)> = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .map(|a| {
                        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&a.value).to_string();
                        (key, val)
                    })
                    .collect();
                stack.push(XmlElement {
                    name,
                    attributes: attrs,
                    text: String::new(),
                    children: Vec::new(),
                });
            }
            Ok(Event::Text(e)) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(_)) => {
                if let Some(elem) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(elem);
                    } else {
                        elements.push(elem);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attrs: Vec<(String, String)> = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .map(|a| {
                        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&a.value).to_string();
                        (key, val)
                    })
                    .collect();
                let elem = XmlElement {
                    name,
                    attributes: attrs,
                    text: String::new(),
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(elem);
                } else {
                    elements.push(elem);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    // Find repeating elements
    let root = if elements.len() == 1 {
        &elements[0]
    } else {
        return xml_elements_to_table(&elements, path);
    };

    if root.children.is_empty() {
        return xml_elements_to_table(std::slice::from_ref(root), path);
    }

    xml_elements_to_table(&root.children, path)
}

fn xml_elements_to_table(elements: &[XmlElement], path: &Path) -> Result<DataTable> {
    // Preserve original key order using Vec + HashSet for dedup
    let mut all_keys: Vec<String> = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut flat_rows: Vec<Vec<(String, String)>> = Vec::new();

    for elem in elements {
        let mut row: Vec<(String, String)> = Vec::new();

        // Add attributes
        for (key, val) in &elem.attributes {
            let col_name = format!("@{}", key);
            if seen_keys.insert(col_name.clone()) {
                all_keys.push(col_name.clone());
            }
            row.push((col_name, val.clone()));
        }

        // Add text content
        if !elem.text.trim().is_empty() {
            if seen_keys.insert("#text".to_string()) {
                all_keys.push("#text".to_string());
            }
            row.push(("#text".to_string(), elem.text.trim().to_string()));
        }

        // Add child elements
        for child in &elem.children {
            let key = child.name.clone();
            let value = if child.children.is_empty() && child.attributes.is_empty() {
                child.text.clone()
            } else {
                child_to_string(child)
            };
            if seen_keys.insert(key.clone()) {
                all_keys.push(key.clone());
            }
            row.push((key, value));
        }

        flat_rows.push(row);
    }

    let columns: Vec<ColumnInfo> = all_keys
        .iter()
        .map(|name| ColumnInfo {
            name: name.clone(),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for flat in &flat_rows {
        let row: Vec<CellValue> = all_keys
            .iter()
            .map(|key| {
                flat.iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| {
                        if v.is_empty() {
                            CellValue::Null
                        } else {
                            CellValue::String(v.clone())
                        }
                    })
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("XML".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
    })
}

fn child_to_string(elem: &XmlElement) -> String {
    let mut parts = Vec::new();
    for (k, v) in &elem.attributes {
        parts.push(format!("@{}={}", k, v));
    }
    if !elem.text.is_empty() {
        parts.push(elem.text.clone());
    }
    for child in &elem.children {
        parts.push(format!("{}: {}", child.name, child_to_string(child)));
    }
    parts.join(", ")
}
