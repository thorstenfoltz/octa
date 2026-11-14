//! DuckDB's nested values (`LIST`, `ARRAY`, `STRUCT`, `MAP`, `UNION`, `ENUM`) as
//! text, in DuckDB's own notation.
//!
//! The driver hands these over as a whole Arrow batch plus a row number, and
//! printing that with `{:?}` put a dump of every row into each cell. The text has
//! to be **readable and castable**: a live connection tab saves whole rows, so
//! this text goes back through `CAST(.. AS <type>)`, and Arrow's own display does
//! not survive that. It leaves strings unquoted, so `['r,s']` would come back as
//! two elements with no error, and it writes a map's `=` as `:`.
//!
//! `duckdb_nested_tests.rs` casts the text back for every shape and type.

use arrow::array::{Array, AsArray};
use arrow::datatypes::{DataType, Decimal128Type};
use arrow::util::display::{ArrayFormatter, FormatOptions};
use duckdb::types::{EnumType, ListType, ValueRef};

/// `v` as text, when it is one of the values the driver gives as an array.
pub(crate) fn nested_text(v: &ValueRef<'_>) -> Option<String> {
    let (array, row): (&dyn Array, usize) = match v {
        ValueRef::List(ListType::Regular(a), row) => (*a, *row),
        ValueRef::List(ListType::Large(a), row) => (*a, *row),
        ValueRef::Enum(EnumType::UInt8(a), row) => (*a, *row),
        ValueRef::Enum(EnumType::UInt16(a), row) => (*a, *row),
        ValueRef::Enum(EnumType::UInt32(a), row) => (*a, *row),
        ValueRef::Struct(a, row) => (*a, *row),
        ValueRef::Array(a, row) => (*a, *row),
        ValueRef::Map(a, row) => (*a, *row),
        ValueRef::Union(a, row) => (a.as_ref(), *row),
        _ => return None,
    };
    let mut out = String::new();
    push_value(&mut out, array, row, false);
    Some(out)
}

/// Whether `text` has to be quoted inside a nested value, by the rule DuckDB's own
/// cast to text follows: a bare `NULL` would read back as null, a bare empty string
/// as nothing, and the rest would split or end the value.
fn needs_quotes(text: &str) -> bool {
    text.is_empty()
        || text.eq_ignore_ascii_case("null")
        || text.trim() != text
        || text.contains(|c: char| "()[]{}'\",:=\\".contains(c))
}

/// `text` inside `'..'`, with a backslash before `'` and `\`.
fn push_quoted(out: &mut String, text: &str) {
    out.push('\'');
    for c in text.chars() {
        if matches!(c, '\'' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
}

fn push_text(out: &mut String, text: &str, inside: bool) {
    if inside && needs_quotes(text) {
        push_quoted(out, text);
    } else {
        out.push_str(text);
    }
}

fn push_bytes(out: &mut String, bytes: &[u8], inside: bool) {
    let hex: String = bytes.iter().map(|b| format!("\\x{b:02X}")).collect();
    push_text(out, &hex, inside);
}

fn push_items(out: &mut String, items: &dyn Array) {
    out.push('[');
    for i in 0..items.len() {
        if i > 0 {
            out.push_str(", ");
        }
        push_value(out, items, i, true);
    }
    out.push(']');
}

/// One value of `array`. `inside` says it sits in a nested value, where text is
/// quoted; a top-level enum or union is the plain value.
fn push_value(out: &mut String, array: &dyn Array, row: usize, inside: bool) {
    if array.is_null(row) {
        out.push_str("NULL");
        return;
    }
    match array.data_type() {
        DataType::List(_) => push_items(out, &array.as_list::<i32>().value(row)),
        DataType::LargeList(_) => push_items(out, &array.as_list::<i64>().value(row)),
        DataType::FixedSizeList(..) => push_items(out, &array.as_fixed_size_list().value(row)),
        DataType::Struct(fields) => {
            out.push('{');
            for (i, (field, column)) in fields.iter().zip(array.as_struct().columns()).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_quoted(out, field.name());
                out.push_str(": ");
                push_value(out, column, row, true);
            }
            out.push('}');
        }
        DataType::Map(..) => {
            let entries = array.as_map().value(row);
            out.push('{');
            for i in 0..entries.len() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_value(out, entries.column(0), i, true);
                out.push('=');
                push_value(out, entries.column(1), i, true);
            }
            out.push('}');
        }
        DataType::Union(..) => {
            let union = array.as_union();
            let child = union.child(union.type_id(row));
            push_value(out, child, union.value_offset(row), inside);
        }
        DataType::Dictionary(..) => {
            let dictionary = array.as_any_dictionary();
            let key = dictionary.normalized_keys()[row];
            push_value(out, dictionary.values(), key, inside);
        }
        DataType::Utf8 => push_text(out, array.as_string::<i32>().value(row), inside),
        DataType::LargeUtf8 => push_text(out, array.as_string::<i64>().value(row), inside),
        DataType::Utf8View => push_text(out, array.as_string_view().value(row), inside),
        DataType::Binary => push_bytes(out, array.as_binary::<i32>().value(row), inside),
        DataType::LargeBinary => push_bytes(out, array.as_binary::<i64>().value(row), inside),
        DataType::BinaryView => push_bytes(out, array.as_binary_view().value(row), inside),
        // Not Arrow's display, which cuts to the stated precision: DuckDB hands a
        // 39-digit HUGEINT over as Decimal128(38, 0), and it lost its last digit.
        DataType::Decimal128(_, scale) => out.push_str(&crate::data::decimal::decimal_text(
            array.as_primitive::<Decimal128Type>().value(row),
            *scale,
        )),
        _ => {
            let options = FormatOptions::new()
                .with_timestamp_format(Some("%Y-%m-%d %H:%M:%S%.f"))
                .with_timestamp_tz_format(Some("%Y-%m-%d %H:%M:%S%.f%:z"));
            // Never `NULL` on a failure, which would change the value silently: this
            // text is visibly wrong, and a save of it fails its cast.
            let text = ArrayFormatter::try_new(array, &options)
                .and_then(|formatter| formatter.value(row).try_to_string());
            match text {
                Ok(text) => out.push_str(&text),
                Err(_) => out.push_str(&format!("{:?}", array.slice(row, 1))),
            }
        }
    }
}

#[cfg(test)]
#[path = "duckdb_nested_tests.rs"]
mod tests;
