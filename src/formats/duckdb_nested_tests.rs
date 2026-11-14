//! Runs against the bundled DuckDB, so `cargo test` runs it on every CI platform.

use crate::formats::duckdb_reader::duckdb_value_to_cell_tz;
use duckdb::Connection;

fn conn() -> Connection {
    Connection::open_in_memory().unwrap()
}

/// The text Octa shows for `expr`, read the way every surface reads a value.
fn shown(conn: &Connection, expr: &str) -> String {
    conn.query_row(&format!("SELECT {expr} AS v"), [], |r| {
        Ok(duckdb_value_to_cell_tz(r.get_ref(0)?, None).to_string())
    })
    .unwrap_or_else(|e| panic!("{expr}: {e}"))
}

/// What matters most: the shown text casts back to exactly the value it came from.
/// A wrong rendering here is a silently changed value on a connection tab's save.
#[track_caller]
fn casts_back(conn: &Connection, ty: &str, expr: &str) {
    let text = shown(conn, expr);
    let same: bool = conn
        .query_row(
            &format!("SELECT CAST(? AS {ty}) IS NOT DISTINCT FROM CAST({expr} AS {ty})"),
            [&text],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| panic!("{expr} as {text}: {e}"));
    assert!(same, "{expr} shown as {text} comes back different");
}

#[test]
fn a_value_reads_as_itself_and_not_as_a_dump_of_its_batch() {
    let c = conn();
    for (expr, text) in [
        ("[1, 2, NULL]", "[1, 2, NULL]"),
        ("[]::INTEGER[]", "[]"),
        ("[0.5, 1.5]::FLOAT[2]", "[0.5, 1.5]"),
        ("['fruit', 'red']", "[fruit, red]"),
        ("['r,s', 'it''s']", "['r,s', 'it\\'s']"),
        ("{'x': 1, 'y': 'a, b'}", "{'x': 1, 'y': 'a, b'}"),
        ("MAP {'k': 1}", "{k=1}"),
        ("[{'a': [1, 2]}]", "[{'a': [1, 2]}]"),
        ("'happy'::ENUM('happy', 'sad')", "happy"),
        ("'a, b'::ENUM('a, b')", "a, b"),
        ("union_value(k := 1)", "1"),
    ] {
        assert_eq!(shown(&c, expr), text, "{expr}");
    }
}

/// Two rows of one batch each read as their own row.
#[test]
fn each_row_gets_its_own_value() {
    let c = conn();
    let mut stmt = c
        .prepare("SELECT [i, 2] FROM range(1, 3) t(i) ORDER BY i")
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| {
            Ok(duckdb_value_to_cell_tz(r.get_ref(0)?, None).to_string())
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, ["[1, 2]", "[2, 2]"]);
}

#[test]
fn numbers_and_flags_cast_back() {
    let c = conn();
    casts_back(&c, "INTEGER[]", "[1, 2, NULL]");
    casts_back(&c, "INTEGER[]", "[]::INTEGER[]");
    casts_back(&c, "FLOAT[2]", "[0.5, -1.25]::FLOAT[2]");
    casts_back(
        &c,
        "DOUBLE[]",
        "[0.1, 1e300, 'inf'::DOUBLE, 'nan'::DOUBLE]::DOUBLE[]",
    );
    casts_back(&c, "BIGINT[]", "[9223372036854775807, -1]::BIGINT[]");
    casts_back(
        &c,
        "HUGEINT[]",
        "[170141183460469231731687303715884105727]::HUGEINT[]",
    );
    casts_back(&c, "HUGEINT[]", "[-5, 0]::HUGEINT[]");
    casts_back(
        &c,
        "DECIMAL(18,2)[]",
        "[1.50, 12.34, -0.05, 0.00]::DECIMAL(18,2)[]",
    );
    casts_back(
        &c,
        "DECIMAL(38,2)[]",
        "[123456789012345678901234567890123456.78]::DECIMAL(38,2)[]",
    );
    casts_back(&c, "BOOLEAN[]", "[true, false, NULL]");
}

/// The list that decides it: each of these splits, ends or changes a value if it
/// is not quoted and escaped the way DuckDB reads it back.
#[test]
fn text_that_looks_like_syntax_casts_back() {
    let c = conn();
    for tricky in [
        "plain",
        "a, b",
        "it''s",
        "[x]",
        "{y}",
        "(z)",
        "k=v",
        "c:d",
        "null",
        "NULL",
        "",
        " pad ",
        "back\\slash",
        "ends with \\",
        "dq\"uote",
        "línea 列",
    ] {
        casts_back(&c, "VARCHAR[]", &format!("['{tricky}', 'next']"));
        casts_back(
            &c,
            "STRUCT(a VARCHAR, b INTEGER)",
            &format!("{{'a': '{tricky}', 'b': 1}}"),
        );
    }
    casts_back(
        &c,
        "VARCHAR[]",
        "['line' || chr(10) || 'break', 'tab' || chr(9)]",
    );
    casts_back(&c, "VARCHAR[]", "[NULL, 'NULL', 'null', '']");
}

#[test]
fn nested_shapes_cast_back() {
    let c = conn();
    casts_back(&c, "INTEGER[][]", "[[1], [2, 3], []]");
    casts_back(
        &c,
        "STRUCT(a INTEGER[])[]",
        "[{'a': [1, 2]}, {'a': []}, NULL]",
    );
    casts_back(
        &c,
        "STRUCT(v INTEGER[][], s STRUCT(t VARCHAR))",
        "{'v': [[1], [2, 3]], 's': {'t': 'a, b'}}",
    );
    casts_back(
        &c,
        "STRUCT(\"my field\" INTEGER, \"a,b\" INTEGER)",
        "{'my field': 1, 'a,b': 2}",
    );
    casts_back(&c, "STRUCT(x INTEGER, y VARCHAR)", "{'x': NULL, 'y': ''}");
    casts_back(
        &c,
        "MAP(VARCHAR, INTEGER)",
        "MAP {'k': 1, 'a=b': 2, 'c,d': 3}",
    );
    casts_back(
        &c,
        "MAP(VARCHAR, VARCHAR[])",
        "MAP {'k': ['x', 'y,z'], 'empty': []}",
    );
    casts_back(&c, "MAP(INTEGER, VARCHAR)", "MAP {1: 'a=b', 2: NULL}");
}

#[test]
fn dates_times_and_bytes_cast_back() {
    let c = conn();
    casts_back(&c, "DATE[]", "[DATE '2026-11-06', NULL]");
    casts_back(&c, "TIME[]", "[TIME '12:34:56.789']");
    casts_back(
        &c,
        "TIMESTAMP[]",
        "[TIMESTAMP '2026-11-06 12:34:56.123456']",
    );
    casts_back(
        &c,
        "TIMESTAMP WITH TIME ZONE[]",
        "[TIMESTAMPTZ '2026-11-06 12:34:56+01']",
    );
    casts_back(&c, "TIME_NS[]", "['12:34:56.123456789'::TIME_NS]");
    casts_back(
        &c,
        "INTERVAL[]",
        "[INTERVAL 3 DAY, INTERVAL 2 HOUR, INTERVAL 1 MONTH]",
    );
    casts_back(&c, "BLOB[]", "['abc'::BLOB, '\\xFF\\x00'::BLOB]");
    casts_back(
        &c,
        "UUID[]",
        "['12345678-1234-1234-1234-123456789012'::UUID]",
    );
    casts_back(
        &c,
        "ENUM('happy', 'a, b')[]",
        "['happy'::ENUM('happy', 'a, b'), 'a, b'::ENUM('happy', 'a, b')]",
    );
}

#[test]
fn a_top_level_enum_casts_back() {
    let c = conn();
    casts_back(&c, "ENUM('a, b', 'c')", "'a, b'::ENUM('a, b', 'c')");
}

/// The known ceiling, pinned so a DuckDB that closes it shows up. No text says which
/// member of a union a value is in: DuckDB prints `1` itself, refuses to read it
/// without a text member, and with one puts it there. So a whole-row save on a
/// connection tab moves such a value into the text member. A `.duckdb` tab writes
/// only the cells that changed, unless a schema change rewrites the rows.
#[test]
fn a_union_shows_its_value_but_no_text_can_say_its_member() {
    let c = conn();
    assert_eq!(shown(&c, "union_value(k := 1)"), "1");
    let tag = |sql: &str| c.query_row(sql, [], |r| r.get::<_, String>(0));
    let duckdb_says = "SELECT CAST(union_value(k := 1)::UNION(k INTEGER, s VARCHAR) AS VARCHAR)";
    assert_eq!(tag(duckdb_says).unwrap(), "1");
    assert!(tag("SELECT CAST('1' AS UNION(k INTEGER))::VARCHAR").is_err());
    assert_eq!(
        tag("SELECT union_tag(CAST('1' AS UNION(k INTEGER, s VARCHAR)))::VARCHAR").unwrap(),
        "s"
    );
}

/// Reached the debug-print fallback before, and would not have shown an unsigned
/// value of this size as a number at all.
#[test]
fn an_unsigned_128_bit_number_reads_as_a_number() {
    let c = conn();
    let max = "340282366920938463463374607431768211455";
    assert_eq!(shown(&c, &format!("{max}::UHUGEINT")), max);
}
