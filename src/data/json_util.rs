use std::collections::HashSet;

use serde_json::Value;

/// Collect paths of all container nodes (Object/Array) in a JSON value.
///
/// Uses the same path convention as the JSON tree view renderer:
/// - Object keys: `"key"`, `"parent.child"`
/// - Array indices: `"[0]"`, `"items[2]"`
///
/// `max_depth`: `None` = all depths, `Some(0)` = root only, `Some(1)` = root + direct children, etc.
pub fn collect_json_paths(value: &Value, max_depth: Option<usize>) -> HashSet<String> {
    let mut paths = HashSet::new();
    collect_paths_recursive(value, "", 0, max_depth, &mut paths);
    paths
}

fn collect_paths_recursive(
    value: &Value,
    path: &str,
    depth: usize,
    max_depth: Option<usize>,
    paths: &mut HashSet<String>,
) {
    match value {
        Value::Object(map) => {
            paths.insert(path.to_string());
            if max_depth.is_some_and(|max| depth >= max) {
                return;
            }
            for (k, v) in map {
                let child_path = if path.is_empty() {
                    k.to_string()
                } else {
                    format!("{path}.{k}")
                };
                collect_paths_recursive(v, &child_path, depth + 1, max_depth, paths);
            }
        }
        Value::Array(arr) => {
            paths.insert(path.to_string());
            if max_depth.is_some_and(|max| depth >= max) {
                return;
            }
            for (i, v) in arr.iter().enumerate() {
                let child_path = if path.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{path}[{i}]")
                };
                collect_paths_recursive(v, &child_path, depth + 1, max_depth, paths);
            }
        }
        _ => {}
    }
}

/// Parse a path string into segments.
/// Handles dot-separated keys and bracket-indexed arrays.
/// e.g. `"a.b[0].c"` -> `["a", "b", "[0]", "c"]`
fn parse_path_segments(path: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
                let mut idx = String::from('[');
                for c in chars.by_ref() {
                    idx.push(c);
                    if c == ']' {
                        break;
                    }
                }
                segments.push(idx);
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Set a value at a dot/bracket-separated path in a JSON value.
///
/// Returns `Ok(())` on success, `Err(description)` if the path is invalid.
pub fn set_json_value_at_path(
    root: &mut Value,
    path: &str,
    new_value: Value,
) -> Result<(), String> {
    if path.is_empty() {
        *root = new_value;
        return Ok(());
    }

    let segments = parse_path_segments(path);
    if segments.is_empty() {
        return Err("Empty path".to_string());
    }

    let mut current = root;
    for (i, seg) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;

        if seg.starts_with('[') && seg.ends_with(']') {
            // Array index
            let idx_str = &seg[1..seg.len() - 1];
            let idx: usize = idx_str
                .parse()
                .map_err(|_| format!("Invalid array index: {idx_str}"))?;
            let arr = current
                .as_array_mut()
                .ok_or_else(|| format!("Expected array at segment '{seg}'"))?;
            if idx >= arr.len() {
                return Err(format!(
                    "Array index {idx} out of bounds (len {})",
                    arr.len()
                ));
            }
            if is_last {
                arr[idx] = new_value;
                return Ok(());
            }
            current = &mut arr[idx];
        } else {
            // Object key
            let obj = current
                .as_object_mut()
                .ok_or_else(|| format!("Expected object at key '{seg}'"))?;
            if is_last {
                obj.insert(seg.clone(), new_value);
                return Ok(());
            }
            current = obj
                .get_mut(seg.as_str())
                .ok_or_else(|| format!("Key '{seg}' not found"))?;
        }
    }

    Err("Path traversal did not reach a leaf".to_string())
}

/// Compute the maximum nesting depth of a JSON value.
///
/// Leaf values (string, number, bool, null) have depth 0.
/// An object or array adds 1 level for each nesting step.
pub fn max_json_depth(value: &Value) -> usize {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                0
            } else {
                1 + map.values().map(max_json_depth).max().unwrap_or(0)
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                0
            } else {
                1 + arr.iter().map(max_json_depth).max().unwrap_or(0)
            }
        }
        _ => 0,
    }
}

/// Parse a user-edited string back into a `serde_json::Value`.
///
/// Tries in order: null, bool, integer, float, then falls back to string.
pub fn parse_json_edit(input: &str) -> Value {
    let trimmed = input.trim();
    if trimmed == "null" {
        return Value::Null;
    }
    if trimmed == "true" {
        return Value::Bool(true);
    }
    if trimmed == "false" {
        return Value::Bool(false);
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = trimmed.parse::<f64>()
        && let Some(num) = serde_json::Number::from_f64(n)
    {
        return Value::Number(num);
    }
    Value::String(input.to_string())
}
