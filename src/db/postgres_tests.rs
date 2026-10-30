use super::*;

#[test]
fn redshift_uses_svv_catalog() {
    assert!(list_tables_sql(PgDialect::Redshift, "public").contains("svv_redshift_tables"));
    assert!(list_tables_sql(PgDialect::Postgres, "public").contains("information_schema"));
    assert!(list_schemas_sql(PgDialect::Redshift).contains("svv_redshift_schemas"));
}

#[test]
fn table_schema_literal_is_escaped() {
    assert!(list_tables_sql(PgDialect::Postgres, "a'b").contains("'a''b'"));
}

#[test]
fn native_types_read_into_the_same_cells_as_before() {
    assert_eq!(text_to_cell(&Type::INT4, Some("-42")), CellValue::Int(-42));
    assert_eq!(
        text_to_cell(&Type::FLOAT8, Some("1.5")),
        CellValue::Float(1.5)
    );
    assert_eq!(text_to_cell(&Type::BOOL, Some("t")), CellValue::Bool(true));
    assert_eq!(
        text_to_cell(&Type::DATE, Some("2024-01-31")),
        CellValue::Date("2024-01-31".into())
    );
    assert_eq!(
        text_to_cell(&Type::TIMESTAMP, Some("2024-01-31 12:34:56.789")),
        CellValue::DateTime("2024-01-31 12:34:56.789".into())
    );
    assert_eq!(
        text_to_cell(&Type::TIMESTAMP, Some("2024-01-31 12:34:56")),
        CellValue::DateTime("2024-01-31 12:34:56".into())
    );
    // Printed in the session's zone, shown in UTC: hour-only and half-hour
    // offsets both occur in the wild.
    assert_eq!(
        text_to_cell(&Type::TIMESTAMPTZ, Some("2024-01-31 12:00:00+01")),
        CellValue::DateTime("2024-01-31 11:00:00".into())
    );
    assert_eq!(
        text_to_cell(&Type::TIMESTAMPTZ, Some("2024-01-31 12:00:00.5+05:30")),
        CellValue::DateTime("2024-01-31 06:30:00.5".into())
    );
    assert_eq!(
        text_to_cell(&Type::NUMERIC, Some("99.50")),
        CellValue::String("99.50".into())
    );
    assert_eq!(text_to_cell(&Type::INT8, None), CellValue::Null);
}

#[test]
fn every_other_type_keeps_the_text_postgres_printed() {
    assert_eq!(
        text_to_cell(&Type::UUID, Some("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11")),
        CellValue::String("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11".into())
    );
    assert_eq!(
        text_to_cell(&Type::JSONB, Some("{\"a\": 1}")),
        CellValue::String("{\"a\": 1}".into())
    );
    assert_eq!(
        text_to_cell(&Type::BYTEA, Some("\\xdead00")),
        CellValue::Binary(vec![0xde, 0xad, 0x00])
    );
    // Values the typed parse cannot read are kept, never turned into NULL.
    assert_eq!(
        text_to_cell(&Type::DATE, Some("infinity")),
        CellValue::String("infinity".into())
    );
    assert_eq!(
        text_to_cell(&Type::DATE, Some("0044-03-15 BC")),
        CellValue::String("0044-03-15 BC".into())
    );
    assert_eq!(
        text_to_cell(&Type::BYTEA, Some("\\336\\255")),
        CellValue::String("\\336\\255".into())
    );
}

#[test]
fn time_and_interval_text_read_typed() {
    let h = 3_600_000_000_000i64;
    assert_eq!(
        text_to_cell(&Type::TIME, Some("14:30:00.25")),
        CellValue::Time(14 * h + h / 2 + 250_000_000)
    );
    // Stored as whole days plus under a day, one sign: what a save writes.
    for (text, ns) in [
        ("03:00:00", 3 * h),
        ("-00:30:00", -h / 2),
        ("1 day", 24 * h),
        ("2 days 03:00:00", 51 * h),
        ("-2 days -03:00:00", -51 * h),
    ] {
        assert_eq!(
            text_to_cell(&Type::INTERVAL, Some(text)),
            CellValue::Duration(ns),
            "{text}"
        );
    }
    // Stored any other way, Postgres keeps its days apart (they differ across
    // a clock change), so the text stays and a save cannot rewrite it.
    for text in [
        "51:00:00",
        "-1 days +02:00:00",
        "1 day 25:00:00",
        "1 mon 2 days",
    ] {
        assert_eq!(
            text_to_cell(&Type::INTERVAL, Some(text)),
            CellValue::String(text.into()),
            "{text}"
        );
    }
    assert_eq!(pg_type_to_arrow(&Type::TIME), "Time64(µs)");
    assert_eq!(pg_type_to_arrow(&Type::INTERVAL), "Duration(µs)");
    assert_eq!(pg_type_to_arrow(&Type::TIMETZ), "Utf8");
}
