use super::*;

fn fmt(sql: &str, opts: &SqlFormatOptions) -> String {
    format_sql(sql, opts, FormatDialect::Generic)
}

#[test]
fn default_style_uppercases_and_breaks_clauses() {
    let out = fmt(
        "select a, b from t where a = 1",
        &SqlFormatOptions::default(),
    );
    assert_eq!(out, "SELECT\n    a,\n    b\nFROM\n    t\nWHERE\n    a = 1");
}

#[test]
fn lower_case_and_as_written() {
    let lower = SqlFormatOptions {
        keyword_case: KeywordCase::Lower,
        ..Default::default()
    };
    assert!(fmt("SELECT a FROM t", &lower).starts_with("select\n"));
    let keep = SqlFormatOptions {
        keyword_case: KeywordCase::AsWritten,
        ..Default::default()
    };
    assert!(fmt("SeLeCt a FROM t", &keep).starts_with("SeLeCt\n"));
}

#[test]
fn indent_styles() {
    let two = SqlFormatOptions {
        indent: IndentStyle::TwoSpaces,
        ..Default::default()
    };
    assert!(fmt("select a from t", &two).contains("\n  a\n"));
    let tab = SqlFormatOptions {
        indent: IndentStyle::Tab,
        ..Default::default()
    };
    assert!(fmt("select a from t", &tab).contains("\n\ta\n"));
}

#[test]
fn leading_commas_move_to_the_next_line() {
    let opts = SqlFormatOptions {
        commas: CommaStyle::Leading,
        ..Default::default()
    };
    assert_eq!(
        fmt("select a, b, c from t", &opts),
        "SELECT\n    a\n    , b\n    , c\nFROM\n    t"
    );
}

#[test]
fn leading_commas_leave_comment_lines_and_strings_alone() {
    let out = move_commas_to_line_start("SELECT\n    'x,y' AS s, -- one, two,\n    b");
    assert_eq!(out, "SELECT\n    'x,y' AS s, -- one, two,\n    b");
}

#[test]
fn inline_width_keeps_short_lists_on_one_line() {
    let opts = SqlFormatOptions {
        inline_width: 40,
        ..Default::default()
    };
    assert!(fmt("select a, b, c from t", &opts).contains("a, b, c"));
}

#[test]
fn blank_lines_between_statements() {
    let opts = SqlFormatOptions {
        blank_lines_between: 2,
        ..Default::default()
    };
    let out = fmt("select 1; select 2;", &opts);
    assert!(out.contains(";\n\n\nSELECT"), "{out}");
}

#[test]
fn comments_survive() {
    let out = fmt(
        "-- top\nselect a -- why\nfrom t",
        &SqlFormatOptions::default(),
    );
    assert!(out.contains("-- top"));
    assert!(out.contains("-- why"));
}

const NESTED: &str = "select coalesce(sum(o.total), 0, 1) as spent from o \
where o.id in (select id from vip) group by o.name";

#[test]
fn clause_width_follows_the_list_width_until_set() {
    let follows = SqlFormatOptions {
        inline_width: 40,
        ..Default::default()
    };
    assert_eq!(follows.effective_clause_width(), 40);
    assert!(fmt(NESTED, &follows).contains("GROUP BY o.name"));

    let own = SqlFormatOptions {
        inline_width: 40,
        clause_width: Some(0),
        ..Default::default()
    };
    assert!(
        fmt(NESTED, &own).contains("GROUP BY\n"),
        "{}",
        fmt(NESTED, &own)
    );
}

#[test]
fn bracket_width_breaks_long_brackets() {
    let narrow = SqlFormatOptions {
        bracket_width: 10,
        ..Default::default()
    };
    // Function names keep their case; only keywords change.
    assert!(fmt(NESTED, &narrow).to_uppercase().contains("COALESCE(\n"));
    assert!(
        fmt(NESTED, &SqlFormatOptions::default())
            .to_uppercase()
            .contains("COALESCE(SUM(O.TOTAL), 0, 1)")
    );
}

#[test]
fn one_line_breaks_nothing_even_with_leading_commas() {
    let opts = SqlFormatOptions {
        one_line: true,
        commas: CommaStyle::Leading,
        ..Default::default()
    };
    let out = fmt("select a,\n b from t", &opts);
    assert!(!out.contains('\n'), "{out}");
    assert!(out.starts_with("SELECT a, b FROM t"), "{out}");
}

#[test]
fn final_semicolon_is_added_once_and_never_inside_a_comment() {
    let opts = SqlFormatOptions {
        final_semicolon: true,
        one_line: true,
        ..Default::default()
    };
    assert_eq!(fmt("select 1", &opts), "SELECT 1;");
    assert_eq!(fmt("select 1;", &opts), "SELECT 1;");
    assert_eq!(
        add_final_semicolon("SELECT 1 -- note"),
        "SELECT 1 -- note\n;"
    );
    assert_eq!(add_final_semicolon("   "), "   ");
}

#[test]
fn clause_width_needs_a_list_width() {
    // sqlformat puts a clause's items at column 0 when only the clause may
    // stay inline: `SELECT a,\nb,\nc`. So it only applies with a list width.
    let opts = SqlFormatOptions {
        inline_width: 0,
        clause_width: Some(40),
        ..Default::default()
    };
    let out = fmt("select a, b, c from t group by a, b", &opts);
    assert!(!out.contains("\nb,"), "{out}");
}

/// The examples the Settings explanations quote, checked against the
/// formatter so the help cannot drift from what Format does.
#[test]
fn the_settings_explanations_tell_the_truth() {
    let at30 = SqlFormatOptions {
        inline_width: 30,
        ..Default::default()
    };
    let out = fmt(
        "select id, name, city from t where t.id in (select id from v) group by c.name",
        &at30,
    );
    assert!(out.contains("SELECT id, name, city"), "{out}");
    assert!(out.contains("GROUP BY c.name"), "{out}");
    assert!(out.contains("IN (SELECT id FROM v)"), "{out}");

    let zero = fmt("select id, name, city from t", &SqlFormatOptions::default());
    assert!(
        zero.contains("id,\n"),
        "0 puts each item on its own line: {zero}"
    );

    let brackets = fmt(
        "select coalesce(sum(o.total), 0) from o where x in (1, 2, 3)",
        &SqlFormatOptions::default(),
    )
    .to_uppercase();
    assert!(brackets.contains("COALESCE(SUM(O.TOTAL), 0)"), "{brackets}");
    assert!(brackets.contains("IN (1, 2, 3)"), "{brackets}");
}
