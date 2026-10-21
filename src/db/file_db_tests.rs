use super::*;
use crate::data::CellValue;
use crate::db::sqlite_file::SqliteFile;

fn sqlite(dir: &Path, writes: bool) -> FileConnector<SqliteFile> {
    let path = dir.join("t.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1), (2);")
        .unwrap();
    let conn = DbConnection::file(
        "t".into(),
        "t",
        DbEngine::Sqlite,
        &path.to_string_lossy(),
        writes,
    );
    FileConnector::connect(&conn).unwrap()
}

fn count(c: &mut impl DbConnector) -> i64 {
    match &c.query("SELECT count(*) FROM t").unwrap().rows[0][0] {
        CellValue::Int(n) => *n,
        v => panic!("{v:?}"),
    }
}

#[test]
fn statements_are_sorted_by_what_they_do_to_a_transaction() {
    assert_eq!(tx_word("BEGIN"), TxWord::Begin);
    assert_eq!(tx_word("  begin transaction;"), TxWord::Begin);
    assert_eq!(tx_word("START TRANSACTION"), TxWord::Begin);
    assert_eq!(tx_word("-- note\nCOMMIT"), TxWord::End);
    assert_eq!(tx_word("END"), TxWord::End);
    assert_eq!(tx_word("ROLLBACK"), TxWord::End);
    assert_eq!(tx_word("ABORT"), TxWord::End);
    assert_eq!(tx_word("ROLLBACK TO SAVEPOINT s1"), TxWord::Other);
    assert_eq!(tx_word("ROLLBACK TRANSACTION TO s1"), TxWord::Other);
    assert_eq!(tx_word("SELECT 'BEGIN'"), TxWord::Other);
    assert_eq!(tx_word("INSERT INTO t VALUES (1)"), TxWord::Other);
}

#[test]
fn the_file_is_held_only_inside_a_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = sqlite(dir.path(), true);
    assert_eq!(count(&mut c), 2);
    assert!(c.tx.is_none(), "a plain query must not keep the file open");
    c.execute("BEGIN").unwrap();
    assert!(c.tx.is_some());
    c.execute("INSERT INTO t VALUES (3)").unwrap();
    assert_eq!(count(&mut c), 3, "the transaction sees its own row");
    c.execute("ROLLBACK").unwrap();
    assert!(c.tx.is_none());
    // Had the INSERT run on a connection of its own it would have committed,
    // and the count would still be 3.
    assert_eq!(count(&mut c), 2, "ROLLBACK undid the insert");
}

#[test]
fn commit_keeps_what_the_transaction_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = sqlite(dir.path(), true);
    c.execute("BEGIN").unwrap();
    c.execute("INSERT INTO t VALUES (3)").unwrap();
    c.execute("COMMIT").unwrap();
    assert!(c.tx.is_none());
    assert_eq!(count(&mut c), 3);
}

#[test]
fn begin_and_commit_sent_as_queries_still_hold_the_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = sqlite(dir.path(), true);
    c.query("BEGIN").unwrap();
    assert!(c.tx.is_some());
    c.execute("INSERT INTO t VALUES (3)").unwrap();
    c.query("ROLLBACK").unwrap();
    assert!(c.tx.is_none());
    assert_eq!(count(&mut c), 2);
}

#[test]
fn an_empty_or_missing_file_is_refused_and_never_created() {
    let dir = tempfile::tempdir().unwrap();
    let empty = DbConnection::file("e".into(), "e", DbEngine::Sqlite, "  ", false);
    let err = file_path(&empty).unwrap_err();
    assert!(format!("{err:#}").contains("names no file"), "{err:#}");

    let missing = dir.path().join("nope.sqlite");
    let conn = DbConnection::file(
        "m".into(),
        "m",
        DbEngine::Sqlite,
        &missing.to_string_lossy(),
        true,
    );
    let err = crate::db::connect(&conn, None, None)
        .err()
        .expect("a missing file must not connect");
    assert!(format!("{err:#}").contains("does not exist"), "{err:#}");
    assert!(!missing.exists(), "the file was created");
}
