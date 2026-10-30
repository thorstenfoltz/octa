//! Live-DB write-back: diff a loaded table against its `DbRowMeta` baseline
//! into a plan (deletes / updates / inserts / added columns), then apply the
//! plan in one transaction. Same diff rules as the SQLite file writer
//! (`src/formats/sqlite_reader.rs`), but addressed by primary-key values
//! instead of rowids, since a server table has no client-visible rowid.
//!
//! Accepted ceilings (by construction, documented in the manual):
//! - Rows beyond the `initial_load_rows` window were never loaded, so they
//!   are never touched: deletes/updates only address loaded tags, inserts
//!   append.
//! - Keyed saves re-read the touched rows inside the transaction (locked on
//!   Postgres, MySQL, Oracle, SQL Server) and merge cell by cell; colliding
//!   changes abort with [`WriteConflicts`]. Redshift and Exasol read without a
//!   lock, so a gap of milliseconds remains between the check and the write
//!   (and Exasol's writes are not atomic: it has no `BEGIN`).
//! - MySQL/MariaDB and Oracle commit on DDL, and Exasol has no transaction. A
//!   save that adds a column checks before the `ALTER TABLE ADD` (a conflict
//!   there writes nothing), then starts a new transaction and checks again
//!   (under fresh locks on MySQL and Oracle); a conflict found only in that
//!   second check leaves the added column in place and says so in
//!   [`WriteConflicts::columns_added`].

use anyhow::{Context, bail};

use crate::data::{CellValue, ColumnInfo, DataTable};

use std::collections::HashMap;

use super::{DbConnector, DbEngine, sql_literal};

/// How a changed row is addressed on the server.
///
/// A table with a server-enforced unique key uses it. A table with none can
/// still be edited by matching **every original column value**, which is only
/// acceptable because each such statement's affected-row count is checked to
/// be exactly 1: if the values match two rows, or none, the whole transaction
/// rolls back rather than guessing. That check is the entire safety argument
/// for [`RowIdentity::FullRow`], so it is not optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowIdentity {
    /// A primary key, or a UNIQUE constraint over NOT NULL columns. Addresses
    /// exactly one row by the server's own guarantee, so no count check is
    /// needed - and none is made, because MySQL reports *changed* rather than
    /// *matched* rows and would make a correct update look wrong.
    Key(Vec<String>),
    /// Every column of the baseline row. No server guarantee, so every
    /// statement built from it is verified to have touched exactly one row.
    FullRow(Vec<String>),
}

impl RowIdentity {
    /// The columns forming the WHERE clause.
    pub fn columns(&self) -> &[String] {
        match self {
            Self::Key(c) | Self::FullRow(c) => c,
        }
    }

    /// Whether each statement must be verified to have touched exactly one row.
    pub fn needs_affected_row_check(&self) -> bool {
        matches!(self, Self::FullRow(_))
    }

    /// Whether a NULL among the addressing values is acceptable. A key column
    /// is NOT NULL by construction, so a NULL there means the baseline is
    /// wrong; a full row may legitimately hold NULLs, matched with `IS NULL`.
    fn allows_null(&self) -> bool {
        matches!(self, Self::FullRow(_))
    }
}

/// What a save would do to the server table, computed from the table's rows
/// vs its `DbRowMeta` baseline.
#[derive(Debug, Default, PartialEq)]
pub struct DbWriteBackPlan {
    /// PK values per row to delete (from `original`, so an edited PK cell
    /// still locates its row).
    pub deletes: Vec<Vec<CellValue>>,
    /// (original PK values, full current row) per changed row.
    pub updates: Vec<(Vec<CellValue>, Vec<CellValue>)>,
    /// Full rows for `None`-tagged (user-inserted) rows.
    pub inserts: Vec<Vec<CellValue>>,
    /// Columns present now but not in `original_columns` (`ALTER TABLE ADD`).
    pub added_columns: Vec<ColumnInfo>,
    /// Baselines for the conflict check; see [`MergeInfo`].
    pub merge: Option<MergeInfo>,
}

impl DbWriteBackPlan {
    pub fn is_empty(&self) -> bool {
        self.deletes.is_empty()
            && self.updates.is_empty()
            && self.inserts.is_empty()
            && self.added_columns.is_empty()
    }
}

/// What a conflict check needs about each planned row, by index parallel to
/// [`DbWriteBackPlan::updates`] / [`DbWriteBackPlan::deletes`]. Built only by
/// [`build_write_back_plan`], i.e. for edits made in a tab. A sync plan
/// already compares against the server's current state and carries none.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct MergeInfo {
    /// Baseline column names, the layout of `base` and server rows.
    pub base_columns: Vec<String>,
    /// The tab's column names now, the layout of `mine` and written rows.
    pub columns: Vec<String>,
    /// `(tag, baseline row)` per entry of `updates`.
    pub update_bases: Vec<(i64, Vec<CellValue>)>,
    /// `(tag, baseline row)` per entry of `deletes`.
    pub delete_bases: Vec<(i64, Vec<CellValue>)>,
}

/// One cell both sides changed, to different values.
#[derive(Debug, Clone, PartialEq)]
pub struct CellConflict {
    pub column: String,
    pub base: CellValue,
    pub theirs: CellValue,
    pub mine: CellValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConflictKind {
    /// Same cells changed on both sides. `theirs` is the whole server row
    /// (baseline order), which a resolution rebases onto.
    Cells {
        cells: Vec<CellConflict>,
        theirs: Vec<CellValue>,
    },
    /// I edited the row; someone deleted it on the server.
    DeletedOnServer,
    /// I deleted the row; someone changed it on the server.
    ChangedOnServer { theirs: Vec<CellValue> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowConflict {
    pub tag: i64,
    /// The baseline row (baseline column order) the conflict was found
    /// against; `apply_resolutions` skips a conflict whose tag no longer has
    /// this baseline.
    pub base: Vec<CellValue>,
    /// Key values, to name the row in the dialog.
    pub key: Vec<CellValue>,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Cells: my value. Deleted on server: add it back. Changed on server: delete anyway.
    KeepMine,
    /// Cells: their value. Deleted on server: let it go. Changed on server: keep theirs.
    TakeTheirs,
}

/// What the server check turned a plan into.
#[derive(Debug, Default, PartialEq)]
pub struct MergeOutcome {
    /// (key, row to write) per update that merged.
    pub updates: Vec<(Vec<CellValue>, Vec<CellValue>)>,
    pub deletes: Vec<Vec<CellValue>>,
    /// (tag, written row) for every update that took cells from the server,
    /// so the tab can show them after the save.
    pub merged: Vec<(i64, Vec<CellValue>)>,
    pub conflicts: Vec<RowConflict>,
    /// Tags of rows dropped because the server row is gone (a delete of a
    /// vanished row, an unedited row forced into the plan): the tab can
    /// remove them.
    pub gone: Vec<i64>,
}

/// Outcome counts of an applied write-back.
#[derive(Debug, Clone, PartialEq)]
pub struct DbWriteBackReport {
    pub deleted: usize,
    pub updated: usize,
    pub inserted: usize,
    pub added_columns: usize,
    /// (tag, written row in the tab's column order) for rows that also took
    /// someone else's changes; the tab shows them after the save.
    pub merged: Vec<(i64, Vec<CellValue>)>,
    /// Tags of rows left out because the server row is already gone; the tab
    /// can remove them (see [`MergeOutcome::gone`]).
    pub gone: Vec<i64>,
}

/// Keys per conflict re-read. 100 because the Oracle driver stops every
/// result at 100 rows (`.claude/rules/database.md`, oracle-rs defects).
pub const CONFLICT_READ_BATCH: usize = 100;

/// Rows that changed on the server since the tab loaded, in ways that
/// collide with the save. The transaction was rolled back: no row was written.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteConflicts {
    pub conflicts: Vec<RowConflict>,
    /// The conflict was found in the check after the `ALTER TABLE ADD`s on an
    /// engine whose DDL commits (see [`rechecks_after_alter`]): the plan's
    /// added columns are already on the server, so the tab's baseline must
    /// take them in (NULL in every row) before the next save.
    pub columns_added: bool,
}

impl std::fmt::Display for WriteConflicts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let saved = if self.columns_added {
            "only the new columns were added"
        } else {
            "nothing was saved"
        };
        write!(
            f,
            "{} row(s) were changed by someone else since the table was loaded; {saved}",
            self.conflicts.len()
        )
    }
}

impl std::error::Error for WriteConflicts {}

impl WriteConflicts {
    /// After a `columns_added` conflict the plan's `added` columns are on the
    /// server already, NULL in every row: take them into `table`'s baseline
    /// and into each conflict's baseline and server row. The next save then
    /// does not add them a second time, and [`apply_resolutions`] still
    /// recognises every baseline. A no-op unless `columns_added`.
    pub fn adopt_added_columns(&mut self, table: &mut DataTable, added: &[String]) {
        let Some(meta) = table.db_meta.as_mut().filter(|_| self.columns_added) else {
            return;
        };
        let new: Vec<String> = added
            .iter()
            .filter(|c| !meta.original_columns.contains(c))
            .cloned()
            .collect();
        let pad = |row: &mut Vec<CellValue>| row.resize(row.len() + new.len(), CellValue::Null);
        for row in meta.original.values_mut() {
            pad(row);
        }
        for c in &mut self.conflicts {
            pad(&mut c.base);
            if let ConflictKind::Cells { theirs, .. } | ConflictKind::ChangedOnServer { theirs } =
                &mut c.kind
            {
                pad(theirs);
            }
        }
        meta.original_columns.extend(new);
    }
}

impl DbWriteBackReport {
    /// Show the tab what the server holds now: each merged row's cells go
    /// into the tab's row with that tag, matched by name (`columns` is the
    /// layout the save wrote; the tab may hold more, such as hash columns
    /// the database computes), and rows whose server row is gone leave the
    /// tab. Call it with the edits applied, before re-baselining.
    pub fn apply_to(&self, table: &mut DataTable, columns: &[String]) {
        let row_of = |t: &DataTable, tag: i64| {
            t.db_meta
                .as_ref()?
                .row_tags
                .iter()
                .position(|x| *x == Some(tag))
        };
        for (tag, row) in &self.merged {
            let Some(r) = row_of(table, *tag) else {
                continue;
            };
            for (name, v) in columns.iter().zip(row) {
                if let Some(j) = table.columns.iter().position(|c| &c.name == name) {
                    table.rows[r][j] = v.clone();
                }
            }
        }
        for &tag in &self.gone {
            if let Some(r) = row_of(table, tag) {
                table.delete_row(r);
                // The server dropped it, not the user: nothing to undo.
                table.undo_stack.pop();
            }
        }
    }
}

/// Take `names` out of `table` (edits applied) and out of its baseline, so a
/// plan built afterwards never sees them: the hash columns a database
/// computes for a tab, also one renamed or deleted since (gone from the
/// table, still in the baseline).
pub fn drop_columns(table: &mut DataTable, names: &[String]) {
    let gone = |n: &String| names.contains(n);
    let keep: Vec<bool> = table.columns.iter().map(|c| !gone(&c.name)).collect();
    if keep.contains(&false) {
        table.columns.retain(|c| !gone(&c.name));
        for row in &mut table.rows {
            let mut k = keep.iter();
            row.retain(|_| *k.next().unwrap_or(&true));
        }
    }
    if let Some(meta) = table.db_meta.as_mut() {
        let keep: Vec<bool> = meta.original_columns.iter().map(|c| !gone(c)).collect();
        if keep.contains(&false) {
            meta.original_columns.retain(|c| !gone(c));
            for row in meta.original.values_mut() {
                let mut k = keep.iter();
                row.retain(|_| *k.next().unwrap_or(&true));
            }
        }
    }
}

/// Build the plan from a table whose edits are applied (`apply_edits()` done
/// by the caller). Errors when `db_meta` is missing (row identity lost), a
/// baseline column was removed or renamed, a PK column is not among the
/// current columns, or a PK value is NULL (the row cannot be addressed).
pub fn build_write_back_plan(
    table: &DataTable,
    identity: &RowIdentity,
) -> anyhow::Result<DbWriteBackPlan> {
    let pk_cols = identity.columns();
    let Some(meta) = table.db_meta.as_ref() else {
        bail!(
            "row identity lost: the table was rewritten locally (e.g. by a SQL mutation); \
             reload the table or use Run on server"
        );
    };
    if pk_cols.is_empty() {
        bail!("the table has no key and no baseline columns, so edits cannot be addressed");
    }

    // Baseline columns must all still exist under their original names, in
    // any position; column drops/renames are rejected for now.
    let current_names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    for orig in &meta.original_columns {
        if !current_names.contains(&orig.as_str()) {
            bail!("column '{orig}' was removed or renamed; write-back supports only added columns");
        }
    }
    let added_columns: Vec<ColumnInfo> = table
        .columns
        .iter()
        .filter(|c| !meta.original_columns.contains(&c.name))
        .cloned()
        .collect();

    // PK column indices in the ORIGINAL column order (the baseline snapshot
    // rows are in load order, which is the original order).
    let pk_orig_idx: Vec<usize> = pk_cols
        .iter()
        .map(|pk| {
            meta.original_columns
                .iter()
                .position(|c| c == pk)
                .with_context(|| format!("primary-key column '{pk}' not found in the table"))
        })
        .collect::<anyhow::Result<_>>()?;

    let pk_values = |original_row: &[CellValue]| -> anyhow::Result<Vec<CellValue>> {
        pk_orig_idx
            .iter()
            .map(|&i| {
                let v = original_row.get(i).cloned().unwrap_or(CellValue::Null);
                // A key column is NOT NULL by construction, so a NULL here
                // means the baseline disagrees with the schema. A full-row
                // match may hold NULLs; `pk_where_sql` turns them into
                // `IS NULL`.
                if matches!(v, CellValue::Null) && !identity.allows_null() {
                    bail!("a key value is NULL; the row cannot be addressed");
                }
                Ok(v)
            })
            .collect()
    };

    let mut update_bases = Vec::new();
    let mut delete_bases = Vec::new();
    let mut plan = DbWriteBackPlan {
        added_columns,
        ..Default::default()
    };

    // DELETE rows whose tag is no longer present.
    let live_tags: std::collections::HashSet<i64> =
        meta.row_tags.iter().filter_map(|t| *t).collect();
    let mut delete_tags: Vec<&i64> = meta
        .original
        .keys()
        .filter(|t| !live_tags.contains(t))
        .collect();
    delete_tags.sort();
    for tag in delete_tags {
        plan.deletes.push(pk_values(&meta.original[tag])?);
        delete_bases.push((*tag, meta.original[tag].clone()));
    }

    // INSERT / UPDATE per current row. Added columns force full-row updates
    // even for rows whose baseline cells match (same gate as the SQLite
    // writer), so the new columns get their values.
    let force_update = !plan.added_columns.is_empty();
    let current_names_owned: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    for (row_idx, tag) in meta.row_tags.iter().enumerate() {
        let row_vals: Vec<CellValue> = (0..table.columns.len())
            .map(|c| table.get(row_idx, c).cloned().unwrap_or(CellValue::Null))
            .collect();
        match tag {
            None => plan.inserts.push(row_vals),
            Some(tag) => {
                let Some(original) = meta.original.get(tag) else {
                    continue;
                };
                if !force_update
                    && same_row(
                        &to_current_order(&meta.original_columns, &current_names_owned, original),
                        &row_vals,
                    )
                {
                    continue;
                }
                plan.updates.push((pk_values(original)?, row_vals));
                update_bases.push((*tag, original.clone()));
            }
        }
    }
    plan.merge = Some(MergeInfo {
        base_columns: meta.original_columns.clone(),
        columns: table.columns.iter().map(|c| c.name.clone()).collect(),
        update_bases,
        delete_bases,
    });
    Ok(plan)
}

/// Cell equality as write-back means it: `==`, except NaN equals NaN (a NaN
/// decoded from the server would otherwise differ from itself and conflict
/// forever), and an `Int` equals a `Float` holding exactly that whole number
/// (Oracle types an unconstrained NUMBER per result, so the load can read
/// `Float(5.0)` where a re-read of a few rows reads `Int(5)`). NULL stays
/// distinct from an empty string.
fn same_cell(a: &CellValue, b: &CellValue) -> bool {
    a == b
        || match (a, b) {
            (CellValue::Float(x), CellValue::Float(y)) => x.is_nan() && y.is_nan(),
            (CellValue::Int(i), CellValue::Float(f)) | (CellValue::Float(f), CellValue::Int(i)) => {
                whole_i64(*f) == Some(*i)
            }
            _ => false,
        }
}

/// `f` as an `i64` when it is exactly a whole number in range. Never
/// `i as f64 == f`: that rounds above 2^53 and calls neighbours equal.
fn whole_i64(f: f64) -> Option<i64> {
    (f.fract() == 0.0 && (i64::MIN as f64..i64::MAX as f64).contains(&f)).then_some(f as i64)
}

fn same_row(a: &[CellValue], b: &[CellValue]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_cell(x, y))
}

/// Three-way merge of one row, cell by cell, matched by column name.
/// `base` / `theirs` are in `base_columns` order, `mine` in `columns` order;
/// the merged row comes back in `columns` order. A column added in the tab
/// has no server value and keeps mine.
pub fn merge_row(
    base_columns: &[String],
    columns: &[String],
    base: &[CellValue],
    theirs: &[CellValue],
    mine: &[CellValue],
) -> Result<Vec<CellValue>, Vec<CellConflict>> {
    let mut out = Vec::with_capacity(columns.len());
    let mut conflicts = Vec::new();
    for (j, name) in columns.iter().enumerate() {
        let m = mine.get(j).cloned().unwrap_or(CellValue::Null);
        let Some(i) = base_columns.iter().position(|c| c == name) else {
            out.push(m);
            continue;
        };
        let b = base.get(i).cloned().unwrap_or(CellValue::Null);
        let t = theirs.get(i).cloned().unwrap_or(CellValue::Null);
        if same_cell(&t, &b) || same_cell(&m, &t) {
            out.push(m);
        } else if same_cell(&m, &b) {
            out.push(t);
        } else {
            conflicts.push(CellConflict {
                column: name.clone(),
                base: b,
                theirs: t,
                mine: m.clone(),
            });
            out.push(m);
        }
    }
    if conflicts.is_empty() {
        Ok(out)
    } else {
        Err(conflicts)
    }
}

/// A hashable spelling of key values. Both sides come through the same
/// connector decoding; a whole `Float` prints as its `Int` so a key Oracle
/// typed differently per result (see [`same_cell`]) still matches.
pub fn key_text(key: &[CellValue]) -> String {
    key.iter()
        .map(|v| match v {
            CellValue::Float(f) => whole_i64(*f).map_or_else(|| v.to_string(), |i| i.to_string()),
            _ => v.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

/// Check every planned update and delete against `server` (key text ->
/// server row in baseline order; a missing key means the row is gone).
pub fn merge_plan(
    plan: &DbWriteBackPlan,
    info: &MergeInfo,
    server: &HashMap<String, Vec<CellValue>>,
) -> MergeOutcome {
    debug_assert_eq!(plan.updates.len(), info.update_bases.len());
    debug_assert_eq!(plan.deletes.len(), info.delete_bases.len());
    let mut out = MergeOutcome::default();
    for ((key, mine), (tag, base)) in plan.updates.iter().zip(&info.update_bases) {
        let Some(theirs) = server.get(&key_text(key)) else {
            // A row I never edited (only forced into the plan by added
            // columns) that someone deleted: nothing of mine is lost.
            let untouched = info.columns.iter().enumerate().all(|(j, name)| {
                let m = mine.get(j);
                match info.base_columns.iter().position(|c| c == name) {
                    Some(i) => matches!((m, base.get(i)), (Some(m), Some(b)) if same_cell(m, b)),
                    None => matches!(m, None | Some(CellValue::Null)),
                }
            });
            if untouched {
                out.gone.push(*tag);
                continue;
            }
            out.conflicts.push(RowConflict {
                tag: *tag,
                base: base.clone(),
                key: key.clone(),
                kind: ConflictKind::DeletedOnServer,
            });
            continue;
        };
        match merge_row(&info.base_columns, &info.columns, base, theirs, mine) {
            Ok(row) => {
                if !same_row(theirs, base) {
                    out.merged.push((*tag, row.clone()));
                }
                out.updates.push((key.clone(), row));
            }
            Err(cells) => out.conflicts.push(RowConflict {
                tag: *tag,
                base: base.clone(),
                key: key.clone(),
                kind: ConflictKind::Cells {
                    cells,
                    theirs: theirs.clone(),
                },
            }),
        }
    }
    for (key, (tag, base)) in plan.deletes.iter().zip(&info.delete_bases) {
        match server.get(&key_text(key)) {
            None => out.gone.push(*tag), // already gone: nothing to delete
            Some(theirs) if same_row(theirs, base) => out.deletes.push(key.clone()),
            Some(theirs) => out.conflicts.push(RowConflict {
                tag: *tag,
                base: base.clone(),
                key: key.clone(),
                kind: ConflictKind::ChangedOnServer {
                    theirs: theirs.clone(),
                },
            }),
        }
    }
    out
}

/// A baseline-order row laid out in the tab's current column order (NULL for
/// columns the tab added).
fn to_current_order(
    base_columns: &[String],
    columns: &[String],
    row: &[CellValue],
) -> Vec<CellValue> {
    columns
        .iter()
        .map(|name| {
            base_columns
                .iter()
                .position(|c| c == name)
                .and_then(|i| row.get(i).cloned())
                .unwrap_or(CellValue::Null)
        })
        .collect()
}

/// Apply the user's choices to the tab so the next save does what they
/// picked: each row is **rebased** onto the server's values (its baseline
/// becomes theirs), their non-conflicting changes are brought into the tab,
/// and each disputed cell gets mine or theirs. The next save checks the
/// server again.
///
/// The baselines and row tags are rewritten outside the undo log, so an older
/// undo entry would restore cells against a baseline that no longer exists and
/// the next save could overwrite the colleague's change. Both the undo and the
/// redo history of `table` are therefore cleared (user decision). The stacks
/// live on `DataTable`, so this function does it itself.
pub fn apply_resolutions(table: &mut DataTable, conflicts: &[RowConflict], choices: &[Resolution]) {
    let columns: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    for (conflict, choice) in conflicts.iter().zip(choices) {
        let Some(meta) = table.db_meta.as_ref() else {
            return;
        };
        let base_columns = meta.original_columns.clone();
        let row_idx = meta.row_tags.iter().position(|t| *t == Some(conflict.tag));
        // A stale or mis-routed conflict must not rebase some other row.
        match meta.original.get(&conflict.tag) {
            Some(cur) if same_row(cur, &conflict.base) => {}
            _ => continue,
        }
        let base = &conflict.base;
        match (&conflict.kind, choice) {
            (ConflictKind::Cells { theirs, .. }, _) => {
                let Some(r) = row_idx else { continue };
                for (j, name) in columns.iter().enumerate() {
                    let Some(i) = base_columns.iter().position(|c| c == name) else {
                        continue;
                    };
                    let m = table.get(r, j).cloned().unwrap_or(CellValue::Null);
                    let (Some(b), Some(t)) = (base.get(i), theirs.get(i)) else {
                        continue;
                    };
                    let v = if same_cell(t, b) || same_cell(&m, t) {
                        m.clone()
                    } else if same_cell(&m, b) {
                        t.clone()
                    } else if *choice == Resolution::KeepMine {
                        m.clone()
                    } else {
                        t.clone()
                    };
                    if !same_cell(&v, &m) {
                        table.set(r, j, v);
                    }
                }
                if let Some(meta) = table.db_meta.as_mut() {
                    meta.original.insert(conflict.tag, theirs.clone());
                }
            }
            (ConflictKind::DeletedOnServer, Resolution::KeepMine) => {
                // Add it back: the row becomes an insert.
                if let (Some(r), Some(meta)) = (row_idx, table.db_meta.as_mut()) {
                    meta.row_tags[r] = None;
                    meta.original.remove(&conflict.tag);
                }
            }
            (ConflictKind::DeletedOnServer, Resolution::TakeTheirs) => {
                // Let it go: gone from the tab too, with no DELETE planned.
                if let Some(r) = row_idx {
                    table.delete_row(r);
                }
                if let Some(meta) = table.db_meta.as_mut() {
                    meta.original.remove(&conflict.tag);
                }
            }
            (ConflictKind::ChangedOnServer { theirs }, Resolution::KeepMine) => {
                // Delete anyway: the delete goes ahead against their version.
                if let Some(meta) = table.db_meta.as_mut() {
                    meta.original.insert(conflict.tag, theirs.clone());
                }
            }
            (ConflictKind::ChangedOnServer { theirs }, Resolution::TakeTheirs) => {
                // Keep theirs: the row comes back into the tab as the server has it.
                let end = table.row_count();
                table.insert_row(end);
                let row = to_current_order(&base_columns, &columns, theirs);
                table.rows[end] = row;
                if let Some(meta) = table.db_meta.as_mut() {
                    meta.row_tags[end] = Some(conflict.tag);
                    meta.original.insert(conflict.tag, theirs.clone());
                }
            }
        }
    }
    table.undo_stack.clear();
    table.redo_stack.clear();
}

/// The fully-quoted `schema.table` target.
fn target_sql(engine: DbEngine, schema: &str, table: &str) -> String {
    if schema.is_empty() {
        engine.quote_ident(table)
    } else {
        format!(
            "{}.{}",
            engine.quote_ident(schema),
            engine.quote_ident(table)
        )
    }
}

/// `col = literal AND col2 IS NULL` for a WHERE clause.
///
/// A NULL is matched with `IS NULL`, never `= NULL`, which is never true in
/// SQL. Key columns cannot be NULL, so this only ever fires for a full-row
/// match - where getting it wrong would silently address no rows at all.
fn pk_where_sql(engine: DbEngine, pk_cols: &[String], pk_vals: &[CellValue]) -> String {
    pk_cols
        .iter()
        .zip(pk_vals)
        .map(|(col, val)| {
            let ident = engine.quote_ident(col);
            match val {
                CellValue::Null => format!("{ident} IS NULL"),
                _ => format!("{ident} = {}", sql_literal(engine, val)),
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// `SELECT <baseline columns> FROM t WHERE (key) OR (key) ...`, locked until
/// the transaction ends where the engine can: `FOR UPDATE` on Postgres,
/// MySQL and Oracle, an update-lock table hint on SQL Server. Other engines
/// rely on their transaction isolation (documented ceiling).
fn reread_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    base_columns: &[String],
    key_cols: &[String],
    keys: &[Vec<CellValue>],
) -> String {
    let cols = base_columns
        .iter()
        .map(|c| engine.quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let filter = keys
        .iter()
        .map(|k| format!("({})", pk_where_sql(engine, key_cols, k)))
        .collect::<Vec<_>>()
        .join(" OR ");
    let target = target_sql(engine, schema, table);
    match engine {
        DbEngine::Mssql => {
            format!("SELECT {cols} FROM {target} WITH (UPDLOCK, ROWLOCK) WHERE {filter}")
        }
        DbEngine::Postgres | DbEngine::MySql | DbEngine::Oracle => {
            format!("SELECT {cols} FROM {target} WHERE {filter} FOR UPDATE")
        }
        _ => format!("SELECT {cols} FROM {target} WHERE {filter}"),
    }
}

/// Read the server's current rows for every key the plan touches, keyed by
/// [`key_text`], in baseline column order. Decoded by `connector.query`, the
/// same path that loaded the tab; the one known difference (Oracle typing an
/// unconstrained NUMBER per result) is absorbed by [`same_cell`] and
/// [`key_text`].
fn read_server_rows(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    schema: &str,
    table: &str,
    key_cols: &[String],
    plan: &DbWriteBackPlan,
    info: &MergeInfo,
) -> anyhow::Result<HashMap<String, Vec<CellValue>>> {
    let key_idx: Vec<usize> = key_cols
        .iter()
        .map(|k| {
            info.base_columns
                .iter()
                .position(|c| c == k)
                .with_context(|| format!("key column '{k}' not found in the table"))
        })
        .collect::<anyhow::Result<_>>()?;
    let keys: Vec<Vec<CellValue>> = plan
        .updates
        .iter()
        .map(|(k, _)| k.clone())
        .chain(plan.deletes.iter().cloned())
        .collect();
    // `query` stops at the load cap; a batch above it would cut rows off and
    // make them look deleted on the server.
    let batch_len = CONFLICT_READ_BATCH.min(crate::formats::initial_load_rows());
    let mut server = HashMap::new();
    for batch in keys.chunks(batch_len) {
        let sql = reread_sql(engine, schema, table, &info.base_columns, key_cols, batch);
        let rows = connector
            .query(&sql)
            .context("checking for changes on the server")?;
        // A short answer at the cap means the cap was lowered mid-save
        // (Settings, Load all rows ending): the missing rows are not gone.
        let got = rows.rows.len();
        if got < batch.len() && got >= crate::formats::initial_load_rows() {
            bail!(
                "the row limit changed during the save, so the check for changes on the server \
                 read only {got} of {} rows; save again",
                batch.len()
            );
        }
        for row in rows.rows {
            let key: Vec<CellValue> = key_idx
                .iter()
                .map(|&i| row.get(i).cloned().unwrap_or(CellValue::Null))
                .collect();
            server.insert(key_text(&key), row);
        }
    }
    Ok(server)
}

fn delete_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    pk_cols: &[String],
    pk_vals: &[CellValue],
) -> String {
    format!(
        "DELETE FROM {} WHERE {}",
        target_sql(engine, schema, table),
        pk_where_sql(engine, pk_cols, pk_vals)
    )
}

fn update_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    pk_cols: &[String],
    pk_vals: &[CellValue],
    row: &[CellValue],
) -> String {
    let assignments: Vec<String> = columns
        .iter()
        .zip(row)
        .map(|(c, v)| {
            format!(
                "{} = {}",
                engine.quote_ident(&c.name),
                sql_literal(engine, v)
            )
        })
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {}",
        target_sql(engine, schema, table),
        assignments.join(", "),
        pk_where_sql(engine, pk_cols, pk_vals)
    )
}

fn insert_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    row: &[CellValue],
) -> String {
    let col_list = columns
        .iter()
        .map(|c| engine.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let values = row
        .iter()
        .map(|v| sql_literal(engine, v))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({col_list}) VALUES ({values})",
        target_sql(engine, schema, table)
    )
}

fn alter_add_sql(engine: DbEngine, schema: &str, table: &str, col: &ColumnInfo) -> String {
    use crate::data::schema_export::sql::column_type_sql;
    let dialect = super::live_dialect_for(engine);
    // SQL Server and Oracle take `ADD <col>` (Oracle rejects the COLUMN
    // keyword outright), the others `ADD COLUMN <col>` (MySQL and Postgres
    // both also accept the bare ADD, but COLUMN reads clearer).
    let add = match engine {
        DbEngine::Mssql | DbEngine::Oracle => "ADD",
        _ => "ADD COLUMN",
    };
    format!(
        "ALTER TABLE {} {add} {} {}",
        target_sql(engine, schema, table),
        engine.quote_ident(&col.name),
        column_type_sql(dialect, &col.data_type)
    )
}

/// Outcome of [`plan_from_comparison`].
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub plan: DbWriteBackPlan,
    /// Columns in the desired table that the server table does not have. They
    /// are reported, never added: a script that silently ALTERs a production
    /// table is not what a reviewer asked for.
    pub ignored_columns: Vec<String>,
}

/// Whether two cells differ in a way worth writing.
///
/// [`crate::data::compare`] compares cells as text, which is right for a diff
/// report and wrong for generating SQL: a CSV `120.50` parses to `Float(120.5)`
/// while Postgres returns the exact `120.50`, so an unchanged row would be
/// reported as changed and the script would carry an UPDATE that changes
/// nothing. A reviewer who sees phantom statements stops trusting the script,
/// so numbers are compared as numbers and everything else as text.
fn cells_differ(a: &CellValue, b: &CellValue) -> bool {
    let (sa, sb) = (a.to_string(), b.to_string());
    if sa == sb {
        return false;
    }
    match (sa.trim().parse::<f64>(), sb.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x != y,
        _ => true,
    }
}

/// Turn a key-matched comparison of a server table against a desired table
/// into a write-back plan: rows only on the server become deletes, rows only
/// in the desired table become inserts, and matched rows that really differ
/// become full-row updates addressed by the key columns.
///
/// Shared by the CLI's `--sync-sql` and the MCP `sync_sql` tool, so the two
/// cannot disagree. `added_columns` is always empty by design (see
/// [`SyncPlan::ignored_columns`]).
pub fn plan_from_comparison(
    server: &DataTable,
    desired: &DataTable,
    result: &crate::data::compare::CompareResult,
    keys: &[String],
) -> anyhow::Result<SyncPlan> {
    let key_idx: Vec<usize> = keys
        .iter()
        .map(|k| {
            server
                .columns
                .iter()
                .position(|c| &c.name == k)
                .ok_or_else(|| anyhow::anyhow!("key column '{k}' is not in the server table"))
        })
        .collect::<anyhow::Result<_>>()?;

    let key_values = |t: &DataTable, row: usize| -> Vec<CellValue> {
        key_idx
            .iter()
            .map(|&c| t.get(row, c).cloned().unwrap_or(CellValue::Null))
            .collect()
    };
    let full_row = |t: &DataTable, row: usize| -> Vec<CellValue> {
        (0..t.col_count())
            .map(|c| t.get(row, c).cloned().unwrap_or(CellValue::Null))
            .collect()
    };

    let ignored_columns: Vec<String> = desired
        .columns
        .iter()
        .filter(|c| !server.columns.iter().any(|sc| sc.name == c.name))
        .map(|c| c.name.clone())
        .collect();

    let updates = result
        .changed
        .iter()
        .filter(|ch| {
            // `changed_columns` is text-based; re-check each named column
            // numerically so formatting alone does not produce an UPDATE.
            ch.changed_columns.iter().any(|name| {
                let si = server.columns.iter().position(|c| &c.name == name);
                let di = desired.columns.iter().position(|c| &c.name == name);
                match (si, di) {
                    (Some(si), Some(di)) => {
                        let sv = server.get(ch.row_a, si).cloned().unwrap_or(CellValue::Null);
                        let dv = desired
                            .get(ch.row_b, di)
                            .cloned()
                            .unwrap_or(CellValue::Null);
                        cells_differ(&sv, &dv)
                    }
                    // A column only one side has is a real difference.
                    _ => true,
                }
            })
        })
        .map(|ch| (key_values(server, ch.row_a), full_row(desired, ch.row_b)))
        .collect();

    Ok(SyncPlan {
        plan: DbWriteBackPlan {
            deletes: result
                .only_in_a
                .iter()
                .map(|&row| key_values(server, row))
                .collect(),
            updates,
            inserts: result
                .only_in_b
                .iter()
                .map(|&row| full_row(desired, row))
                .collect(),
            added_columns: Vec::new(),
            merge: None,
        },
        ignored_columns,
    })
}

/// Render the plan as a reviewable SQL script instead of executing it.
///
/// The statements, their order and their escaping are the same ones
/// [`apply_write_back`] runs, because both go through the private builders in
/// this file. A reviewer approving this text is approving exactly what Confirm
/// would have done.
pub fn render_plan_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    pk_cols: &[String],
    plan: &DbWriteBackPlan,
) -> String {
    if plan.is_empty() {
        return format!(
            "-- no changes to write back to {}\n",
            target_sql(engine, schema, table)
        );
    }

    let begin = match engine {
        DbEngine::Mssql => "BEGIN TRANSACTION",
        _ => "BEGIN",
    };
    let mut out = String::new();
    out.push_str(&format!("{begin};\n"));

    for col in &plan.added_columns {
        out.push_str(&alter_add_sql(engine, schema, table, col));
        out.push_str(";\n");
    }
    for pk_vals in &plan.deletes {
        out.push_str(&delete_sql(engine, schema, table, pk_cols, pk_vals));
        out.push_str(";\n");
    }
    for (pk_vals, row) in &plan.updates {
        out.push_str(&update_sql(
            engine, schema, table, columns, pk_cols, pk_vals, row,
        ));
        out.push_str(";\n");
    }
    for row in &plan.inserts {
        out.push_str(&insert_sql(engine, schema, table, columns, row));
        out.push_str(";\n");
    }
    out.push_str("COMMIT;\n");
    out
}

/// A ready-to-paste SQL script built from the edit audit trail, one `UPDATE`
/// per edited row (unlike [`render_plan_sql`], this never touches inserts or
/// deletes - the trail only ever sees cell edits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlExport {
    pub statements: Vec<String>,
    /// `true` when there was no real row key to address rows by, so the
    /// statements are matched on the first column's original value instead -
    /// a starting point for the user to edit, not something to run as-is.
    pub is_template: bool,
}

/// Turn the active tab's pending cell edits into `UPDATE` statements, reusing
/// [`update_sql`] so quoting, identifier escaping and literal formatting stay
/// dialect-correct in the one place that already gets them right.
///
/// `identity` is the row's key on a live-database tab; `None` (a file-backed
/// tab has no server key) falls back to the table's first column as a
/// stand-in, and the result is marked [`SqlExport::is_template`] so the panel
/// can say so. Either way, rows are addressed by the key columns' ORIGINAL
/// values (`DataTable::original_value`), so a key cell that was itself edited
/// is still found where the server still has it.
pub fn edits_as_update_sql(
    table: &DataTable,
    engine: DbEngine,
    schema: &str,
    table_name: &str,
    identity: Option<&RowIdentity>,
) -> anyhow::Result<SqlExport> {
    let is_template = identity.is_none();
    let key_cols: Vec<String> = match identity {
        Some(id) => id.columns().to_vec(),
        None => {
            let first = table
                .columns
                .first()
                .ok_or_else(|| anyhow::anyhow!("the table has no columns"))?;
            vec![first.name.clone()]
        }
    };
    let key_idx: Vec<usize> = key_cols
        .iter()
        .map(|k| {
            table
                .columns
                .iter()
                .position(|c| &c.name == k)
                .ok_or_else(|| anyhow::anyhow!("key column '{k}' is not in the table"))
        })
        .collect::<anyhow::Result<_>>()?;

    let entries = crate::data::edit_audit::audit_entries(table);
    let mut statements = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let row = entries[i].row;
        let mut cols = Vec::new();
        let mut vals = Vec::new();
        let mut j = i;
        while j < entries.len() && entries[j].row == row {
            let col_idx = entries[j].col;
            cols.push(table.columns[col_idx].clone());
            vals.push(
                table
                    .edits
                    .get(&(row, col_idx))
                    .cloned()
                    .unwrap_or(CellValue::Null),
            );
            j += 1;
        }
        let pk_vals: Vec<CellValue> = key_idx
            .iter()
            .map(|&idx| {
                table
                    .original_value(row, idx)
                    .cloned()
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        let sql = update_sql(
            engine, schema, table_name, &cols, &key_cols, &pk_vals, &vals,
        );
        statements.push(format!("{sql};"));
        i = j;
    }
    Ok(SqlExport {
        statements,
        is_template,
    })
}

/// Refuse a statement that did not touch exactly one row.
///
/// Only enforced for [`RowIdentity::FullRow`], where nothing but this check
/// stands between an ambiguous WHERE clause and silently rewriting several
/// rows. `0` means the row is gone or was changed by someone else since the
/// tab loaded; `2` or more means the table holds duplicate rows that the
/// baseline values cannot tell apart. Both roll the transaction back.
fn verify_one(check: bool, affected: u64, what: &str) -> anyhow::Result<()> {
    if !check || affected == 1 {
        return Ok(());
    }
    if affected == 0 {
        bail!(
            "the {what} matched no row: this table has no key, so rows are matched on all their \
             values, and it has changed on the server since the tab was loaded"
        );
    }
    bail!(
        "the {what} matched {affected} rows: this table has no key, so rows are matched on all \
         their values, and these rows are identical. Nothing was changed."
    )
}

/// Whether a save that adds columns checks the server again after the
/// `ALTER TABLE ADD`s. MySQL/MariaDB and Oracle commit on DDL, ending the
/// transaction and its locks; Exasol has no transaction here at all (its
/// connector swallows `BEGIN`), so its one plain read would be as old as the
/// ALTERs are slow. Postgres, SQL Server and Redshift run the ALTER inside
/// the transaction, under the locks already held.
fn rechecks_after_alter(engine: DbEngine) -> bool {
    matches!(
        engine,
        DbEngine::MySql | DbEngine::Oracle | DbEngine::Exasol
    )
}

/// The conflict check: re-read the rows a keyed plan touches (under lock
/// where the engine can) and merge them, so nobody's change since the load
/// is overwritten. `None` for a plan that is not checked: `FullRow` (each
/// statement verifies its own match) and a sync plan (`merge: None`, it
/// already compared against the server). `columns_added` is passed through to
/// [`WriteConflicts`].
fn check_against_server(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    schema: &str,
    table: &str,
    identity: &RowIdentity,
    plan: &DbWriteBackPlan,
    columns_added: bool,
) -> anyhow::Result<Option<MergeOutcome>> {
    let (RowIdentity::Key(key_cols), Some(info)) = (identity, &plan.merge) else {
        return Ok(None);
    };
    let server = read_server_rows(connector, engine, schema, table, key_cols, plan, info)?;
    let out = merge_plan(plan, info, &server);
    if !out.conflicts.is_empty() {
        return Err(WriteConflicts {
            conflicts: out.conflicts,
            columns_added,
        }
        .into());
    }
    Ok(Some(out))
}

/// Apply the plan in ONE transaction: re-read and merge the keyed rows it
/// touches (see [`WriteConflicts`]), then `ALTER TABLE ADD` per new column,
/// DELETE by key, full-row UPDATE by key, INSERT new rows. Rolls back on any
/// error (mirrors `write_table_generic`'s transaction skeleton). The caller
/// has already checked `ensure_write_allowed`.
pub fn apply_write_back(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[ColumnInfo],
    identity: &RowIdentity,
    plan: &DbWriteBackPlan,
) -> anyhow::Result<DbWriteBackReport> {
    let pk_cols = identity.columns();
    let check = identity.needs_affected_row_check();
    let written: Vec<&[CellValue]> = plan
        .updates
        .iter()
        .map(|(_, row)| row.as_slice())
        .chain(plan.inserts.iter().map(Vec::as_slice))
        .collect();
    connector.check_rows_writable(columns, &written)?;
    let begin = match engine {
        DbEngine::Mssql => "BEGIN TRANSACTION",
        _ => "BEGIN",
    };
    connector.execute(begin).context("starting transaction")?;
    let result = (|| -> anyhow::Result<DbWriteBackReport> {
        // Checked before the ALTERs too, so a conflict leaves nothing written.
        let mut checked =
            check_against_server(connector, engine, schema, table, identity, plan, false)?;
        for col in &plan.added_columns {
            connector
                .execute(&alter_add_sql(engine, schema, table, col))
                .with_context(|| format!("adding column '{}'", col.name))?;
        }
        // The ALTER committed the transaction and released the locks on these
        // engines (or there was none): start a new one and check again, so the
        // DML stays atomic and is built from rows read after the ALTER. A
        // conflict found only here leaves the added columns in place.
        if !plan.added_columns.is_empty() && rechecks_after_alter(engine) {
            connector.execute(begin).context("starting transaction")?;
            checked = check_against_server(connector, engine, schema, table, identity, plan, true)?;
        }
        let (deletes, updates) = match &checked {
            Some(out) => (&out.deletes, &out.updates),
            None => (&plan.deletes, &plan.updates),
        };
        for pk_vals in deletes {
            let n = connector
                .execute(&delete_sql(engine, schema, table, pk_cols, pk_vals))
                .context("deleting a row")?;
            verify_one(check, n, "delete")?;
        }
        for (pk_vals, row) in updates {
            let n = connector
                .execute(&update_sql(
                    engine, schema, table, columns, pk_cols, pk_vals, row,
                ))
                .context("updating a row")?;
            verify_one(check, n, "update")?;
        }
        for row in &plan.inserts {
            connector
                .execute(&insert_sql(engine, schema, table, columns, row))
                .context("inserting a row")?;
        }
        // Count what was really written: the merge can drop rows.
        let (deleted, updated) = (deletes.len(), updates.len());
        let MergeOutcome { merged, gone, .. } = checked.unwrap_or_default();
        Ok(DbWriteBackReport {
            deleted,
            updated,
            inserted: plan.inserts.len(),
            added_columns: plan.added_columns.len(),
            merged,
            gone,
        })
    })();
    match result {
        Ok(report) => {
            connector.execute("COMMIT").context("committing")?;
            Ok(report)
        }
        Err(e) => {
            let _ = connector.execute("ROLLBACK");
            Err(e)
        }
    }
}

#[cfg(test)]
#[path = "write_back_tests.rs"]
mod tests;
