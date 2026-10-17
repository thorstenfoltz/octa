use super::*;

#[test]
fn pivot_with_group() {
    let sql = pivot_sql(
        "month",
        PivotAgg::Sum,
        "revenue",
        &["region".to_string(), "team".to_string()],
    );
    assert_eq!(
        sql,
        r#"PIVOT data ON "month" USING sum("revenue") GROUP BY "region", "team""#
    );
}

#[test]
fn pivot_no_group() {
    let sql = pivot_sql("m", PivotAgg::Count, "v", &[]);
    assert_eq!(sql, r#"PIVOT data ON "m" USING count("v")"#);
}

#[test]
fn unpivot_needs_two() {
    assert!(unpivot_sql(&["a".to_string()], "name", "value").is_none());
    let sql = unpivot_sql(&["q1".to_string(), "q2".to_string()], "quarter", "amount").unwrap();
    assert_eq!(
        sql,
        r#"UNPIVOT data ON "q1", "q2" INTO NAME "quarter" VALUE "amount""#
    );
}

#[test]
fn agg_parse() {
    assert_eq!(PivotAgg::parse("SUM"), Some(PivotAgg::Sum));
    assert_eq!(PivotAgg::parse("mean"), Some(PivotAgg::Avg));
    assert_eq!(PivotAgg::parse("nope"), None);
}

#[test]
fn idents_quoted() {
    assert_eq!(quote_ident(r#"a"b"#), r#""a""b""#);
}

#[test]
fn explain_mentions_columns() {
    // Locale-independent parts: the chosen column names appear in the text.
    let s = explain_pivot("month", PivotAgg::Sum, "revenue", &["region".to_string()]);
    assert!(s.contains("month"), "{s}");
    assert!(s.contains("revenue"), "{s}");
    assert!(s.contains("region"), "{s}");

    let u = explain_unpivot(&["q1".to_string(), "q2".to_string()], "quarter", "amount");
    assert!(u.contains("q1") && u.contains("q2"), "{u}");
    assert!(u.contains("quarter") && u.contains("amount"), "{u}");
}
