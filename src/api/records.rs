//! Finding the rows inside a response envelope.
//!
//! `json_to_table` already guesses: it takes the first non-empty array of
//! objects it finds and drops the rest of the envelope. That is right for a
//! file and wrong for an API, where `{"meta": {...}, "results": [...]}` and
//! `{"data": [...], "included": [...]}` both occur and the guess picks
//! whichever key happens to come first. So a connection can name the array
//! with a JSON pointer, and the dialog offers the candidates it found.

use serde_json::Value;

/// How deep to look for candidate record arrays. An API envelope nests a
/// couple of levels; past that it is data, not structure.
const MAX_DEPTH: usize = 4;

/// The sub-value a pointer names, or the whole value when the pointer is
/// empty. `Value::pointer` is the house idiom for this (see `db/rest.rs`),
/// so no jsonpath dependency is involved.
pub fn records_at(v: &Value, pointer: &str) -> Option<Value> {
    let p = pointer.trim();
    if p.is_empty() {
        return Some(v.clone());
    }
    // Accept `data/items` as well as `/data/items`: the leading slash is a
    // detail of the pointer syntax, not something to make the user learn.
    let p = if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    };
    v.pointer(&p).cloned()
}

/// Every path in `v` that holds a non-empty array of objects, as JSON
/// pointers, outermost first. The dialog's records-array picker is built from
/// this, so the user chooses rather than types.
pub fn candidate_record_paths(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(v, String::new(), 0, &mut out);
    out
}

fn walk(v: &Value, path: String, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    match v {
        Value::Array(arr) => {
            if !arr.is_empty() && arr.iter().any(|e| matches!(e, Value::Object(_))) {
                out.push(path.clone());
            }
        }
        Value::Object(map) => {
            for (k, child) in map {
                // `/` and `~` are escaped in a JSON pointer token.
                let token = k.replace('~', "~0").replace('/', "~1");
                walk(child, format!("{path}/{token}"), depth + 1, out);
            }
        }
        _ => {}
    }
}

/// How many rows the pointed-at value would produce, for the "this page had N
/// rows" readout and for deciding a page was the last one.
pub fn row_count(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        Value::Null => 0,
        _ => 1,
    }
}

/// Merge the rows of `page` into `acc`, both being what `records_at` returned.
///
/// An endpoint that returns a bare object per page (one record) is as valid as
/// one returning an array, so both fold into the same accumulator.
pub fn extend(acc: &mut Vec<Value>, page: Value) {
    match page {
        Value::Array(a) => acc.extend(a),
        Value::Null => {}
        other => acc.push(other),
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
