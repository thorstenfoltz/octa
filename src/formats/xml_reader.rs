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

/// One element of a parsed document. Also read by the Outlook for Mac
/// reader (`mail::olm_reader`), which walks the same tree.
#[derive(Clone)]
pub(crate) struct XmlElement {
    pub(crate) name: String,
    pub(crate) attributes: Vec<(String, String)>,
    pub(crate) text: String,
    pub(crate) children: Vec<XmlElement>,
}

fn parse_xml_to_table(content: &str, path: &Path) -> Result<DataTable> {
    let elements = parse_tree(content)?;

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

/// The attributes of a start tag, values unescaped (`&amp;` is `&`).
fn attributes(e: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    e.attributes()
        .filter_map(|a| a.ok())
        .map(|a| {
            let key = a.key.as_ref().to_string();
            let val = a
                // XML's five entities by name, never `resolve_predefined_entity`:
                // another crate switches quick-xml's `escape-html` feature on
                // for the whole build, and `&nbsp;` would turn into a space.
                .normalized_value_with(
                    quick_xml::XmlVersion::Implicit1_0,
                    1,
                    quick_xml::escape::resolve_xml_entity,
                )
                .map(|v| v.into_owned())
                .unwrap_or_else(|_| a.value.to_string());
            (key, val)
        })
        .collect()
}

/// Close an element: its text trimmed once, now that all of it is in.
/// (Trimming each piece instead lost the spaces around `&amp;`.)
fn finish(mut elem: XmlElement, stack: &mut [XmlElement], elements: &mut Vec<XmlElement>) {
    let trimmed = elem.text.trim();
    if trimmed.len() != elem.text.len() {
        elem.text = trimmed.to_string();
    }
    match stack.last_mut() {
        Some(parent) => parent.children.push(elem),
        None => elements.push(elem),
    }
}

/// The document as a tree of elements: the top-level ones, each with its
/// attributes, text and children.
pub(crate) fn parse_tree(content: &str) -> Result<Vec<XmlElement>> {
    let mut reader = XmlFileReader::from_str(content);

    let mut elements: Vec<XmlElement> = Vec::new();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => stack.push(XmlElement {
                name: e.name().as_ref().to_string(),
                attributes: attributes(&e),
                text: String::new(),
                children: Vec::new(),
            }),
            Ok(Event::Text(e)) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&e);
                }
            }
            // quick-xml hands every `&...;` over as an event of its own, so
            // `Tom &amp; Jerry` arrives in three pieces and the middle one
            // has to be put back: a character reference, one of XML's five
            // entities, or (undeclared here) the reference as written.
            Ok(Event::GeneralRef(e)) => {
                if let Some(current) = stack.last_mut() {
                    match e.resolve_char_ref() {
                        Ok(Some(c)) => current.text.push(c),
                        _ => {
                            let name: &str = &e;
                            match quick_xml::escape::resolve_xml_entity(name) {
                                Some(v) => current.text.push_str(v),
                                None => current.text.push_str(&format!("&{name};")),
                            }
                        }
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&e);
                }
            }
            Ok(Event::End(_)) => {
                if let Some(elem) = stack.pop() {
                    finish(elem, &mut stack, &mut elements);
                }
            }
            Ok(Event::Empty(e)) => {
                let elem = XmlElement {
                    name: e.name().as_ref().to_string(),
                    attributes: attributes(&e),
                    text: String::new(),
                    children: Vec::new(),
                };
                finish(elem, &mut stack, &mut elements);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }
    Ok(elements)
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
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("XML".to_string()),
        ..DataTable::empty()
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

#[cfg(test)]
#[path = "xml_reader_tests.rs"]
mod tests;
