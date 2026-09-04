//! `Cli::detect_action`: turn the parsed flags into exactly one [`Action`],
//! reporting the missing-companion errors the smoke tests pin.
//!
//! Split out of `cli/mod.rs`, which had grown to 2,376 lines holding the whole
//! clap surface, the action enum, the detection pass and the dispatcher in one
//! file. Code moved unchanged.

use super::dispatch::split_cols;
use super::*;

impl Cli {
    /// Resolve the action flag set into a strongly-typed [`Action`].
    /// Returns `None` when none of the action flags were given.
    /// `Err(...)` when an action's required companion is missing (e.g.
    /// `--sql` without `-q`).
    /// Build the writer options from the `--compression` / `--row-group-size`
    /// flags on top of the saved settings, so the command line and the GUI
    /// write the same files. Precedence is flag > `settings.toml` > built-in
    /// default; with no readable config directory (a container, CI) the load
    /// yields the built-in defaults rather than failing.
    fn write_options(&self) -> octa::formats::write_options::WriteOptions {
        let mut opts = octa::ui::settings::AppSettings::load().write_options;
        if let Some(codec) = &self.compression {
            opts.parquet.compression = codec.clone();
        }
        if let Some(n) = self.row_group_size {
            opts.parquet.row_group_size = Some(n);
        }
        opts
    }

    pub fn detect_action(&self) -> Result<Option<Action>, &'static str> {
        if let Some(p) = &self.schema {
            return Ok(Some(Action::Schema(p.clone())));
        }
        if let Some(p) = &self.head {
            return Ok(Some(Action::Head {
                path: p.clone(),
                n: self.lines,
            }));
        }
        if let Some(p) = &self.tail {
            return Ok(Some(Action::Tail {
                path: p.clone(),
                n: self.lines,
            }));
        }
        if let Some(p) = &self.sample {
            return Ok(Some(Action::Sample {
                path: p.clone(),
                n: self.lines,
                seed: self.seed,
            }));
        }
        if !self.convert.is_empty() {
            // Clap's `num_args = 2` enforces the count, but guard
            // defensively for forward-compatibility.
            if self.convert.len() != 2 {
                return Err("--convert needs exactly two paths: --convert IN OUT");
            }
            return Ok(Some(Action::Convert {
                input: self.convert[0].clone(),
                output: self.convert[1].clone(),
                to: self.to.clone(),
                write_options: self.write_options(),
            }));
        }
        if let Some(p) = &self.sql {
            let Some(q) = self.query.clone() else {
                return Err("--sql requires -q / --query \"<sql>\"");
            };
            let extras = parse_named_paths(&self.sql_table, "--sql-table")?;
            let attachments = parse_named_paths(&self.sql_attach, "--sql-attach")?;
            let write_target = match &self.sql_write_to {
                Some(path) => {
                    let table = self
                        .sql_write_table
                        .clone()
                        .ok_or("--sql-write-to requires --sql-write-table TABLE")?;
                    Some(sql::SqlWriteSpec {
                        path: path.clone(),
                        schema: self.sql_write_schema.clone(),
                        table,
                        mode: self.sql_write_mode.to_write_mode(),
                    })
                }
                None => None,
            };
            return Ok(Some(Action::Sql {
                path: p.clone(),
                query: q,
                extras,
                attachments,
                write_target,
                stream: self.stream,
            }));
        }
        if let Some(p) = &self.export_schema {
            return Ok(Some(Action::ExportSchema {
                path: p.clone(),
                target: self.target.to_schema_target(),
            }));
        }
        if !self.compare_schemas.is_empty() {
            if self.compare_schemas.len() != 2 {
                return Err(
                    "--compare-schemas needs exactly two paths: --compare-schemas FILE_A FILE_B",
                );
            }
            return Ok(Some(Action::CompareSchemas {
                path_a: self.compare_schemas[0].clone(),
                path_b: self.compare_schemas[1].clone(),
                table_a: self.table_a.clone(),
                table_b: self.table_b.clone(),
            }));
        }
        if let Some(path) = &self.check_references {
            let (Some(parent_column), Some(child_column)) =
                (self.parent_column.clone(), self.child_column.clone())
            else {
                return Err(
                    "--check-references requires --parent-column COL and --child-column COL",
                );
            };
            return Ok(Some(Action::CheckReferences(Box::new(
                super::referential::Args {
                    parent: path.clone(),
                    parent_column,
                    child: self.child_file.clone(),
                    child_column,
                    table_a: self.table_a.clone().or_else(|| self.table.clone()),
                    table_b: self.table_b.clone(),
                },
            ))));
        }
        if let Some(path) = &self.compare_distributions {
            let Some(column) = self.dist_column.clone() else {
                return Err("--compare-distributions requires --dist-column COL");
            };
            return Ok(Some(Action::CompareDistributions(Box::new(
                super::distribution_compare::Args {
                    path: path.clone(),
                    column,
                    path_b: self.dist_file_b.clone(),
                    column_b: self.dist_column_b.clone(),
                    table: self.table_a.clone().or_else(|| self.table.clone()),
                    table_b: self.table_b.clone(),
                },
            ))));
        }
        if !self.diff.is_empty() {
            let db_b = match (&self.diff_db, &self.diff_db_table) {
                (Some(conn), Some(table)) => Some((conn.clone(), table.clone())),
                (Some(_), None) => {
                    return Err("--diff-db requires --diff-db-table SCHEMA.TABLE");
                }
                _ => None,
            };
            // With a database on the B side one file is the whole input.
            let want = if db_b.is_some() { 1 } else { 2 };
            if self.diff.len() != want {
                return Err(if db_b.is_some() {
                    "--diff with --diff-db takes exactly one file: --diff FILE --diff-db CONN --diff-db-table T"
                } else {
                    "--diff needs exactly two paths: --diff FILE_A FILE_B"
                });
            }
            let mode = octa::data::compare::CompareMode::parse(&self.diff_mode)
                .ok_or("--diff-mode must be one of: set, ordered, join")?;
            if matches!(mode, octa::data::compare::CompareMode::Join) && self.diff_on.is_empty() {
                return Err("--diff-mode join requires --diff-on COL[,COL...]");
            }
            return Ok(Some(Action::Diff {
                path_a: self.diff[0].clone(),
                path_b: self.diff.get(1).cloned(),
                db_b,
                mode,
                on: self.diff_on.clone(),
            }));
        }
        if let Some(p) = &self.validate_schema {
            let Some(schema_file) = self.expect_schema.clone() else {
                return Err("--validate-schema requires --expect-schema SCHEMA_FILE");
            };
            return Ok(Some(Action::ValidateSchema {
                path: p.clone(),
                schema_file,
                table: self.table.clone(),
            }));
        }
        if self.fuzzy_join {
            return Ok(Some(Action::FuzzyJoin(Box::new(
                crate::cli::fuzzy_join::Args {
                    files: self.files.clone(),
                    join_file: self.fuzzy_join_file.clone(),
                    on: self.fuzzy_on.clone(),
                    method: self.fuzzy_method.clone(),
                    threshold: self.fuzzy_threshold,
                    block: self.fuzzy_block.clone(),
                    join_type: self.fuzzy_join_type.clone(),
                    max_rows: self.fuzzy_max_rows,
                },
            ))));
        }
        if let Some(out) = self.report.clone() {
            let Some(path) = self.files.first().cloned() else {
                return Err("--report needs an input FILE");
            };
            return Ok(Some(Action::Report {
                out,
                path,
                table: self.table.clone(),
                sample: self.report_sample,
                sections: self.report_sections.clone(),
            }));
        }
        if let Some(dir) = &self.schema_drift {
            return Ok(Some(Action::SchemaDrift {
                dir: dir.clone(),
                recursive: self.recursive,
                ignore_case: self.ignore_case,
            }));
        }
        if let Some(dir) = &self.relationships {
            return Ok(Some(Action::Relationships {
                dir: dir.clone(),
                recursive: self.recursive,
            }));
        }
        if let Some(path) = &self.check {
            let Some(rules) = self.rules.clone() else {
                return Err("--check requires --rules FILE");
            };
            return Ok(Some(Action::Check {
                path: path.clone(),
                rules,
            }));
        }
        if let Some(paths) = &self.drift_report {
            // clap's `num_args = 2` guarantees the pair, so this is a shape
            // assertion rather than a user-facing error.
            let [a, b] = paths.as_slice() else {
                return Err("--drift-report needs exactly two files");
            };
            return Ok(Some(Action::DriftReport {
                path_a: a.clone(),
                path_b: b.clone(),
                fail_on: self.fail_on.clone(),
            }));
        }
        if let Some(dir) = &self.harmonise_schema {
            // --out-dir is what makes this non-destructive, so it is required
            // rather than defaulted to something clever.
            let Some(out_dir) = self.out_dir.clone() else {
                return Err("--harmonise-schema requires --out-dir DIR");
            };
            return Ok(Some(Action::Harmonise {
                dir: dir.clone(),
                out_dir,
                target_file: self.target_file.clone(),
                recursive: self.recursive,
                ignore_case: self.ignore_case,
                overwrite: self.overwrite,
                write_options: self.write_options(),
            }));
        }
        if let Some(p) = &self.describe {
            return Ok(Some(Action::Describe {
                path: p.clone(),
                table: self.table.clone(),
                sample_rows: self.sample_rows,
                deep: self.deep,
            }));
        }
        if let Some(p) = &self.unique_columns {
            return Ok(Some(Action::UniqueColumns {
                path: p.clone(),
                table: self.table.clone(),
                max_combo: self.max_combo,
            }));
        }
        if !self.anonymize.is_empty() {
            if self.anonymize.len() != 2 {
                return Err("--anonymize needs exactly two paths: --anonymize SPEC FILE");
            }
            return Ok(Some(Action::Anonymize {
                spec: self.anonymize[0].clone(),
                file: self.anonymize[1].clone(),
            }));
        }
        if self.union {
            return Ok(Some(Action::Union {
                files: self.files.clone(),
                union_file: self.union_file.clone(),
                drop: self.union_drop.clone(),
                cast: self.union_cast.clone(),
                ignore_case: self.union_ignore_case,
            }));
        }
        if self.join {
            let join_on = match &self.join_on {
                Some(s) => s
                    .split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>(),
                None => vec![],
            };
            if join_on.is_empty() {
                return Err("--join requires --join-on COL[,COL,...] with at least one key column");
            }
            return Ok(Some(Action::Join {
                files: self.files.clone(),
                join_file: self.join_file.clone(),
                join_on,
                join_type: self.join_type.clone(),
            }));
        }
        if let Some(p) = &self.dedupe {
            return Ok(Some(Action::Dedupe {
                path: p.clone(),
                dedupe_on: self.dedupe_on.clone(),
                dedupe_keep: self.dedupe_keep.clone(),
            }));
        }
        if !self.impute.is_empty() {
            // The positional FILE is the single data source for --impute.
            let path = self
                .files
                .first()
                .cloned()
                .ok_or("--impute requires a positional FILE argument")?;
            return Ok(Some(Action::Impute {
                path,
                specs: self.impute.clone(),
            }));
        }
        if self.outliers {
            let path = self
                .files
                .first()
                .cloned()
                .ok_or("--outliers requires a positional FILE argument")?;
            return Ok(Some(Action::Outliers {
                path,
                method: self.outlier_method.clone(),
                cols: self.outlier_cols.clone(),
                k: self.outlier_k,
            }));
        }
        if let Some(p) = &self.detect_pii {
            return Ok(Some(Action::DetectPii {
                path: p.clone(),
                sample_rows: self.pii_sample,
            }));
        }
        if self.batch_convert {
            if self.files.is_empty() {
                return Err("--batch-convert requires at least one positional FILE");
            }
            let out_dir = self
                .out_dir
                .clone()
                .ok_or("--batch-convert requires --out-dir DIR")?;
            let target_ext = self.to.clone().ok_or("--batch-convert requires --to EXT")?;
            return Ok(Some(Action::BatchConvert {
                inputs: self.files.clone(),
                out_dir,
                target_ext,
                overwrite: self.overwrite,
                write_options: self.write_options(),
            }));
        }
        if let Some(col) = &self.resample {
            let path = self
                .files
                .first()
                .cloned()
                .ok_or("--resample requires a positional FILE argument")?;
            let value_cols = self
                .value_cols
                .as_deref()
                .ok_or("--resample requires --value-cols COLS")?;
            let interval = match &self.interval {
                None => octa::data::timeseries::Interval::Day,
                Some(s) => octa::data::timeseries::Interval::parse(s)
                    .ok_or("--interval must be minute|hour|day|week|month|quarter|year")?,
            };
            let agg = match &self.agg {
                None => octa::data::timeseries::TimeAgg::Sum,
                Some(s) => octa::data::timeseries::TimeAgg::parse(s)
                    .ok_or("--agg must be sum|mean|min|max|count|first|last")?,
            };
            return Ok(Some(Action::Resample {
                path,
                spec: octa::data::timeseries::ResampleSpec {
                    time_col: col.clone(),
                    value_cols: split_cols(value_cols),
                    interval,
                    agg,
                    group_by: self.group_by.as_deref().map(split_cols).unwrap_or_default(),
                },
            }));
        }
        if let Some(col) = &self.rolling {
            let path = self
                .files
                .first()
                .cloned()
                .ok_or("--rolling requires a positional FILE argument")?;
            let order_col = self
                .order_by
                .clone()
                .ok_or("--rolling requires --order-by COL")?;
            let window = self.window.ok_or("--rolling requires --window N")?;
            let agg = match &self.agg {
                None => octa::data::timeseries::TimeAgg::Mean,
                Some(s) => octa::data::timeseries::TimeAgg::parse(s)
                    .ok_or("--agg must be sum|mean|min|max|count|first|last")?,
            };
            return Ok(Some(Action::Rolling {
                path,
                spec: octa::data::timeseries::RollingSpec {
                    order_col,
                    value_col: col.clone(),
                    window,
                    agg,
                    partition_by: self
                        .partition_by_cols
                        .as_deref()
                        .map(split_cols)
                        .unwrap_or_default(),
                },
            }));
        }
        if let Some(col) = &self.partition_by {
            let path = self
                .files
                .first()
                .cloned()
                .ok_or("--partition-by requires a positional FILE argument")?;
            let out_dir = self
                .out_dir
                .clone()
                .ok_or("--partition-by requires --out-dir DIR")?;
            let layout = match &self.partition_layout {
                Some(word) => octa::data::partition::PartitionLayout::parse(word)
                    .ok_or("--partition-layout must be one of: flat, folder, hive, hive-parts")?,
                None => octa::data::partition::PartitionLayout::default(),
            };
            return Ok(Some(Action::Partition {
                path,
                col: col.clone(),
                out_dir,
                format: self.partition_format.clone(),
                layout,
            }));
        }
        if let Some(out) = &self.to_workbook {
            return Ok(Some(Action::ToWorkbook {
                out: out.clone(),
                inputs: self.files.clone(),
            }));
        }
        if let Some(path) = &self.sync_sql {
            let conn = self
                .db
                .clone()
                .ok_or("--sync-sql requires --db CONNECTION")?;
            let table = self
                .sync_table
                .clone()
                .ok_or("--sync-sql requires --sync-table SCHEMA.TABLE")?;
            // Without key columns there is no way to tell an updated row from
            // a deleted-plus-inserted pair, so this is an error, not a default.
            let on = self
                .sync_on
                .clone()
                .ok_or("--sync-sql requires --sync-on COL[,COL...]")?;
            return Ok(Some(Action::SyncSql {
                path: path.clone(),
                conn,
                table,
                on,
            }));
        }
        if let Some(sql) = &self.db_query {
            let conn = self
                .db
                .clone()
                .ok_or("--db-query requires --db CONNECTION")?;
            return Ok(Some(Action::DbQuery {
                conn,
                sql: sql.clone(),
            }));
        }
        if self.db_tables {
            let conn = self
                .db
                .clone()
                .ok_or("--db-tables requires --db CONNECTION")?;
            return Ok(Some(Action::DbTables {
                conn,
                catalog: self.db_catalog.clone(),
            }));
        }
        if let Some(target) = &self.db_write_table {
            let conn = self
                .db
                .clone()
                .ok_or("--db-write-table requires --db CONNECTION")?;
            let file = self
                .files
                .first()
                .cloned()
                .ok_or("--db-write-table requires a positional FILE argument")?;
            return Ok(Some(Action::DbWrite {
                conn,
                catalog: self.db_catalog.clone(),
                target: target.clone(),
                mode: self.db_write_mode.to_db_write_mode(),
                file,
            }));
        }
        if let Some(source) = &self.db_copy {
            let conn = self
                .db
                .clone()
                .ok_or("--db-copy requires --db CONNECTION")?;
            let target_conn = self
                .db_copy_to
                .clone()
                .ok_or("--db-copy requires --db-copy-to CONNECTION")?;
            return Ok(Some(Action::DbCopy {
                conn,
                catalog: self.db_catalog.clone(),
                source: source.clone(),
                target_conn,
                target: self.db_copy_target.clone(),
                target_catalog: self.db_copy_target_catalog.clone(),
                mode: self.db_write_mode.to_db_write_mode(),
            }));
        }
        if let Some(url) = &self.cloud_ls {
            return Ok(Some(Action::CloudLs {
                url: url.clone(),
                recursive: self.recursive,
            }));
        }
        if let Some(url) = &self.cloud_get {
            let out = self.out.clone().ok_or("--cloud-get needs --out FILE")?;
            return Ok(Some(Action::CloudGet {
                url: url.clone(),
                out,
            }));
        }
        if let Some(file) = &self.cloud_put {
            let url = self.to.clone().ok_or("--cloud-put needs --to URL")?;
            return Ok(Some(Action::CloudPut {
                file: file.clone(),
                url,
            }));
        }
        if let Some(from) = &self.cloud_copy {
            let to = self.to.clone().ok_or("--cloud-copy needs --to URL")?;
            return Ok(Some(Action::CloudTransfer {
                from: from.clone(),
                to,
                move_it: false,
            }));
        }
        if let Some(from) = &self.cloud_move {
            let to = self.to.clone().ok_or("--cloud-move needs --to URL")?;
            return Ok(Some(Action::CloudTransfer {
                from: from.clone(),
                to,
                move_it: true,
            }));
        }
        if let Some(url) = &self.cloud_delete {
            return Ok(Some(Action::CloudDelete {
                url: url.clone(),
                recursive: self.recursive,
            }));
        }
        if self.list_connections {
            return Ok(Some(Action::ListConnections));
        }
        if let Some(spec) = &self.add_connection {
            return Ok(Some(Action::AddConnection {
                spec: spec.clone(),
                secret_env: self.secret_env.clone(),
            }));
        }
        if let Some(name) = &self.remove_connection {
            return Ok(Some(Action::RemoveConnection(name.clone())));
        }
        if let Some(shell) = self.completions {
            return Ok(Some(Action::Completions(shell)));
        }
        if self.mcp {
            return Ok(Some(Action::Mcp));
        }
        Ok(None)
    }
}
