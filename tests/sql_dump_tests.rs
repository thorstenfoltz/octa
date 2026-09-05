//! SQL dump reader: the three dump flavours people are actually sent, opened
//! through the same registry-by-name path the **Open as** menu uses.

use octa::data::CellValue;
use octa::formats::{FormatReader, FormatRegistry};

/// A `mysqldump`, with everything that makes one hard: conditional comments,
/// `AUTO_INCREMENT`, per-column `COLLATE` / `CHARACTER SET` / `COMMENT`, an
/// `enum` type, MySQL-only `KEY` clauses, table options after the closing
/// paren, `LOCK TABLES`, and backslash escapes in the data.
const MYSQL: &str = r#"-- MySQL dump 10.13  Distrib 8.0.36, for Linux (x86_64)
--
-- Host: localhost    Database: shop
-- ------------------------------------------------------
/*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;
/*!40103 SET TIME_ZONE='+00:00' */;

DROP TABLE IF EXISTS `customers`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
CREATE TABLE `customers` (
  `id` int NOT NULL AUTO_INCREMENT,
  `name` varchar(100) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'full name',
  `email` varchar(255) CHARACTER SET utf8mb4 DEFAULT NULL,
  `tier` enum('free','pro') NOT NULL DEFAULT 'free',
  `spent` decimal(10,2) DEFAULT '0.00',
  `created_at` datetime DEFAULT current_timestamp() ON UPDATE current_timestamp(),
  PRIMARY KEY (`id`),
  UNIQUE KEY `email` (`email`),
  KEY `idx_name` (`name`)
) ENGINE=InnoDB AUTO_INCREMENT=4 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

LOCK TABLES `customers` WRITE;
/*!40000 ALTER TABLE `customers` DISABLE KEYS */;
INSERT INTO `customers` VALUES
(1,'Ada Lovelace','ada@example.com','pro',12.50,'2024-01-02 10:00:00'),
(2,'O\'Brien; Sean','sean@example.com','free',0.00,'2024-02-03 11:30:00'),
(3,'Line\nBreak',NULL,'free',0.00,'2024-03-04 12:00:00'),
(4,'back\\slash','b@example.com','pro',7.25,'2024-04-05 09:00:00');
/*!40000 ALTER TABLE `customers` ENABLE KEYS */;
UNLOCK TABLES;

DROP TABLE IF EXISTS `notes`;
CREATE TABLE `notes` (
  `id` int NOT NULL AUTO_INCREMENT,
  `body` text,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
INSERT INTO `notes` VALUES (1,'first');
"#;

/// A default `pg_dump`: schema-qualified names, `SET` preamble, a type SQLite
/// cannot spell, `COPY ... FROM stdin` for the data, and statements that only
/// Postgres can run.
fn pg_dump() -> String {
    // Written with explicit escapes: the COPY body is tab-separated, and a
    // literal tab in the source would be invisible and easy to reformat away.
    [
        "--",
        "-- PostgreSQL database dump",
        "--",
        "",
        // pg_dump 17.6 and newer open with this. It is a psql meta-command,
        // not SQL, and glued itself to the next statement until the reader
        // learned to skip it.
        "\\restrict XqtSEwFIAJ4lVXvFP27NbJOeJrgmwTjeJtjQuWA8jGXBpZFe",
        "",
        "SET statement_timeout = 0;",
        "SET standard_conforming_strings = on;",
        "SELECT pg_catalog.set_config('search_path', '', false);",
        "",
        "--",
        "-- Name: orders; Type: TABLE; Schema: public; Owner: postgres",
        "--",
        "",
        "CREATE TABLE public.orders (",
        "    id integer NOT NULL,",
        "    customer text NOT NULL,",
        "    placed_at timestamp with time zone,",
        "    total numeric(10,2)",
        ");",
        "",
        "ALTER TABLE public.orders OWNER TO postgres;",
        "",
        "--",
        "-- Data for Name: orders; Type: TABLE DATA; Schema: public",
        "--",
        "",
        "COPY public.orders (id, customer, placed_at, total) FROM stdin;",
        "1\tAda\t2024-01-02 10:00:00+00\t12.50",
        "2\tSean\t\\N\t0.00",
        "3\tTab\\there\t2024-03-04 12:00:00+00\t5.00",
        "\\.",
        "",
        "ALTER TABLE ONLY public.orders ADD CONSTRAINT orders_pkey PRIMARY KEY (id);",
        "",
    ]
    .join("\n")
}

/// What `sqlite3 db .dump` writes: plain SQL, plus the transaction wrapper.
const SQLITE: &str = r#"PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT, qty INTEGER);
INSERT INTO items VALUES(1,'bolt',10);
INSERT INTO items VALUES(2,'nut; washer',4);
COMMIT;
"#;

fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

fn reader() -> &'static dyn FormatReader {
    // Leaked on purpose: a test helper that hands out a registry-owned
    // reference needs the registry to outlive the borrow.
    let reg = Box::leak(Box::new(FormatRegistry::new()));
    reg.reader_by_name("SQL dump")
        .expect("the SQL dump reader must be registered under this name")
}

/// The whole point of the design: `.sql` keeps opening as text, and the dump
/// reader is reached only by name.
#[test]
fn a_sql_file_still_opens_as_text_by_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "dump.sql", SQLITE);
    let reg = FormatRegistry::new();
    assert_eq!(
        reg.reader_for_path(&path).map(|r| r.name().to_string()),
        Some("Text".to_string()),
        "opening a .sql by extension must not replay it"
    );
    assert!(
        reg.reader_by_name("SQL dump").is_some(),
        "but it must be reachable by name"
    );
}

#[test]
fn a_mysqldump_opens_as_its_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "shop.sql", MYSQL);
    let r = reader();

    let tables = r.list_tables(&path).unwrap().expect("tables");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["customers", "notes"]);

    let customers = &tables[0];
    let cols: Vec<&str> = customers.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        cols,
        vec!["id", "name", "email", "tier", "spent", "created_at"],
        "MySQL-only KEY clauses must not become columns"
    );
    assert_eq!(customers.row_count, Some(4));

    let table = r.read_table(&path, "customers").unwrap();
    assert_eq!(table.row_count(), 4);
    assert_eq!(
        table.get(0, 1),
        Some(&CellValue::String("Ada Lovelace".into()))
    );
    // The escaped quote and the semicolon inside it: the statement must not
    // have been split there, and the backslash must be gone.
    assert_eq!(
        table.get(1, 1),
        Some(&CellValue::String("O'Brien; Sean".into()))
    );
    // `\n` in a mysqldump is a newline, not the two characters.
    assert_eq!(
        table.get(2, 1),
        Some(&CellValue::String("Line\nBreak".into()))
    );
    assert_eq!(table.get(2, 2), Some(&CellValue::Null));
    // `\\` is one backslash, not two.
    assert_eq!(
        table.get(3, 1),
        Some(&CellValue::String("back\\slash".into()))
    );
}

#[test]
fn a_pg_dump_replays_its_copy_block() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "orders.sql", &pg_dump());
    let r = reader();

    let tables = r.list_tables(&path).unwrap().expect("tables");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["orders"],
        "the schema qualifier must be dropped, not kept in the name"
    );

    let table = r.read_table(&path, "orders").unwrap();
    assert_eq!(table.row_count(), 3, "COPY rows are the data of a pg_dump");
    assert_eq!(table.get(0, 1), Some(&CellValue::String("Ada".into())));
    assert_eq!(table.get(1, 2), Some(&CellValue::Null), "\\N is NULL");
    assert_eq!(
        table.get(2, 1),
        Some(&CellValue::String("Tab\there".into())),
        "COPY escapes are decoded"
    );
}

#[test]
fn a_sqlite_dump_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "items.sql", SQLITE);
    let r = reader();
    let table = r.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 2);
    // A semicolon inside a string literal must not end the statement.
    assert_eq!(
        table.get(1, 1),
        Some(&CellValue::String("nut; washer".into()))
    );
}

/// A dump the reader cannot make sense of has to say so, not open empty.
#[test]
fn a_file_that_is_not_a_dump_explains_itself() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "notes.sql", "this is not SQL at all\n");
    let err = reader().read_file(&path).unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("no tables"),
        "unhelpful error for a non-dump: {text}"
    );
}

/// A statement carrying data that will not replay must be counted; noise like
/// `LOCK TABLES` must not be.
#[test]
fn only_lost_data_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "partial.sql",
        "CREATE TABLE good (a INTEGER);\n\
         INSERT INTO good VALUES (1);\n\
         LOCK TABLES `good` WRITE;\n\
         INSERT INTO missing_table VALUES (2);\n\
         UNLOCK TABLES;\n",
    );
    let r = reader();
    let table = r.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 1);

    let report = octa::formats::sql_dump_reader::report_for(&path).expect("a report for this file");
    assert_eq!(
        report.skipped.len(),
        1,
        "only the failed INSERT counts, not LOCK/UNLOCK TABLES: {:?}",
        report.skipped
    );
    assert!(report.skipped[0].contains("missing_table"));
    assert!(report.banner().is_some());
}
