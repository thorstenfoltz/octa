//! Which tables in this database point at which, according to the database?
//!
//! The relationship map's other two sources guess from values. A server
//! already knows: somebody declared the foreign keys. Reading them costs two
//! catalog queries and no table data at all, so it answers instantly on a
//! warehouse where sampling every table would not.
//!
//! Shaped like [`crate::db::row_key_sql`]: pure SQL builders with `''`-doubled
//! literals, a per-engine dispatch, and a pure assembler. Nothing here touches
//! the UI, so the GUI dialog and the MCP tool run the same code.
//!
//! **A declaration is not a measurement.** Postgres, MySQL, SQL Server and
//! Exasol enforce their foreign keys, so an edge from those engines is a fact
//! about the rows too. Redshift, Snowflake, Databricks and BigQuery accept a
//! declaration and enforce nothing, so there an edge says only what somebody
//! intended. [`crate::data::rel_map::score_edges`] is how the map checks.

use std::collections::{HashMap, HashSet};

use crate::data::DataTable;
use crate::data::rel_map::{RelMap, Relationship, TableNode};
use crate::db::{DbConnector, DbEngine};

/// One column of one table, as the catalog lists it: schema, table, column.
/// Kept as a tuple rather than a struct because it is only ever built by
/// [`scan`] and consumed by [`build_db_map`], which want it in that order.
pub type ColumnRow = (String, String, String);

/// One column of one declared foreign key. A composite key produces one of
/// these per column pair, all sharing `constraint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub child_schema: String,
    pub child_table: String,
    pub child_column: String,
    pub parent_schema: String,
    pub parent_table: String,
    pub parent_column: String,
    pub constraint: String,
}

impl ForeignKey {
    fn child(&self) -> String {
        format!("{}.{}", self.child_schema, self.child_table)
    }
    fn parent(&self) -> String {
        format!("{}.{}", self.parent_schema, self.parent_table)
    }
}

/// Result of assembling a map from catalog metadata.
#[derive(Debug, Clone, Default)]
pub struct DbMap {
    pub map: RelMap,
    /// Foreign keys whose other side is not among the drawn tables, so there
    /// was no box to attach the line to. Reported rather than dropped in
    /// silence: it is how a user learns the schema they picked is not the
    /// whole story.
    pub skipped_edges: usize,
    /// The table cap was reached and some tables were left out.
    pub truncated: bool,
}

/// SQL literal with `'` doubled, as everywhere else in this module's siblings.
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn lit_list(schemas: &[String]) -> String {
    schemas
        .iter()
        .map(|s| lit(s))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `catalog.` prefix for the three-level engines, empty otherwise.
fn catalog_prefix(engine: DbEngine, catalog: Option<&str>) -> String {
    match catalog {
        Some(c) if !c.is_empty() => format!("{}.", engine.quote_ident(c)),
        _ => String::new(),
    }
}

/// The seven values [`scan`] reads out of a foreign-key result, by column
/// name, in this order: child schema, table, column; parent schema, table,
/// column; constraint name.
///
/// Every dialect aliases its output to the canonical names except Snowflake,
/// whose `SHOW IMPORTED KEYS` output cannot be aliased, so its own names are
/// listed instead. Lookup is case-insensitive because Snowflake and Exasol
/// upper-case unquoted identifiers.
fn fk_result_columns(engine: DbEngine) -> [&'static str; 7] {
    match engine {
        DbEngine::Snowflake => [
            "fk_schema_name",
            "fk_table_name",
            "fk_column_name",
            "pk_schema_name",
            "pk_table_name",
            "pk_column_name",
            "fk_name",
        ],
        _ => [
            "child_schema",
            "child_table",
            "child_column",
            "parent_schema",
            "parent_table",
            "parent_column",
            "constraint_name",
        ],
    }
}

/// Statements listing the declared foreign keys whose **child** table lives in
/// one of `schemas`.
///
/// Returns a `Vec` because two engines cannot answer for several schemas in
/// one statement: Snowflake's `SHOW IMPORTED KEYS` takes one schema, and
/// BigQuery's `INFORMATION_SCHEMA` views are dataset-scoped.
pub fn foreign_key_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schemas: &[String],
) -> anyhow::Result<Vec<String>> {
    if !engine.has_foreign_keys() {
        anyhow::bail!(
            "{} has no foreign keys, so there are no declared relationships to read",
            engine.label()
        );
    }
    if schemas.is_empty() {
        anyhow::bail!("pick at least one schema");
    }
    let list = lit_list(schemas);

    Ok(match engine {
        // The portable SQL-92 form. `key_column_usage` twice: once for the
        // child columns, once for the parent's, aligned on
        // `position_in_unique_constraint` so a composite key pairs its columns
        // in the right order rather than by luck.
        DbEngine::Postgres | DbEngine::Redshift => vec![format!(
            "SELECT kcu.table_schema AS child_schema, kcu.table_name AS child_table, \
             kcu.column_name AS child_column, pku.table_schema AS parent_schema, \
             pku.table_name AS parent_table, pku.column_name AS parent_column, \
             rc.constraint_name AS constraint_name \
             FROM information_schema.referential_constraints rc \
             JOIN information_schema.key_column_usage kcu \
               ON kcu.constraint_name = rc.constraint_name \
              AND kcu.constraint_schema = rc.constraint_schema \
             JOIN information_schema.key_column_usage pku \
               ON pku.constraint_name = rc.unique_constraint_name \
              AND pku.constraint_schema = rc.unique_constraint_schema \
              AND pku.ordinal_position = kcu.position_in_unique_constraint \
             WHERE kcu.table_schema IN ({list}) \
             ORDER BY kcu.table_schema, kcu.table_name, rc.constraint_name, kcu.ordinal_position"
        )],
        // MySQL carries the referenced side on `key_column_usage` itself, so
        // no join at all. A non-NULL `referenced_table_name` is what makes a
        // row a foreign key there.
        DbEngine::MySql => vec![format!(
            "SELECT kcu.table_schema AS child_schema, kcu.table_name AS child_table, \
             kcu.column_name AS child_column, \
             kcu.referenced_table_schema AS parent_schema, \
             kcu.referenced_table_name AS parent_table, \
             kcu.referenced_column_name AS parent_column, \
             kcu.constraint_name AS constraint_name \
             FROM information_schema.key_column_usage kcu \
             WHERE kcu.referenced_table_name IS NOT NULL \
               AND kcu.table_schema IN ({list}) \
             ORDER BY kcu.table_schema, kcu.table_name, kcu.constraint_name, kcu.ordinal_position"
        )],
        // SQL Server's `INFORMATION_SCHEMA.KEY_COLUMN_USAGE` has no
        // `position_in_unique_constraint`, so the portable query cannot pair a
        // composite key's columns there. `sys.foreign_key_columns` states both
        // sides directly and is the documented route.
        DbEngine::Mssql => vec![format!(
            "SELECT SCHEMA_NAME(ct.schema_id) AS child_schema, ct.name AS child_table, \
             cc.name AS child_column, SCHEMA_NAME(pt.schema_id) AS parent_schema, \
             pt.name AS parent_table, pc.name AS parent_column, fk.name AS constraint_name \
             FROM sys.foreign_keys fk \
             JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
             JOIN sys.tables ct ON ct.object_id = fkc.parent_object_id \
             JOIN sys.columns cc ON cc.object_id = fkc.parent_object_id \
              AND cc.column_id = fkc.parent_column_id \
             JOIN sys.tables pt ON pt.object_id = fkc.referenced_object_id \
             JOIN sys.columns pc ON pc.object_id = fkc.referenced_object_id \
              AND pc.column_id = fkc.referenced_column_id \
             WHERE SCHEMA_NAME(ct.schema_id) IN ({list}) \
             ORDER BY fk.name, fkc.constraint_column_id"
        )],
        // Exasol keeps constraints in its own SYS views, not information_schema.
        DbEngine::Exasol => vec![format!(
            "SELECT CONSTRAINT_SCHEMA AS CHILD_SCHEMA, CONSTRAINT_TABLE AS CHILD_TABLE, \
             COLUMN_NAME AS CHILD_COLUMN, REFERENCED_SCHEMA AS PARENT_SCHEMA, \
             REFERENCED_TABLE AS PARENT_TABLE, REFERENCED_COLUMN AS PARENT_COLUMN, \
             CONSTRAINT_NAME AS CONSTRAINT_NAME \
             FROM SYS.EXA_ALL_CONSTRAINT_COLUMNS \
             WHERE CONSTRAINT_TYPE = 'FOREIGN KEY' AND CONSTRAINT_SCHEMA IN ({list}) \
             ORDER BY CONSTRAINT_SCHEMA, CONSTRAINT_TABLE, CONSTRAINT_NAME, ORDINAL_POSITION"
        )],
        // Snowflake's information_schema has table_constraints but nothing
        // naming the referenced columns, so SHOW is the only route. It takes
        // one schema and its output columns are fixed - see
        // `fk_result_columns`.
        DbEngine::Snowflake => schemas
            .iter()
            .map(|s| {
                let qualified = match catalog {
                    Some(c) if !c.is_empty() => {
                        format!("{}.{}", engine.quote_ident(c), engine.quote_ident(s))
                    }
                    _ => engine.quote_ident(s),
                };
                format!("SHOW IMPORTED KEYS IN SCHEMA {qualified}")
            })
            .collect(),
        // Unity Catalog exposes the SQL-92 views but no
        // `position_in_unique_constraint`, so `constraint_column_usage` names
        // the parent columns. Composite keys pair by the catalog's own row
        // order there rather than by an explicit ordinal.
        DbEngine::Databricks => {
            let p = catalog_prefix(engine, catalog);
            vec![format!(
                "SELECT kcu.table_schema AS child_schema, kcu.table_name AS child_table, \
                 kcu.column_name AS child_column, ccu.table_schema AS parent_schema, \
                 ccu.table_name AS parent_table, ccu.column_name AS parent_column, \
                 rc.constraint_name AS constraint_name \
                 FROM {p}information_schema.referential_constraints rc \
                 JOIN {p}information_schema.key_column_usage kcu \
                   ON kcu.constraint_schema = rc.constraint_schema \
                  AND kcu.constraint_name = rc.constraint_name \
                 JOIN {p}information_schema.constraint_column_usage ccu \
                   ON ccu.constraint_schema = rc.unique_constraint_schema \
                  AND ccu.constraint_name = rc.unique_constraint_name \
                 WHERE kcu.table_schema IN ({list}) \
                 ORDER BY kcu.table_schema, kcu.table_name, rc.constraint_name, \
                 kcu.ordinal_position"
            )]
        }
        // BigQuery's constraint views are dataset-scoped, so one statement per
        // dataset, each fully qualified with the project.
        DbEngine::BigQuery => schemas
            .iter()
            .map(|s| {
                let p = match catalog {
                    Some(c) if !c.is_empty() => {
                        format!("{}.{}", engine.quote_ident(c), engine.quote_ident(s))
                    }
                    _ => engine.quote_ident(s),
                };
                format!(
                    "SELECT kcu.table_schema AS child_schema, kcu.table_name AS child_table, \
                     kcu.column_name AS child_column, ccu.table_schema AS parent_schema, \
                     ccu.table_name AS parent_table, ccu.column_name AS parent_column, \
                     kcu.constraint_name AS constraint_name \
                     FROM {p}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu \
                     JOIN {p}.INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc \
                       ON tc.constraint_schema = kcu.constraint_schema \
                      AND tc.constraint_name = kcu.constraint_name \
                     JOIN {p}.INFORMATION_SCHEMA.CONSTRAINT_COLUMN_USAGE AS ccu \
                       ON ccu.constraint_schema = kcu.constraint_schema \
                      AND ccu.constraint_name = kcu.constraint_name \
                     WHERE tc.constraint_type = 'FOREIGN KEY' \
                     ORDER BY kcu.table_name, kcu.constraint_name, kcu.ordinal_position"
                )
            })
            .collect(),
        DbEngine::ClickHouse => unreachable!("guarded by has_foreign_keys above"),
    })
}

/// Statements listing every column of every table in `schemas`, so a box can
/// show its columns.
///
/// Deliberately covers **all** tables, not only the ones a foreign key names:
/// the dialog lets the user add an unlinked table to the picture, and this way
/// that costs no further round trip.
pub fn schema_columns_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schemas: &[String],
) -> Vec<String> {
    if schemas.is_empty() {
        return Vec::new();
    }
    let list = lit_list(schemas);
    match engine {
        DbEngine::Exasol => vec![format!(
            "SELECT COLUMN_SCHEMA AS TABLE_SCHEMA, COLUMN_TABLE AS TABLE_NAME, \
             COLUMN_NAME AS COLUMN_NAME FROM SYS.EXA_ALL_COLUMNS \
             WHERE COLUMN_SCHEMA IN ({list}) \
             ORDER BY COLUMN_SCHEMA, COLUMN_TABLE, COLUMN_ORDINAL_POSITION"
        )],
        DbEngine::BigQuery => schemas
            .iter()
            .map(|s| {
                let p = match catalog {
                    Some(c) if !c.is_empty() => {
                        format!("{}.{}", engine.quote_ident(c), engine.quote_ident(s))
                    }
                    _ => engine.quote_ident(s),
                };
                format!(
                    "SELECT table_schema, table_name, column_name \
                     FROM {p}.INFORMATION_SCHEMA.COLUMNS \
                     ORDER BY table_name, ordinal_position"
                )
            })
            .collect(),
        _ => {
            let p = catalog_prefix(engine, catalog);
            vec![format!(
                "SELECT table_schema, table_name, column_name \
                 FROM {p}information_schema.columns \
                 WHERE table_schema IN ({list}) \
                 ORDER BY table_schema, table_name, ordinal_position"
            )]
        }
    }
}

/// Index of `name` in `table`, case-insensitively. Snowflake and Exasol
/// upper-case unquoted identifiers, so an exact match would find nothing there.
fn col_index(table: &DataTable, name: &str) -> Option<usize> {
    table
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}

fn cell_text(table: &DataTable, row: usize, col: usize) -> String {
    table
        .get(row, col)
        .map(|c| c.to_string().trim().to_string())
        .unwrap_or_default()
}

/// Read the columns and the declared foreign keys of `schemas`.
///
/// Two catalog queries for most engines (one per schema on Snowflake and
/// BigQuery), and **no table data**, so the cost does not grow with the size
/// of the database.
pub fn scan(
    c: &mut dyn DbConnector,
    catalog: Option<&str>,
    schemas: &[String],
) -> anyhow::Result<(Vec<ColumnRow>, Vec<ForeignKey>)> {
    let engine = c.engine();
    crate::db::reject_catalog(engine, catalog)?;

    let mut columns = Vec::new();
    for sql in schema_columns_sql(engine, catalog, schemas) {
        let res = c.query(&sql)?;
        let (Some(s), Some(t), Some(col)) = (
            col_index(&res, "table_schema"),
            col_index(&res, "table_name"),
            col_index(&res, "column_name"),
        ) else {
            anyhow::bail!("column listing returned an unexpected shape");
        };
        for row in 0..res.row_count() {
            columns.push((
                cell_text(&res, row, s),
                cell_text(&res, row, t),
                cell_text(&res, row, col),
            ));
        }
    }

    let names = fk_result_columns(engine);
    let mut fks = Vec::new();
    for sql in foreign_key_sql(engine, catalog, schemas)? {
        let res = c.query(&sql)?;
        let mut idx = [0usize; 7];
        for (i, name) in names.iter().enumerate() {
            let Some(pos) = col_index(&res, name) else {
                anyhow::bail!("foreign-key listing has no `{name}` column");
            };
            idx[i] = pos;
        }
        for row in 0..res.row_count() {
            let fk = ForeignKey {
                child_schema: cell_text(&res, row, idx[0]),
                child_table: cell_text(&res, row, idx[1]),
                child_column: cell_text(&res, row, idx[2]),
                parent_schema: cell_text(&res, row, idx[3]),
                parent_table: cell_text(&res, row, idx[4]),
                parent_column: cell_text(&res, row, idx[5]),
                constraint: cell_text(&res, row, idx[6]),
            };
            if fk.child_table.is_empty() || fk.parent_table.is_empty() {
                continue;
            }
            fks.push(fk);
        }
    }

    Ok((columns, fks))
}

/// Assemble the picture from what [`scan`] read. Pure.
///
/// `wanted` is the set of `schema.table` labels to draw. `None` means "the
/// tables that take part in a foreign key", which is the useful default: a
/// grid of boxes with no lines between them is an inventory, not a map.
pub fn build_db_map(
    columns: &[ColumnRow],
    fks: &[ForeignKey],
    wanted: Option<&HashSet<String>>,
    max_tables: usize,
) -> DbMap {
    let linked: HashSet<String> = fks.iter().flat_map(|f| [f.child(), f.parent()]).collect();

    // Columns per table, in catalog order, which is the order the boxes list
    // them in and the order the line anchors address.
    let mut order: Vec<String> = Vec::new();
    let mut cols: HashMap<String, Vec<String>> = HashMap::new();
    for (schema, table, column) in columns {
        let label = format!("{schema}.{table}");
        let keep = match wanted {
            Some(w) => w.contains(&label),
            None => linked.contains(&label),
        };
        if !keep {
            continue;
        }
        let entry = cols.entry(label.clone()).or_insert_with(|| {
            order.push(label.clone());
            Vec::new()
        });
        entry.push(column.clone());
    }

    let truncated = order.len() > max_tables;
    order.truncate(max_tables);

    let index: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();
    let nodes: Vec<TableNode> = order
        .iter()
        .map(|label| TableNode {
            name: label.clone(),
            columns: cols.get(label).cloned().unwrap_or_default(),
            // A row count would be a COUNT(*) per box for a number the map
            // does not draw.
            rows: 0,
        })
        .collect();

    let mut edges = Vec::new();
    let mut skipped_edges = 0usize;
    for fk in fks {
        let (Some(&lt), Some(&rt)) = (
            index.get(fk.child().as_str()),
            index.get(fk.parent().as_str()),
        ) else {
            skipped_edges += 1;
            continue;
        };
        let (Some(lc), Some(rc)) = (
            nodes[lt]
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&fk.child_column)),
            nodes[rt]
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&fk.parent_column)),
        ) else {
            skipped_edges += 1;
            continue;
        };
        edges.push(Relationship {
            left_table: lt,
            left_col: lc,
            right_table: rt,
            right_col: rc,
            // Nothing was measured. `scored` is what says so; the numbers stay
            // zero until `rel_map::score_edges` fills them in.
            score: 0.0,
            overlap: 0.0,
            left_distinct: 0.0,
            right_distinct: 0.0,
            left_orphans: 0,
            left_distinct_values: 0,
            right_orphans: 0,
            right_distinct_values: 0,
            constraint: Some(fk.constraint.clone()),
            scored: false,
        });
    }

    DbMap {
        map: RelMap { nodes, edges },
        skipped_edges,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fk(child: &str, ccol: &str, parent: &str, pcol: &str, name: &str) -> ForeignKey {
        let (cs, ct) = child.split_once('.').expect("schema.table");
        let (ps, pt) = parent.split_once('.').expect("schema.table");
        ForeignKey {
            child_schema: cs.into(),
            child_table: ct.into(),
            child_column: ccol.into(),
            parent_schema: ps.into(),
            parent_table: pt.into(),
            parent_column: pcol.into(),
            constraint: name.into(),
        }
    }

    fn cols(spec: &[(&str, &[&str])]) -> Vec<ColumnRow> {
        let mut out = Vec::new();
        for (table, columns) in spec {
            let (s, t) = table.split_once('.').expect("schema.table");
            for c in *columns {
                out.push((s.to_string(), t.to_string(), (*c).to_string()));
            }
        }
        out
    }

    #[test]
    fn draws_only_the_linked_tables_by_default() {
        let columns = cols(&[
            ("public.orders", &["id", "customer_id"]),
            ("public.customers", &["id", "name"]),
            ("public.audit_log", &["id", "message"]),
        ]);
        let fks = vec![fk(
            "public.orders",
            "customer_id",
            "public.customers",
            "id",
            "fk_orders_customer",
        )];
        let out = build_db_map(&columns, &fks, None, 30);
        let names: Vec<&str> = out.map.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["public.orders", "public.customers"]);
        assert_eq!(out.map.edges.len(), 1);
        let e = &out.map.edges[0];
        assert_eq!(e.constraint.as_deref(), Some("fk_orders_customer"));
        assert!(!e.scored, "a declared key arrives unmeasured");
        // The anchors address the column list the box shows.
        assert_eq!(
            out.map.nodes[e.left_table].columns[e.left_col],
            "customer_id"
        );
        assert_eq!(out.map.nodes[e.right_table].columns[e.right_col], "id");
    }

    #[test]
    fn an_explicit_table_set_can_add_an_unlinked_table() {
        let columns = cols(&[
            ("public.orders", &["id", "customer_id"]),
            ("public.customers", &["id"]),
            ("public.audit_log", &["id"]),
        ]);
        let fks = vec![fk(
            "public.orders",
            "customer_id",
            "public.customers",
            "id",
            "fk1",
        )];
        let wanted: HashSet<String> = ["public.orders".to_string(), "public.audit_log".to_string()]
            .into_iter()
            .collect();
        let out = build_db_map(&columns, &fks, Some(&wanted), 30);
        let names: Vec<&str> = out.map.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["public.orders", "public.audit_log"]);
        // customers is not drawn, so its line has nothing to attach to.
        assert!(out.map.edges.is_empty());
        assert_eq!(out.skipped_edges, 1);
    }

    #[test]
    fn a_key_pointing_outside_the_scanned_schemas_is_counted_not_dropped_silently() {
        let columns = cols(&[("sales.orders", &["id", "customer_id"])]);
        let fks = vec![fk(
            "sales.orders",
            "customer_id",
            "crm.customers",
            "id",
            "fk_cross",
        )];
        let out = build_db_map(&columns, &fks, None, 30);
        assert!(out.map.edges.is_empty());
        assert_eq!(out.skipped_edges, 1);
    }

    #[test]
    fn a_composite_key_becomes_one_edge_per_column_pair() {
        let columns = cols(&[
            ("public.lines", &["order_id", "region", "qty"]),
            ("public.orders", &["id", "region"]),
        ]);
        let fks = vec![
            fk("public.lines", "order_id", "public.orders", "id", "fk_line"),
            fk(
                "public.lines",
                "region",
                "public.orders",
                "region",
                "fk_line",
            ),
        ];
        let out = build_db_map(&columns, &fks, None, 30);
        assert_eq!(out.map.edges.len(), 2);
        assert!(
            out.map
                .edges
                .iter()
                .all(|e| e.constraint.as_deref() == Some("fk_line")),
            "both halves name the same constraint"
        );
    }

    #[test]
    fn the_table_cap_truncates_and_says_so() {
        let spec: Vec<(String, Vec<&str>)> = (0..5)
            .map(|i| (format!("public.t{i}"), vec!["id", "other_id"]))
            .collect();
        let columns: Vec<ColumnRow> = spec
            .iter()
            .flat_map(|(t, cs)| {
                let (s, name) = t.split_once('.').unwrap();
                cs.iter()
                    .map(move |c| (s.to_string(), name.to_string(), (*c).to_string()))
            })
            .collect();
        let fks: Vec<ForeignKey> = (1..5)
            .map(|i| {
                fk(
                    &format!("public.t{i}"),
                    "other_id",
                    "public.t0",
                    "id",
                    &format!("fk{i}"),
                )
            })
            .collect();
        let out = build_db_map(&columns, &fks, None, 3);
        assert_eq!(out.map.nodes.len(), 3);
        assert!(out.truncated);
    }

    #[test]
    fn clickhouse_is_refused_rather_than_asked() {
        let err = foreign_key_sql(DbEngine::ClickHouse, None, &["default".into()])
            .expect_err("ClickHouse has no foreign keys");
        assert!(format!("{err}").contains("no foreign keys"), "{err}");
    }

    #[test]
    fn every_other_engine_produces_at_least_one_statement() {
        let schemas = vec!["pub'lic".to_string()];
        for engine in DbEngine::ALL.iter().copied() {
            if !engine.has_foreign_keys() {
                continue;
            }
            let catalog = engine.has_catalogs().then_some("cat");
            let stmts = foreign_key_sql(engine, catalog, &schemas)
                .unwrap_or_else(|e| panic!("{engine:?}: {e}"));
            assert!(!stmts.is_empty(), "{engine:?} produced no statement");
            // The schema name survives, escaped for the context it lands in:
            // a doubled apostrophe in a string literal, or whatever
            // `quote_ident` does when the engine takes it as an identifier
            // (Snowflake's SHOW and BigQuery's table paths do).
            let quoted = engine.quote_ident("pub'lic");
            for s in &stmts {
                assert!(
                    s.contains("pub''lic") || s.contains(&quoted),
                    "{engine:?} lost or mangled the schema name: {s}"
                );
            }
            assert!(
                !schema_columns_sql(engine, catalog, &schemas).is_empty(),
                "{engine:?} produced no column listing"
            );
        }
    }

    #[test]
    fn snowflake_and_bigquery_ask_once_per_schema() {
        let schemas = vec!["a".to_string(), "b".to_string()];
        for engine in [DbEngine::Snowflake, DbEngine::BigQuery] {
            let stmts = foreign_key_sql(engine, Some("cat"), &schemas).expect("sql");
            assert_eq!(stmts.len(), 2, "{engine:?} should ask per schema");
        }
        // The rest answer for every schema in one statement.
        for engine in [DbEngine::Postgres, DbEngine::MySql, DbEngine::Mssql] {
            let stmts = foreign_key_sql(engine, None, &schemas).expect("sql");
            assert_eq!(stmts.len(), 1, "{engine:?} should ask once");
        }
    }

    #[test]
    fn mssql_reads_sys_views_because_its_information_schema_cannot_pair_composites() {
        let sql = &foreign_key_sql(DbEngine::Mssql, None, &["dbo".into()]).expect("sql")[0];
        assert!(sql.contains("sys.foreign_key_columns"), "{sql}");
        assert!(!sql.contains("position_in_unique_constraint"), "{sql}");
    }

    #[test]
    fn mysql_needs_no_join_because_the_referenced_side_is_on_the_same_view() {
        let sql = &foreign_key_sql(DbEngine::MySql, None, &["app".into()]).expect("sql")[0];
        assert!(sql.contains("referenced_table_name IS NOT NULL"), "{sql}");
        assert!(!sql.contains("JOIN"), "{sql}");
    }
}
