use super::*;

#[test]
fn strings_names_and_comments_are_emptied() {
    let code = |s| code_only(s, Dialect::TSql);
    assert_eq!(
        code("SELECT 'commit', [exec] -- rollback\nFROM t /* exec */").as_deref(),
        Some("SELECT '', []  \nFROM t  ")
    );
    // A doubled quote stays inside the text, however the engine reads it.
    assert_eq!(
        code("SELECT 'it''s commit'").as_deref(),
        Some("SELECT ''''")
    );
    // `]]` is a bracket inside a bracketed name.
    assert_eq!(
        code("SELECT [a]] commit] x").as_deref(),
        Some("SELECT [] x")
    );
    assert_eq!(
        code_only("SELECT `a`, \"b\" FROM t", Dialect::Spark).as_deref(),
        Some("SELECT ``, \"\" FROM t")
    );
}

#[test]
fn sql_server_nests_comments_and_the_others_are_not_guessed() {
    let nested = "SELECT 1 /* a /* b */ COMMIT */ x";
    assert_eq!(
        code_only(nested, Dialect::TSql).as_deref(),
        Some("SELECT 1   x")
    );
    assert_eq!(code_only(nested, Dialect::Redshift), None);
    assert_eq!(code_only(nested, Dialect::Spark), None);
}

#[test]
fn text_that_reads_two_ways_has_no_answer() {
    for (sql, dialect) in [
        ("SELECT 'a\\' INTO t", Dialect::Redshift),
        ("SELECT 'a\\' INSERT", Dialect::Spark),
        ("SELECT $$ x $$ INTO t", Dialect::Redshift),
        ("SELECT 'open", Dialect::TSql),
        ("SELECT [open", Dialect::TSql),
        ("SELECT 1 /* open", Dialect::TSql),
    ] {
        assert_eq!(code_only(sql, dialect), None, "{sql}");
    }
    // SQL Server has no escapes: a backslash is just a character.
    assert!(code_only("SELECT 'a\\'", Dialect::TSql).is_some());
}

#[test]
fn a_word_counts_in_code_only_and_everywhere_when_unsure() {
    let tsql = |s| has_code_word(s, Dialect::TSql, &["COMMIT", "EXEC"]);
    assert!(!tsql("SELECT * FROM log WHERE step = 'commit'"));
    assert!(!tsql("SELECT @exec, #exec, [exec] FROM t -- exec"));
    assert!(tsql("SELECT 1 commit"));
    assert!(tsql("SELECT 1EXEC('x')"));
    // Unclosed: the words count anywhere.
    assert!(tsql("SELECT 'a commit"));
    let red = |s| has_code_word(s, Dialect::Redshift, &["INTO"]);
    assert!(!red("SELECT * FROM t WHERE note = 'moved into archive'"));
    assert!(red("SELECT * INTO #tmp FROM t"));
    assert!(red("SELECT 'a\\' INTO t2 FROM t"));
    assert!(!red("SELECT 'a\\b' FROM t"));
}
