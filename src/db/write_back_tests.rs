use super::*;
use crate::data::DbRowMeta;
use std::collections::HashMap;

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    }
}

/// A two-column PK'd table: id (PK) + name, three baseline rows.
fn base_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![col("id", "Int64"), col("name", "Utf8")];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("a".into())],
        vec![CellValue::Int(2), CellValue::String("b".into())],
        vec![CellValue::Int(3), CellValue::String("c".into())],
    ];
    let mut original = HashMap::new();
    for (i, row) in t.rows.iter().enumerate() {
        original.insert(i as i64, row.clone());
    }
    t.db_meta = Some(DbRowMeta {
        table_name: "t".into(),
        schema: Some("public".into()),
        row_tags: vec![Some(0), Some(1), Some(2)],
        original,
        original_columns: vec!["id".into(), "name".into()],
    });
    t
}

fn pk() -> RowIdentity {
    RowIdentity::Key(vec!["id".into()])
}

/// Streaming a DB tab past the memory ceiling evicts front rows. If
/// `evict_front_rows` dropped only the tags, every evicted row would look
/// like a row the user deleted and the next Save would DELETE it on the
/// server. This is that guard: eviction must leave the plan empty.
#[test]
fn evicting_front_rows_does_not_turn_them_into_deletes() {
    let mut t = base_table();
    t.evict_front_rows(2);
    let meta = t.db_meta.as_ref().expect("meta survives eviction");
    assert_eq!(meta.row_tags.len(), t.rows.len());
    assert_eq!(meta.row_tags, vec![Some(2)]);
    assert_eq!(meta.original.len(), 1, "evicted baselines are dropped too");

    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert!(
        plan.deletes.is_empty(),
        "evicted rows must not be deleted on the server, got {:?}",
        plan.deletes
    );
    assert!(plan.updates.is_empty());
    assert!(plan.inserts.is_empty());
}

/// An edit to a row that survived the eviction still addresses the right
/// server row: the baseline for the remaining tag is untouched.
#[test]
fn an_edit_after_eviction_still_addresses_its_own_row() {
    let mut t = base_table();
    t.evict_front_rows(2);
    t.set(0, 1, CellValue::String("c2".into()));
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert!(plan.deletes.is_empty());
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.updates[0].0, vec![CellValue::Int(3)]);
}

/// A connector that records the SQL it was given and reports a fixed
/// affected-row count, so the guard can be exercised without a server.
struct CountingConnector {
    affected: u64,
    statements: Vec<String>,
}

impl DbConnector for CountingConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Postgres
    }
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn query(&mut self, _: &str) -> anyhow::Result<DataTable> {
        // The conflict re-read: the server still holds the baseline rows.
        let mut server = base_table();
        server.db_meta = None;
        Ok(server)
    }
    fn execute(&mut self, sql: &str) -> anyhow::Result<u64> {
        self.statements.push(sql.to_string());
        // Transaction control is not a row-affecting statement.
        if matches!(sql, "BEGIN" | "BEGIN TRANSACTION" | "COMMIT" | "ROLLBACK") {
            return Ok(0);
        }
        Ok(self.affected)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: super::super::DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<super::super::DbWriteReport> {
        unimplemented!("not used by these tests")
    }
}

fn full_row() -> RowIdentity {
    RowIdentity::FullRow(vec!["id".into(), "name".into()])
}

fn one_update_plan() -> DbWriteBackPlan {
    let mut t = base_table();
    t.rows[1][1] = CellValue::String("B".into());
    build_write_back_plan(&t, &full_row()).unwrap()
}

/// Without a key the WHERE clause carries every baseline value, which is
/// what lets a keyless table be edited at all.
#[test]
fn a_full_row_match_addresses_the_row_by_all_its_values() {
    let plan = one_update_plan();
    assert_eq!(plan.updates.len(), 1);
    let (where_vals, _) = &plan.updates[0];
    assert_eq!(
        where_vals,
        &vec![CellValue::Int(2), CellValue::String("b".into())],
        "the WHERE uses the ORIGINAL values, not the edited ones"
    );
    let sql = update_sql(
        DbEngine::Postgres,
        "public",
        "t",
        &base_table().columns,
        full_row().columns(),
        where_vals,
        &plan.updates[0].1,
    );
    assert!(sql.contains(r#"WHERE "id" = 2 AND "name" = 'b'"#), "{sql}");
}

/// `= NULL` is never true, so a baseline NULL has to become `IS NULL` or
/// the statement would match nothing and the save would silently no-op.
#[test]
fn a_null_in_the_baseline_is_matched_with_is_null() {
    let sql = pk_where_sql(
        DbEngine::Postgres,
        &["id".to_string(), "note".to_string()],
        &[CellValue::Int(1), CellValue::Null],
    );
    assert_eq!(sql, r#""id" = 1 AND "note" IS NULL"#);
}

/// A key column cannot be NULL, so a NULL there means the baseline is
/// wrong and the row must not be addressed by it.
#[test]
fn a_null_key_value_is_still_refused() {
    let mut t = base_table();
    t.db_meta.as_mut().unwrap().original.get_mut(&1).unwrap()[0] = CellValue::Null;
    t.rows[1][1] = CellValue::String("B".into());
    let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
    assert!(err.contains("NULL"), "{err}");
    // The same baseline is fine for a full-row match.
    assert!(build_write_back_plan(&t, &full_row()).is_ok());
}

#[test]
fn a_full_row_update_that_touches_exactly_one_row_commits() {
    let mut c = CountingConnector {
        affected: 1,
        statements: vec![],
    };
    let plan = one_update_plan();
    let report = apply_write_back(
        &mut c,
        DbEngine::Postgres,
        "public",
        "t",
        &base_table().columns,
        &full_row(),
        &plan,
    )
    .expect("one matched row commits");
    assert_eq!(report.updated, 1);
    assert!(c.statements.contains(&"COMMIT".to_string()));
}

/// Duplicate rows are the case this whole mode has to survive: the WHERE
/// matches both, and rewriting both is exactly the data loss the guard
/// exists to prevent.
#[test]
fn a_full_row_update_matching_two_rows_rolls_back() {
    let mut c = CountingConnector {
        affected: 2,
        statements: vec![],
    };
    let plan = one_update_plan();
    let err = apply_write_back(
        &mut c,
        DbEngine::Postgres,
        "public",
        "t",
        &base_table().columns,
        &full_row(),
        &plan,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("2 rows"), "{err}");
    assert!(c.statements.contains(&"ROLLBACK".to_string()));
    assert!(!c.statements.contains(&"COMMIT".to_string()));
}

/// Zero means someone else changed or removed the row since the tab
/// loaded. Silently writing nothing and reporting success would be worse.
#[test]
fn a_full_row_update_matching_no_row_rolls_back() {
    let mut c = CountingConnector {
        affected: 0,
        statements: vec![],
    };
    let plan = one_update_plan();
    let err = apply_write_back(
        &mut c,
        DbEngine::Postgres,
        "public",
        "t",
        &base_table().columns,
        &full_row(),
        &plan,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("no row"), "{err}");
    assert!(c.statements.contains(&"ROLLBACK".to_string()));
}

/// The count is NOT checked for a keyed table: MySQL reports *changed*
/// rather than *matched* rows, so a correct keyed update can legitimately
/// report 0 and must not be rolled back.
#[test]
fn a_keyed_update_is_not_subject_to_the_count_check() {
    let mut c = CountingConnector {
        affected: 0,
        statements: vec![],
    };
    let mut t = base_table();
    t.rows[1][1] = CellValue::String("B".into());
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    let report = apply_write_back(
        &mut c,
        DbEngine::Postgres,
        "public",
        "t",
        &base_table().columns,
        &pk(),
        &plan,
    )
    .expect("a keyed write is unaffected by the affected-row count");
    assert_eq!(report.updated, 1);
    assert!(c.statements.contains(&"COMMIT".to_string()));
}

#[test]
fn unchanged_table_yields_an_empty_plan() {
    let plan = build_write_back_plan(&base_table(), &pk()).unwrap();
    assert!(plan.is_empty());
}

#[test]
fn detects_update_insert_delete() {
    let mut t = base_table();
    // Edit row 1's name, delete row 2, insert a new row.
    t.rows[1][1] = CellValue::String("B".into());
    t.rows.remove(2);
    t.rows
        .push(vec![CellValue::Int(9), CellValue::String("z".into())]);
    let meta = t.db_meta.as_mut().unwrap();
    meta.row_tags = vec![Some(0), Some(1), None];

    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.deletes, vec![vec![CellValue::Int(3)]]);
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.updates[0].0, vec![CellValue::Int(2)]);
    assert_eq!(plan.updates[0].1[1], CellValue::String("B".into()));
    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(plan.inserts[0][0], CellValue::Int(9));
    assert_eq!(
        plan.deletes.len() + plan.updates.len() + plan.inserts.len(),
        3
    );
}

#[test]
fn edited_pk_cell_is_located_by_its_original_value() {
    let mut t = base_table();
    t.rows[0][0] = CellValue::Int(100);
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    // WHERE uses the ORIGINAL pk (1); SET writes the new one (100).
    assert_eq!(plan.updates[0].0, vec![CellValue::Int(1)]);
    assert_eq!(plan.updates[0].1[0], CellValue::Int(100));
}

#[test]
fn added_column_forces_full_row_updates() {
    let mut t = base_table();
    t.columns.push(col("extra", "Utf8"));
    for row in &mut t.rows {
        row.push(CellValue::String("x".into()));
    }
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.added_columns.len(), 1);
    assert_eq!(plan.updates.len(), 3, "every row rewritten");
}

/// A hash column the database computes: present (with a cell typed
/// into it) or renamed away, it never reaches the plan; a real edit
/// beside it still does.
#[test]
fn dropped_columns_never_reach_the_plan() {
    let mut t = base_table();
    let meta = t.db_meta.as_mut().unwrap();
    meta.original_columns.push("h".into());
    for row in meta.original.values_mut() {
        row.push(CellValue::String("x".into()));
    }
    t.columns.push(col("h", "Utf8"));
    for row in &mut t.rows {
        row.push(CellValue::String("x".into()));
    }
    t.rows[0][2] = CellValue::String("typed".into());
    let mut clean = t.clone();
    drop_columns(&mut clean, &["h".into()]);
    assert!(build_write_back_plan(&clean, &pk()).unwrap().is_empty());

    let mut renamed = t.clone();
    renamed.columns[2].name = "mine".into();
    drop_columns(&mut renamed, &["h".into()]);
    let plan = build_write_back_plan(&renamed, &pk()).unwrap();
    assert_eq!(
        plan.added_columns.len(),
        1,
        "a renamed hash is the user's column"
    );

    t.rows[1][1] = CellValue::String("B".into());
    drop_columns(&mut t, &["h".into()]);
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(
        plan.updates[0].1,
        vec![CellValue::Int(2), CellValue::String("B".into())]
    );
}

#[test]
fn removed_column_is_rejected() {
    let mut t = base_table();
    t.columns.remove(1);
    for row in &mut t.rows {
        row.remove(1);
    }
    let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
    assert!(err.contains("removed or renamed"), "{err}");
}

#[test]
fn null_pk_value_is_rejected() {
    let mut t = base_table();
    t.db_meta
        .as_mut()
        .unwrap()
        .original
        .insert(0, vec![CellValue::Null, CellValue::String("a".into())]);
    t.rows[0][1] = CellValue::String("edited".into());
    let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
    assert!(err.contains("NULL"), "{err}");
}

#[test]
fn missing_db_meta_is_identity_lost() {
    let mut t = base_table();
    t.db_meta = None;
    let err = build_write_back_plan(&t, &pk()).unwrap_err().to_string();
    assert!(err.contains("row identity lost"), "{err}");
}

#[test]
fn sql_rendering_per_engine() {
    let cols = [col("id", "Int64"), col("ok", "Boolean")];
    let identity = pk();
    let pk_cols = identity.columns();
    let pk_vals = [CellValue::Int(7)];
    let row = [CellValue::Int(7), CellValue::Bool(true)];

    assert_eq!(
        delete_sql(DbEngine::Postgres, "public", "t", pk_cols, &pk_vals),
        "DELETE FROM \"public\".\"t\" WHERE \"id\" = 7"
    );
    assert_eq!(
        update_sql(DbEngine::MySql, "app", "t", &cols, pk_cols, &pk_vals, &row),
        "UPDATE `app`.`t` SET `id` = 7, `ok` = TRUE WHERE `id` = 7"
    );
    // MSSQL renders booleans as BIT literals.
    assert_eq!(
        insert_sql(DbEngine::Mssql, "dbo", "t", &cols, &row),
        "INSERT INTO [dbo].[t] ([id], [ok]) VALUES (7, 1)"
    );
    assert_eq!(
        alter_add_sql(DbEngine::Mssql, "dbo", "t", &col("extra", "Utf8")),
        "ALTER TABLE [dbo].[t] ADD [extra] NVARCHAR(MAX)"
    );
    assert_eq!(
        alter_add_sql(DbEngine::Postgres, "public", "t", &col("extra", "Int64")),
        "ALTER TABLE \"public\".\"t\" ADD COLUMN \"extra\" BIGINT"
    );
}

#[test]
fn alter_add_uses_the_same_dialect_as_create() {
    let col = ColumnInfo {
        name: "label".to_string(),
        data_type: "Utf8".to_string(),
    };
    // Snowflake spells it VARCHAR; the Postgres fallback would say TEXT.
    let alter = alter_add_sql(DbEngine::Snowflake, "s", "t", &col);
    assert!(alter.contains("VARCHAR"), "got: {alter}");
    let create = crate::db::create_table_sql(DbEngine::Snowflake, "s", "t", &[col]);
    assert!(create.contains("VARCHAR"), "got: {create}");
}

#[test]
fn renders_plan_as_transactional_script() {
    let cols = vec![col("id", "Int64"), col("name", "Utf8")];
    let plan = DbWriteBackPlan {
        deletes: vec![vec![CellValue::Int(7)]],
        updates: vec![(
            vec![CellValue::Int(3)],
            vec![CellValue::Int(3), CellValue::String("O'Brien".into())],
        )],
        inserts: vec![vec![CellValue::Int(9), CellValue::String("new".into())]],
        added_columns: Vec::new(),
        merge: None,
    };
    let sql = render_plan_sql(
        DbEngine::Postgres,
        "public",
        "people",
        &cols,
        &["id".to_string()],
        &plan,
    );

    assert!(sql.starts_with("BEGIN;\n"), "{sql}");
    assert!(sql.trim_end().ends_with("COMMIT;"), "{sql}");
    let del = sql.find("DELETE FROM").unwrap();
    let upd = sql.find("UPDATE").unwrap();
    let ins = sql.find("INSERT INTO").unwrap();
    assert!(
        del < upd && upd < ins,
        "statements out of apply order:\n{sql}"
    );

    assert!(sql.contains("'O''Brien'"), "quote not doubled:\n{sql}");
}

#[test]
fn renders_added_columns_first_and_mssql_begins_transaction() {
    let cols = vec![col("id", "Int64")];
    let plan = DbWriteBackPlan {
        deletes: Vec::new(),
        updates: Vec::new(),
        inserts: Vec::new(),
        added_columns: vec![col("note", "Utf8")],
        merge: None,
    };
    let sql = render_plan_sql(
        DbEngine::Mssql,
        "dbo",
        "people",
        &cols,
        &["id".to_string()],
        &plan,
    );
    assert!(sql.starts_with("BEGIN TRANSACTION;\n"), "{sql}");
    assert!(sql.contains("ALTER TABLE"), "{sql}");
}

#[test]
fn empty_plan_renders_a_comment_not_an_empty_transaction() {
    let sql = render_plan_sql(
        DbEngine::Postgres,
        "public",
        "people",
        &[],
        &["id".to_string()],
        &DbWriteBackPlan::default(),
    );
    assert!(sql.contains("no changes"), "{sql}");
    assert!(!sql.contains("BEGIN"), "{sql}");
}

#[test]
fn numeric_formatting_alone_is_not_a_change() {
    // A CSV reads 120.50 as a float; Postgres returns the exact text.
    assert!(!cells_differ(
        &CellValue::String("120.50".into()),
        &CellValue::Float(120.5)
    ));
    assert!(!cells_differ(
        &CellValue::Int(88),
        &CellValue::String("88.00".into())
    ));
    assert!(cells_differ(
        &CellValue::Float(120.5),
        &CellValue::Float(120.6)
    ));
    // NULL against a value is a change, not a formatting quirk.
    assert!(cells_differ(&CellValue::Null, &CellValue::Float(1.0)));
    // Text that merely looks numeric stays a text comparison.
    assert!(cells_differ(
        &CellValue::String("007".into()),
        &CellValue::String("7A".into())
    ));
}

#[test]
fn comparison_becomes_deletes_updates_and_inserts() {
    use crate::data::compare::{CompareOptions, compare_join};

    let mut server = DataTable::empty();
    server.columns = vec![col("id", "Int64"), col("city", "Utf8")];
    server.rows = vec![
        vec![CellValue::Int(1), CellValue::String("London".into())],
        vec![CellValue::Int(2), CellValue::String("Baltimore".into())],
        vec![CellValue::Int(3), CellValue::String("Cambridge".into())],
    ];

    let mut desired = DataTable::empty();
    desired.columns = vec![col("id", "Int64"), col("city", "Utf8")];
    desired.rows = vec![
        vec![CellValue::Int(1), CellValue::String("London".into())],
        vec![CellValue::Int(2), CellValue::String("NEWCITY".into())],
        vec![CellValue::Int(5), CellValue::String("Helsinki".into())],
    ];

    let result = compare_join(
        &server,
        &desired,
        &["id".to_string()],
        &CompareOptions::default(),
    )
    .unwrap();
    let synced = plan_from_comparison(&server, &desired, &result, &["id".to_string()]).unwrap();

    assert_eq!(synced.plan.deletes, vec![vec![CellValue::Int(3)]]);
    assert_eq!(synced.plan.inserts.len(), 1);
    // Row 1 is untouched; only row 2 really changed.
    assert_eq!(synced.plan.updates.len(), 1);
    assert_eq!(synced.plan.updates[0].0, vec![CellValue::Int(2)]);
    // Never invents a schema change.
    assert!(synced.plan.added_columns.is_empty());
    assert!(synced.ignored_columns.is_empty());
}

#[test]
fn extra_source_columns_are_reported_not_added() {
    use crate::data::compare::{CompareOptions, compare_join};

    let mut server = DataTable::empty();
    server.columns = vec![col("id", "Int64")];
    server.rows = vec![vec![CellValue::Int(1)]];

    let mut desired = DataTable::empty();
    desired.columns = vec![col("id", "Int64"), col("note", "Utf8")];
    desired.rows = vec![vec![CellValue::Int(1), CellValue::String("hi".into())]];

    let result = compare_join(
        &server,
        &desired,
        &["id".to_string()],
        &CompareOptions::default(),
    )
    .unwrap();
    let synced = plan_from_comparison(&server, &desired, &result, &["id".to_string()]).unwrap();
    assert_eq!(synced.ignored_columns, vec!["note".to_string()]);
    assert!(synced.plan.added_columns.is_empty());
}

#[test]
fn a_key_missing_from_the_server_table_is_an_error() {
    use crate::data::compare::CompareResult;
    let mut server = DataTable::empty();
    server.columns = vec![col("id", "Int64")];
    let desired = server.clone();
    let err = plan_from_comparison(
        &server,
        &desired,
        &CompareResult::default(),
        &["nope".to_string()],
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("nope"), "{err}");
}

// --- edit audit trail SQL export -----------------------------------

/// A plain two-column table (no `db_meta`), matching what the edit audit
/// panel actually has on hand: `DataTable.rows` plus an edit overlay.
fn two_col_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![col("id", "Int64"), col("city", "Utf8")];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("Berlin".into())],
        vec![CellValue::Int(2), CellValue::String("Hamburg".into())],
    ];
    t
}

/// A keyed table exports one real UPDATE per edited row, addressed by the
/// key's ORIGINAL value even when the key cell itself was edited, and
/// never rewrites a column nobody touched.
#[test]
fn a_keyed_table_exports_updates_addressed_by_the_original_key() {
    let mut t = two_col_table();
    t.columns.push(col("country", "Utf8"));
    t.rows[0].push(CellValue::String("Germany".into()));
    t.rows[1].push(CellValue::String("Germany".into()));
    t.edits.insert((0, 0), CellValue::Int(100));
    t.edits
        .insert((0, 1), CellValue::String("Duesseldorf".into()));

    let identity = RowIdentity::Key(vec!["id".into()]);
    let out =
        edits_as_update_sql(&t, DbEngine::Postgres, "", "customers", Some(&identity)).unwrap();
    assert!(!out.is_template, "a keyed table exports real statements");
    assert_eq!(out.statements.len(), 1);
    let sql = &out.statements[0];
    assert!(sql.contains("UPDATE"), "got: {sql}");
    assert!(sql.contains("customers"), "names the target table: {sql}");
    assert!(sql.contains("WHERE"), "targets by key: {sql}");
    assert!(
        sql.contains("\"id\" = 1"),
        "WHERE uses the ORIGINAL key value, not the edited one: {sql}"
    );
    assert!(
        !sql.contains("\"country\""),
        "must not rewrite an unchanged column: {sql}"
    );
    assert!(sql.trim_end().ends_with(';'), "{sql}");
}

/// A key column that is not in the table at all is refused, naming it.
#[test]
fn a_missing_key_column_is_an_error_naming_it() {
    let t = two_col_table();
    let identity = RowIdentity::Key(vec!["nope".into()]);
    let err = edits_as_update_sql(&t, DbEngine::Postgres, "", "customers", Some(&identity))
        .unwrap_err()
        .to_string();
    assert!(err.contains("nope"), "{err}");
}

/// A file-backed table has no key, so the export is flagged as a
/// template keyed on the first column, named after the source file.
#[test]
fn a_file_backed_table_exports_a_flagged_template() {
    let mut t = two_col_table();
    t.source_path = Some("/tmp/sales_2026.csv".into());
    t.edits
        .insert((0, 1), CellValue::String("Duesseldorf".into()));

    let out = edits_as_update_sql(&t, DbEngine::Postgres, "", "sales_2026", None).unwrap();
    assert!(
        out.is_template,
        "no identity means no key, so it is a template"
    );
    assert_eq!(out.statements.len(), 1);
    assert!(
        out.statements[0].contains("sales_2026"),
        "table name from the file stem: {}",
        out.statements[0]
    );
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|n| n.to_string()).collect()
}

#[test]
fn merge_row_follows_the_three_way_rules() {
    let cols = names(&["id", "a", "b"]);
    let base = vec![CellValue::Int(1), s("a0"), s("b0")];
    // Only I changed a; only they changed b: both kept.
    let merged = merge_row(
        &cols,
        &cols,
        &base,
        &[CellValue::Int(1), s("a0"), s("bT")],
        &[CellValue::Int(1), s("aM"), s("b0")],
    )
    .unwrap();
    assert_eq!(merged, vec![CellValue::Int(1), s("aM"), s("bT")]);
    // Both changed a to the same value: agreed.
    let merged = merge_row(
        &cols,
        &cols,
        &base,
        &[CellValue::Int(1), s("aX"), s("b0")],
        &[CellValue::Int(1), s("aX"), s("b0")],
    )
    .unwrap();
    assert_eq!(merged[1], s("aX"));
    // Both changed a differently: a conflict naming the cell.
    let err = merge_row(
        &cols,
        &cols,
        &base,
        &[CellValue::Int(1), s("aT"), s("b0")],
        &[CellValue::Int(1), s("aM"), s("b0")],
    )
    .unwrap_err();
    assert_eq!(
        err,
        vec![CellConflict {
            column: "a".into(),
            base: s("a0"),
            theirs: s("aT"),
            mine: s("aM")
        }]
    );
}

#[test]
fn merge_row_treats_null_as_a_value() {
    let cols = names(&["id", "a"]);
    let base = vec![CellValue::Int(1), CellValue::Null];
    let merged = merge_row(
        &cols,
        &cols,
        &base,
        &[CellValue::Int(1), s("x")],
        &[CellValue::Int(1), CellValue::Null],
    )
    .unwrap();
    assert_eq!(merged[1], s("x"), "their fill of an empty cell is kept");
}

#[test]
fn merge_row_maps_reordered_and_added_columns_by_name() {
    let base_cols = names(&["id", "a", "b"]);
    let now = names(&["b", "id", "new", "a"]);
    let base = vec![CellValue::Int(1), s("a0"), s("b0")];
    let theirs = vec![CellValue::Int(1), s("a0"), s("bT")];
    let mine = vec![s("b0"), CellValue::Int(1), s("n"), s("aM")];
    let merged = merge_row(&base_cols, &now, &base, &theirs, &mine).unwrap();
    assert_eq!(merged, vec![s("bT"), CellValue::Int(1), s("n"), s("aM")]);
}

/// Plan for `base_table()` with row id=2's name edited and id=3 deleted.
fn edited_plan() -> (DataTable, DbWriteBackPlan) {
    let mut t = base_table();
    t.rows[1][1] = s("B");
    t.rows.remove(2);
    t.db_meta.as_mut().unwrap().row_tags.remove(2);
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    (t, plan)
}

fn server(rows: &[Vec<CellValue>]) -> HashMap<String, Vec<CellValue>> {
    rows.iter()
        .map(|r| (key_text(&r[..1]), r.clone()))
        .collect()
}

#[test]
fn the_plan_carries_merge_info_for_keyed_edits() {
    let (_, plan) = edited_plan();
    let info = plan.merge.as_ref().expect("merge info");
    assert_eq!(
        info.update_bases,
        vec![(1, vec![CellValue::Int(2), s("b")])]
    );
    assert_eq!(
        info.delete_bases,
        vec![(2, vec![CellValue::Int(3), s("c")])]
    );
}

#[test]
fn an_untouched_server_merges_cleanly() {
    let (_, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(1), s("a")],
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert!(out.conflicts.is_empty());
    assert!(out.merged.is_empty());
    assert_eq!(out.updates, plan.updates);
    assert_eq!(out.deletes, plan.deletes);
}

#[test]
fn a_row_deleted_on_the_server_conflicts_with_my_edit() {
    let (_, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[vec![CellValue::Int(3), s("c")]]),
    );
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(out.conflicts[0].kind, ConflictKind::DeletedOnServer);
    assert_eq!(out.conflicts[0].tag, 1);
}

#[test]
fn deleting_a_row_that_is_already_gone_is_not_a_conflict() {
    let (_, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[vec![CellValue::Int(2), s("b")]]),
    );
    assert!(out.conflicts.is_empty());
    assert!(out.deletes.is_empty(), "nothing left to delete");
    assert_eq!(out.gone, vec![2]);
}

#[test]
fn deleting_a_row_someone_changed_is_a_conflict() {
    let (_, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c-theirs")],
        ]),
    );
    assert_eq!(
        out.conflicts[0].kind,
        ConflictKind::ChangedOnServer {
            theirs: vec![CellValue::Int(3), s("c-theirs")]
        }
    );
}

#[test]
fn take_theirs_rebases_the_row_and_drops_my_cell() {
    let (mut t, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert_eq!(out.conflicts.len(), 1);
    apply_resolutions(&mut t, &out.conflicts, &[Resolution::TakeTheirs]);
    assert_eq!(t.get(1, 1), Some(&s("b-theirs")));
    assert_eq!(
        t.db_meta.as_ref().unwrap().original[&1],
        vec![CellValue::Int(2), s("b-theirs")]
    );
    assert!(
        t.undo_stack.is_empty() && t.redo_stack.is_empty(),
        "history cleared"
    );
    let mut snap = t.clone();
    snap.apply_edits();
    let again = build_write_back_plan(&snap, &pk()).unwrap();
    assert!(
        again.updates.is_empty(),
        "nothing of mine left to write for that row"
    );
}

#[test]
fn keep_mine_rebases_so_the_next_save_writes_my_value() {
    let (mut t, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    apply_resolutions(&mut t, &out.conflicts, &[Resolution::KeepMine]);
    let mut snap = t.clone();
    snap.apply_edits();
    let again = build_write_back_plan(&snap, &pk()).unwrap();
    assert_eq!(again.updates[0].1, vec![CellValue::Int(2), s("B")]);
    // Re-checked against the same server row, it now merges cleanly.
    let info = again.merge.as_ref().unwrap();
    let out2 = merge_plan(
        &again,
        info,
        &server(&[
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert!(out2.conflicts.is_empty());
}

#[test]
fn add_it_back_turns_my_row_into_an_insert() {
    let (mut t, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[vec![CellValue::Int(3), s("c")]]),
    );
    apply_resolutions(&mut t, &out.conflicts, &[Resolution::KeepMine]);
    let again = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(again.inserts, vec![vec![CellValue::Int(2), s("B")]]);
    assert!(again.updates.is_empty());
}

#[test]
fn let_it_go_removes_the_row_without_planning_a_delete() {
    let (mut t, plan) = edited_plan();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[vec![CellValue::Int(3), s("c")]]),
    );
    apply_resolutions(&mut t, &out.conflicts, &[Resolution::TakeTheirs]);
    assert_eq!(t.row_count(), 1);
    let again = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(
        again.deletes,
        vec![vec![CellValue::Int(3)]],
        "only the delete I made myself"
    );
}

#[test]
fn keep_theirs_brings_a_deleted_row_back() {
    let (mut t, plan) = edited_plan();
    let theirs = vec![CellValue::Int(3), s("c-theirs")];
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[vec![CellValue::Int(2), s("b")], theirs.clone()]),
    );
    let choices: Vec<Resolution> = out
        .conflicts
        .iter()
        .map(|_| Resolution::TakeTheirs)
        .collect();
    apply_resolutions(&mut t, &out.conflicts, &choices);
    assert!(
        t.rows.iter().any(|r| r == &theirs),
        "row is back with their values"
    );
    let again = build_write_back_plan(&t, &pk()).unwrap();
    assert!(again.deletes.is_empty());
}

fn nan() -> CellValue {
    CellValue::Float(f64::NAN)
}

#[test]
fn nan_on_all_three_sides_merges_cleanly() {
    let cols = names(&["id", "x"]);
    let row = vec![CellValue::Int(1), nan()];
    assert_eq!(merge_row(&cols, &cols, &row, &row, &row).unwrap().len(), 2);
}

#[test]
fn an_untouched_nan_row_plans_no_update() {
    let mut t = base_table();
    t.rows[0][1] = nan();
    t.db_meta
        .as_mut()
        .unwrap()
        .original
        .insert(0, t.rows[0].clone());
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert!(plan.updates.is_empty());
}

#[test]
fn a_colleague_changing_another_cell_merges_and_reports_it() {
    let cols = names(&["id", "a", "b"]);
    let mut t = DataTable::empty();
    t.columns = vec![col("id", "Int64"), col("a", "Utf8"), col("b", "Utf8")];
    t.rows = vec![vec![CellValue::Int(1), s("a0"), s("b0")]];
    let mut original = HashMap::new();
    original.insert(0, t.rows[0].clone());
    t.db_meta = Some(DbRowMeta {
        table_name: "t".into(),
        schema: None,
        row_tags: vec![Some(0)],
        original,
        original_columns: cols,
    });
    t.rows[0][1] = s("aM");
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    let theirs = vec![CellValue::Int(1), s("a0"), s("bT")];
    let out = merge_plan(&plan, plan.merge.as_ref().unwrap(), &server(&[theirs]));
    let want = vec![CellValue::Int(1), s("aM"), s("bT")];
    assert_eq!(out.updates[0].1, want);
    assert_eq!(out.merged, vec![(0, want)]);
}

#[test]
fn an_edited_key_cell_is_looked_up_by_the_original_key() {
    let (mut t, _) = edited_plan();
    t.rows[1][0] = CellValue::Int(20);
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.updates[0].0, vec![CellValue::Int(2)]);
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert!(out.conflicts.is_empty());
    assert_eq!(out.updates[0].0, vec![CellValue::Int(2)]);
}

#[test]
fn an_unedited_row_forced_by_added_columns_and_deleted_by_a_colleague_is_dropped() {
    let mut t = base_table();
    t.insert_column(2, "note".into(), "Utf8".into());
    t.apply_edits();
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.updates.len(), 3);
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert!(out.conflicts.is_empty());
    assert_eq!(out.updates.len(), 2);
    assert_eq!(out.gone, vec![0]);
}

#[test]
fn a_note_typed_into_an_added_column_of_a_deleted_row_is_a_conflict() {
    let mut t = base_table();
    t.insert_column(2, "note".into(), "Utf8".into());
    t.set(0, 2, s("keep me"));
    t.apply_edits();
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(out.conflicts[0].kind, ConflictKind::DeletedOnServer);
    assert!(out.gone.is_empty());
}

#[test]
fn a_reordered_unedited_table_plans_no_updates() {
    let mut t = base_table();
    t.columns.swap(0, 1);
    for r in &mut t.rows {
        r.swap(0, 1);
    }
    let plan = build_write_back_plan(&t, &RowIdentity::Key(vec!["id".into()])).unwrap();
    assert!(plan.updates.is_empty());
}

#[test]
fn a_stale_conflict_does_not_rebase_another_row() {
    let (mut t, plan) = edited_plan();
    let mut out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    out.conflicts[0].base = vec![CellValue::Int(2), s("other")];
    apply_resolutions(&mut t, &out.conflicts, &[Resolution::TakeTheirs]);
    assert_eq!(
        t.db_meta.as_ref().unwrap().original[&1],
        vec![CellValue::Int(2), s("b")]
    );
}

/// A connector that answers every SELECT with `server` and records SQL.
struct ScriptedConnector {
    engine: DbEngine,
    server: DataTable,
    /// Rows the server holds from the second SELECT on (someone changed
    /// them in between), when set.
    later: Option<Vec<Vec<CellValue>>>,
    statements: Vec<String>,
}

impl DbConnector for ScriptedConnector {
    fn engine(&self) -> DbEngine {
        self.engine
    }
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        self.statements.push(sql.to_string());
        let answer = self.server.clone();
        if let Some(rows) = self.later.take() {
            self.server.rows = rows;
        }
        Ok(answer)
    }
    fn execute(&mut self, sql: &str) -> anyhow::Result<u64> {
        self.statements.push(sql.to_string());
        Ok(1)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: super::super::DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<super::super::DbWriteReport> {
        unimplemented!("not used by these tests")
    }
}

fn scripted(engine: DbEngine, rows: Vec<Vec<CellValue>>) -> ScriptedConnector {
    let mut server = DataTable::empty();
    server.columns = vec![col("id", "Int64"), col("name", "Utf8")];
    server.rows = rows;
    ScriptedConnector {
        engine,
        server,
        later: None,
        statements: vec![],
    }
}

fn apply(
    c: &mut ScriptedConnector,
    t: &DataTable,
    plan: &DbWriteBackPlan,
) -> anyhow::Result<DbWriteBackReport> {
    let engine = c.engine;
    apply_write_back(c, engine, "public", "t", &t.columns, &pk(), plan)
}

#[test]
fn an_untouched_server_is_read_under_lock_then_updated() {
    let (t, plan) = edited_plan();
    let mut c = scripted(
        DbEngine::Postgres,
        vec![
            vec![CellValue::Int(2), s("b")],
            vec![CellValue::Int(3), s("c")],
        ],
    );
    let report = apply(&mut c, &t, &plan).unwrap();
    assert!(report.merged.is_empty());
    let select = c
        .statements
        .iter()
        .find(|q| q.starts_with("SELECT"))
        .expect("a re-read");
    assert!(select.ends_with("FOR UPDATE"), "{select}");
    assert!(c.statements.iter().any(|q| q.starts_with("UPDATE")));
    assert_eq!(c.statements.last().unwrap(), "COMMIT");
}

#[test]
fn their_change_to_another_cell_is_written_along_with_mine() {
    let mut t = base_table();
    t.columns.push(col("note", "Utf8"));
    for r in &mut t.rows {
        r.push(s("n0"));
    }
    let meta = t.db_meta.as_mut().unwrap();
    meta.original_columns.push("note".into());
    for row in meta.original.values_mut() {
        row.push(s("n0"));
    }
    t.rows[1][1] = s("B");
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    let mut c = scripted(
        DbEngine::Postgres,
        vec![vec![CellValue::Int(2), s("b"), s("n-theirs")]],
    );
    c.server.columns.push(col("note", "Utf8"));
    let report = apply(&mut c, &t, &plan).unwrap();
    assert_eq!(
        report.merged,
        vec![(1, vec![CellValue::Int(2), s("B"), s("n-theirs")])]
    );
    let update = c
        .statements
        .iter()
        .find(|q| q.starts_with("UPDATE"))
        .unwrap();
    assert!(
        update.contains("'B'") && update.contains("'n-theirs'"),
        "{update}"
    );
}

#[test]
fn a_conflict_rolls_back_and_writes_nothing() {
    let (t, plan) = edited_plan();
    let mut c = scripted(
        DbEngine::Postgres,
        vec![
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ],
    );
    let err = apply(&mut c, &t, &plan).unwrap_err();
    let conflicts = err
        .downcast_ref::<WriteConflicts>()
        .expect("typed conflicts");
    assert_eq!(conflicts.conflicts.len(), 1);
    assert!(!conflicts.columns_added);
    assert!(
        !c.statements
            .iter()
            .any(|q| q.starts_with("UPDATE") || q.starts_with("DELETE"))
    );
    assert_eq!(c.statements.last().unwrap(), "ROLLBACK");
}

#[test]
fn sql_server_locks_with_a_table_hint_and_redshift_reads_plainly() {
    let (t, plan) = edited_plan();
    let rows = vec![
        vec![CellValue::Int(2), s("b")],
        vec![CellValue::Int(3), s("c")],
    ];
    let mut ms = scripted(DbEngine::Mssql, rows.clone());
    apply(&mut ms, &t, &plan).unwrap();
    let select = ms
        .statements
        .iter()
        .find(|q| q.starts_with("SELECT"))
        .unwrap();
    assert!(select.contains("WITH (UPDLOCK, ROWLOCK)"), "{select}");
    let mut rs = scripted(DbEngine::Redshift, rows);
    apply(&mut rs, &t, &plan).unwrap();
    let select = rs
        .statements
        .iter()
        .find(|q| q.starts_with("SELECT"))
        .unwrap();
    assert!(
        !select.contains("FOR UPDATE") && !select.contains("UPDLOCK"),
        "{select}"
    );
}

#[test]
fn a_plan_without_merge_info_is_not_re_read() {
    let (t, mut plan) = edited_plan();
    plan.merge = None;
    let mut c = scripted(DbEngine::Postgres, vec![]);
    apply(&mut c, &t, &plan).unwrap();
    assert!(!c.statements.iter().any(|q| q.starts_with("SELECT")));
}

#[test]
fn the_re_read_asks_for_at_most_a_batch_of_keys_at_a_time() {
    let mut t = base_table();
    for i in 3..250i64 {
        let row = vec![CellValue::Int(i + 1), s("x")];
        t.rows.push(row.clone());
        let meta = t.db_meta.as_mut().unwrap();
        meta.row_tags.push(Some(i));
        meta.original.insert(i, row);
    }
    for r in &mut t.rows {
        r[1] = s("edited");
    }
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!(plan.updates.len(), 250);
    let rows: Vec<Vec<CellValue>> = (1..=250)
        .map(|i| {
            vec![
                CellValue::Int(i),
                if i <= 3 {
                    s(["a", "b", "c"][i as usize - 1])
                } else {
                    s("x")
                },
            ]
        })
        .collect();
    let mut c = scripted(DbEngine::Postgres, rows);
    apply(&mut c, &t, &plan).unwrap();
    let keys_per_select: Vec<usize> = c
        .statements
        .iter()
        .filter(|q| q.starts_with("SELECT"))
        .map(|q| q.matches(" OR ").count() + 1)
        .collect();
    assert_eq!(
        keys_per_select,
        vec![100, 100, 50],
        "250 keys in batches of {CONFLICT_READ_BATCH}"
    );
}

/// A row someone already deleted is not deleted again, and its tag comes
/// back so the tab can drop it.
#[test]
fn a_row_already_gone_on_the_server_is_reported_not_deleted() {
    let (t, plan) = edited_plan();
    let mut c = scripted(DbEngine::Postgres, vec![vec![CellValue::Int(2), s("b")]]);
    let report = apply(&mut c, &t, &plan).unwrap();
    assert_eq!((report.deleted, report.updated), (0, 1));
    assert_eq!(report.gone, vec![2]);
    assert!(!c.statements.iter().any(|q| q.starts_with("DELETE")));
}

/// `base_table()` plus a column added in the tab, with id=2's name edited:
/// the added column forces all three rows into the plan.
fn added_column_plan() -> (DataTable, DbWriteBackPlan) {
    let mut t = base_table();
    t.columns.push(col("note", "Utf8"));
    for r in &mut t.rows {
        r.push(s("n"));
    }
    t.rows[1][1] = s("B");
    let plan = build_write_back_plan(&t, &pk()).unwrap();
    assert_eq!((plan.updates.len(), plan.added_columns.len()), (3, 1));
    (t, plan)
}

fn untouched_rows() -> Vec<Vec<CellValue>> {
    vec![
        vec![CellValue::Int(1), s("a")],
        vec![CellValue::Int(2), s("b")],
        vec![CellValue::Int(3), s("c")],
    ]
}

fn theirs_changed_id_2() -> Vec<Vec<CellValue>> {
    let mut rows = untouched_rows();
    rows[1][1] = s("b-theirs");
    rows
}

fn starting_with<'a>(c: &'a ScriptedConnector, word: &str) -> Vec<&'a String> {
    c.statements
        .iter()
        .filter(|q| q.starts_with(word))
        .collect()
}

/// MySQL commits on DDL, so the check also runs before the ALTER: a
/// conflict found there must not have added the column already.
#[test]
fn a_conflict_before_the_alter_adds_no_column() {
    let (t, plan) = added_column_plan();
    let mut c = scripted(DbEngine::MySql, theirs_changed_id_2());
    let err = apply(&mut c, &t, &plan).unwrap_err();
    let conflicts = err.downcast_ref::<WriteConflicts>().expect("typed");
    assert!(!conflicts.columns_added, "{err:#}");
    assert!(starting_with(&c, "ALTER").is_empty(), "{:?}", c.statements);
    assert_eq!(c.statements.last().unwrap(), "ROLLBACK");
}

/// The ALTER committed and released the locks: a change that slipped in
/// then is caught by the second check, after the column was added.
#[test]
fn a_conflict_after_an_auto_committing_alter_is_still_caught() {
    let (t, plan) = added_column_plan();
    let mut c = scripted(DbEngine::MySql, untouched_rows());
    c.later = Some(theirs_changed_id_2());
    let err = apply(&mut c, &t, &plan).unwrap_err();
    let conflicts = err.downcast_ref::<WriteConflicts>().expect("typed");
    assert!(
        conflicts.columns_added,
        "the caller must learn the column exists"
    );
    assert!(format!("{err:#}").contains("only the new columns were added"));
    assert_eq!(starting_with(&c, "ALTER").len(), 1, "the column was added");
    assert!(starting_with(&c, "UPDATE").is_empty(), "{:?}", c.statements);
    assert_eq!(c.statements.last().unwrap(), "ROLLBACK");
}

/// After the ALTER a new transaction starts and the rows are read (and
/// locked) again before any DML; engines with transactional DDL read once.
#[test]
fn an_auto_committing_alter_is_followed_by_a_new_transaction_and_a_second_read() {
    let (t, plan) = added_column_plan();
    let mut my = scripted(DbEngine::MySql, untouched_rows());
    apply(&mut my, &t, &plan).unwrap();
    let kinds: Vec<&str> = my
        .statements
        .iter()
        .map(|q| q.split_whitespace().next().unwrap_or(""))
        .collect();
    assert_eq!(
        kinds,
        [
            "BEGIN", "SELECT", "ALTER", "BEGIN", "SELECT", "UPDATE", "UPDATE", "UPDATE", "COMMIT"
        ]
    );
    let mut pg = scripted(DbEngine::Postgres, untouched_rows());
    pg.later = Some(theirs_changed_id_2());
    apply(&mut pg, &t, &plan).expect("Postgres keeps its locks through the ALTER");
    assert_eq!(starting_with(&pg, "SELECT").len(), 1);
    assert_eq!(starting_with(&pg, "BEGIN").len(), 1);
    // Exasol has no transaction to keep: it reads again after the ALTER too.
    let mut exa = scripted(DbEngine::Exasol, untouched_rows());
    exa.later = Some(theirs_changed_id_2());
    let err = apply(&mut exa, &t, &plan).unwrap_err();
    assert!(
        err.downcast_ref::<WriteConflicts>()
            .is_some_and(|c| c.columns_added)
    );
    assert_eq!(starting_with(&exa, "SELECT").len(), 2);
}

#[test]
fn an_int_and_a_float_are_the_same_cell_only_when_exactly_equal() {
    let big = 1i64 << 53;
    assert!(same_cell(&CellValue::Int(5), &CellValue::Float(5.0)));
    assert!(same_cell(&CellValue::Float(-0.0), &CellValue::Int(0)));
    assert!(same_cell(
        &CellValue::Int(big),
        &CellValue::Float(big as f64)
    ));
    assert!(!same_cell(&CellValue::Int(5), &CellValue::Float(5.5)));
    assert!(
        !same_cell(&CellValue::Int(big + 1), &CellValue::Float(big as f64)),
        "2^53 + 1 rounds to 2^53 as f64; they are different numbers"
    );
    assert!(!same_cell(
        &CellValue::Int(i64::MAX),
        &CellValue::Float(i64::MAX as f64)
    ));
    assert!(!same_cell(&CellValue::Int(0), &CellValue::Float(f64::NAN)));
    assert!(!same_cell(&CellValue::Int(5), &s("5")));
}

/// Oracle types an unconstrained NUMBER per result: the load read
/// `Float(5.0)`, a re-read of whole numbers reads `Int(5)`. Same row, same key.
#[test]
fn an_oracle_number_read_as_float_then_int_is_untouched() {
    assert_eq!(
        key_text(&[CellValue::Float(5.0)]),
        key_text(&[CellValue::Int(5)])
    );
    assert_ne!(
        key_text(&[CellValue::Float(5.5)]),
        key_text(&[CellValue::Int(5)])
    );
    let cols = names(&["id", "n"]);
    let base = vec![CellValue::Float(1.0), CellValue::Float(5.0)];
    let theirs = vec![CellValue::Int(1), CellValue::Int(5)];
    let mine = vec![CellValue::Float(1.0), CellValue::Float(6.0)];
    let merged = merge_row(&cols, &cols, &base, &theirs, &mine).unwrap();
    assert_eq!(merged, mine);
    assert!(same_row(&theirs, &base), "not reported as merged");
}

#[test]
fn the_re_read_addresses_composite_and_quoted_keys() {
    let sql = reread_sql(
        DbEngine::Postgres,
        "public",
        "t",
        &names(&["a", "b", "v"]),
        &names(&["a", "b"]),
        &[
            vec![CellValue::Int(1), s("x")],
            vec![CellValue::Int(2), s("o'brien")],
        ],
    );
    assert_eq!(
        sql,
        "SELECT \"a\", \"b\", \"v\" FROM \"public\".\"t\" \
         WHERE (\"a\" = 1 AND \"b\" = 'x') OR (\"a\" = 2 AND \"b\" = 'o''brien') FOR UPDATE"
    );
    let ms = reread_sql(
        DbEngine::Mssql,
        "dbo",
        "t",
        &names(&["a", "b"]),
        &names(&["b"]),
        &[vec![s("it's")]],
    );
    assert_eq!(
        ms,
        "SELECT [a], [b] FROM [dbo].[t] WITH (UPDLOCK, ROWLOCK) WHERE ([b] = 'it''s')"
    );
}

#[test]
fn a_report_shows_their_cells_and_drops_rows_gone_from_the_server() {
    let mut t = base_table();
    // A column the save does not write (a hash the database computes)
    // keeps its value: the merged row is matched by name.
    t.columns.push(col("h", "Utf8"));
    for r in &mut t.rows {
        r.push(s("hash"));
    }
    let report = DbWriteBackReport {
        deleted: 0,
        updated: 1,
        inserted: 0,
        added_columns: 0,
        merged: vec![(1, vec![CellValue::Int(2), s("b-theirs")])],
        gone: vec![2],
    };
    let undo = t.undo_stack.len();
    report.apply_to(&mut t, &names(&["id", "name"]));
    assert_eq!(
        t.rows,
        vec![
            vec![CellValue::Int(1), s("a"), s("hash")],
            vec![CellValue::Int(2), s("b-theirs"), s("hash")],
        ]
    );
    assert_eq!(t.db_meta.as_ref().unwrap().row_tags, vec![Some(0), Some(1)]);
    assert_eq!(
        t.undo_stack.len(),
        undo,
        "the server dropped the row, the user has nothing to undo"
    );
}

#[test]
fn adopting_added_columns_keeps_the_resolutions_working() {
    // The save added column x; the conflict came from the check after the
    // ALTER, so x is on the server already (columns_added).
    let mut t = base_table();
    t.columns.push(col("x", "Utf8"));
    for r in &mut t.rows {
        r.push(CellValue::Null);
    }
    t.set(1, 1, s("B"));
    t.set(1, 2, s("x1"));
    let mut snap = t.clone();
    snap.apply_edits();
    let plan = build_write_back_plan(&snap, &pk()).unwrap();
    assert_eq!(plan.added_columns.len(), 1);
    let out = merge_plan(
        &plan,
        plan.merge.as_ref().unwrap(),
        &server(&[
            vec![CellValue::Int(1), s("a")],
            vec![CellValue::Int(2), s("b-theirs")],
            vec![CellValue::Int(3), s("c")],
        ]),
    );
    assert_eq!(out.conflicts.len(), 1);

    let mut nothing_added = WriteConflicts {
        conflicts: out.conflicts.clone(),
        columns_added: false,
    };
    nothing_added.adopt_added_columns(&mut t, &names(&["x"]));
    assert_eq!(
        t.db_meta.as_ref().unwrap().original_columns,
        names(&["id", "name"]),
        "without columns_added the baseline is left alone"
    );

    let mut w = WriteConflicts {
        conflicts: out.conflicts,
        columns_added: true,
    };
    w.adopt_added_columns(&mut t, &names(&["x"]));
    let meta = t.db_meta.as_ref().unwrap();
    assert_eq!(meta.original_columns, names(&["id", "name", "x"]));
    assert_eq!(
        meta.original[&0],
        vec![CellValue::Int(1), s("a"), CellValue::Null]
    );

    apply_resolutions(&mut t, &w.conflicts, &[Resolution::KeepMine]);
    let mut snap = t.clone();
    snap.apply_edits();
    let again = build_write_back_plan(&snap, &pk()).unwrap();
    assert!(
        again.added_columns.is_empty(),
        "x is on the server already, so it is not added again"
    );
    assert_eq!(
        again.updates,
        vec![(
            vec![CellValue::Int(2)],
            vec![CellValue::Int(2), s("B"), s("x1")]
        )]
    );
}
