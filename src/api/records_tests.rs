use super::*;
use serde_json::json;

fn envelope() -> Value {
    json!({
        "meta": {"page": 1, "tags": ["a", "b"]},
        "results": [{"id": 1}, {"id": 2}],
        "included": [{"id": 9}]
    })
}

#[test]
fn an_empty_pointer_means_the_whole_value() {
    let v = envelope();
    assert_eq!(records_at(&v, "").unwrap(), v);
    assert_eq!(records_at(&v, "   ").unwrap(), v);
}

#[test]
fn a_pointer_picks_the_named_array() {
    let v = envelope();
    let r = records_at(&v, "/results").unwrap();
    assert_eq!(row_count(&r), 2);
    // A leading slash is pointer syntax, not something to make the user learn.
    assert_eq!(records_at(&v, "results").unwrap(), r);
    assert_eq!(records_at(&v, "/included").map(|v| row_count(&v)), Some(1));
    assert!(records_at(&v, "/missing").is_none());
}

/// The reason a pointer exists at all: the automatic guess takes whichever
/// array of objects comes first, and both of these are arrays of objects.
#[test]
fn candidates_list_every_array_of_objects() {
    let c = candidate_record_paths(&envelope());
    assert!(c.contains(&"/results".to_string()), "{c:?}");
    assert!(c.contains(&"/included".to_string()), "{c:?}");
    assert!(
        !c.contains(&"/meta/tags".to_string()),
        "an array of primitives is not a record list: {c:?}"
    );
}

#[test]
fn a_top_level_array_is_its_own_candidate() {
    let v = json!([{"id": 1}]);
    assert_eq!(candidate_record_paths(&v), vec!["".to_string()]);
}

#[test]
fn candidate_paths_escape_pointer_syntax() {
    let v = json!({"a/b": [{"id": 1}]});
    let c = candidate_record_paths(&v);
    assert_eq!(c, vec!["/a~1b".to_string()]);
    assert!(records_at(&v, &c[0]).is_some(), "the escaped path resolves");
}

#[test]
fn extend_folds_arrays_and_single_objects_alike() {
    let mut acc = Vec::new();
    extend(&mut acc, json!([{"id": 1}, {"id": 2}]));
    extend(&mut acc, json!({"id": 3}));
    extend(&mut acc, Value::Null);
    assert_eq!(acc.len(), 3);
}
