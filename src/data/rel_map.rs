//! How do these tables relate to each other?
//!
//! The ranked column pairs already come from
//! [`join_keys::suggest_keys`](crate::data::join_keys::suggest_keys); this adds
//! the two things a picture needs and a ranked list does not: the tables as
//! nodes, and an **orphan count** per relationship.
//!
//! The orphan count is what separates two candidates that value overlap alone
//! cannot tell apart. When two tables both number their rows from 1, every
//! customer id also exists among the order ids, so `order_id -> id` and
//! `customer_id -> id` both score a perfect 1.00. Counting the left values with
//! no partner on the right breaks that tie: the real key leaves none, the
//! coincidence leaves nearly every row.
//!
//! It is computed from the **same sampled value sets** `suggest_keys` scores
//! with, so the two numbers on one edge can never contradict each other.
//! Layout is not this module's business: it returns the graph.

use std::path::Path;

use crate::data::DataTable;
use crate::data::join_keys::{DEFAULT_SAMPLE_ROWS, column_values, score_pair, suggest_keys};
use crate::formats::FormatRegistry;

/// How to build the map.
#[derive(Debug, Clone, PartialEq)]
pub struct RelMapOptions {
    /// Rows read per table before scoring.
    pub sample: usize,
    /// Candidates below this score are not drawn. Higher than the ranked
    /// list's own noise floor: a picture with every weak pair in it is not a
    /// picture.
    pub min_score: f64,
}

impl Default for RelMapOptions {
    fn default() -> Self {
        Self {
            sample: DEFAULT_SAMPLE_ROWS,
            min_score: 0.5,
        }
    }
}

/// One table in the map.
#[derive(Debug, Clone, PartialEq)]
pub struct TableNode {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: usize,
}

/// One relationship between two columns. `left_*` indices address
/// [`RelMap::nodes`].
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    pub left_table: usize,
    pub left_col: usize,
    pub right_table: usize,
    pub right_col: usize,
    pub score: f64,
    pub overlap: f64,
    /// Distinct values over values sampled, per side, copied from the ranking.
    pub left_distinct: f64,
    pub right_distinct: f64,
    /// Distinct values with no partner on the other side, **both ways round**,
    /// each over the distinct values seen on that side.
    ///
    /// Both directions are carried because only one of them breaks a tie and
    /// nothing here knows which: two candidates score identically exactly when
    /// both tables number their rows from 1, and then the useful count is the
    /// one read from the child side. Reporting only the left would leave that
    /// to the accident of which table came first.
    pub left_orphans: usize,
    pub left_distinct_values: usize,
    pub right_orphans: usize,
    pub right_distinct_values: usize,
    /// The constraint name, when this edge is a foreign key the database
    /// declares rather than a pairing guessed from values. `None` for a
    /// value-sampled candidate.
    pub constraint: Option<String>,
    /// Whether the numbers above mean anything. A declared foreign key read
    /// out of a catalog arrives unscored: no rows were read, so there is
    /// nothing to count. [`score_edges`] fills them in on request.
    pub scored: bool,
}

impl Relationship {
    /// Distinct left values that did find a partner.
    pub fn matched(&self) -> usize {
        self.left_distinct_values.saturating_sub(self.left_orphans)
    }

    /// Distinct right values that did find a partner.
    pub fn right_matched(&self) -> usize {
        self.right_distinct_values
            .saturating_sub(self.right_orphans)
    }
}

/// The graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RelMap {
    pub nodes: Vec<TableNode>,
    pub edges: Vec<Relationship>,
}

/// Build the map for N named tables, best relationship first.
pub fn build_map(tables: &[(String, &DataTable)], opts: &RelMapOptions) -> RelMap {
    let nodes: Vec<TableNode> = tables
        .iter()
        .map(|(name, t)| TableNode {
            name: name.clone(),
            columns: t.columns.iter().map(|c| c.name.clone()).collect(),
            rows: t.row_count(),
        })
        .collect();

    let refs: Vec<&DataTable> = tables.iter().map(|(_, t)| *t).collect();
    let edges = suggest_keys(&refs, opts.sample)
        .into_iter()
        .filter(|k| k.score >= opts.min_score)
        .map(|k| Relationship {
            left_table: k.left.0,
            left_col: k.left.1,
            right_table: k.right.0,
            right_col: k.right.1,
            // Every number is copied off the candidate, never recomputed. The
            // ranking already measured this pair from the same sample with the
            // same scorer, so re-reading the columns here could only produce a
            // second answer to disagree with.
            score: k.score,
            overlap: k.overlap,
            left_distinct: k.left_distinct,
            right_distinct: k.right_distinct,
            left_orphans: k.left_orphans,
            left_distinct_values: k.left_values,
            right_orphans: k.right_orphans,
            right_distinct_values: k.right_values,
            constraint: None,
            scored: true,
        })
        .collect();

    RelMap { nodes, edges }
}

/// Fill in the numbers on a map whose edges arrived without them.
///
/// A declared foreign key says two columns are linked; it does not say how
/// much of the child actually points at a parent that exists. On a server that
/// enforces the constraint the answer is always "all of it", but Redshift,
/// Snowflake and BigQuery accept a declaration and enforce nothing, so the
/// question is real there.
///
/// `tables` is index-parallel to `map.nodes`. Each edge is scored **in its own
/// direction**, child to parent, rather than copying a number out of
/// [`suggest_keys`]: `left_orphans` means the opposite thing if the pair comes
/// back the other way round, and a foreign key has a child side that matters.
/// An edge whose table is missing from `tables` is left as it was.
pub fn score_edges(tables: &[(String, DataTable)], map: &mut RelMap, sample: usize) {
    for e in &mut map.edges {
        let (Some((_, left)), Some((_, right))) =
            (tables.get(e.left_table), tables.get(e.right_table))
        else {
            continue;
        };
        let (left_set, left_seen) = column_values(left, e.left_col, sample);
        let (right_set, right_seen) = column_values(right, e.right_col, sample);
        e.left_distinct_values = left_set.len();
        e.right_distinct_values = right_set.len();
        match score_pair(&left_set, left_seen, &right_set, right_seen) {
            Some(p) => {
                e.overlap = p.overlap;
                e.left_distinct = p.left_distinct;
                e.right_distinct = p.right_distinct;
                e.score = p.score;
                e.left_orphans = p.left_orphans;
                e.right_orphans = p.right_orphans;
            }
            // No shared value at all. That is an answer, not a failure: a
            // declared key whose child points nowhere is exactly what this
            // pass exists to surface.
            None => {
                e.overlap = 0.0;
                e.left_distinct = 0.0;
                e.right_distinct = 0.0;
                e.score = 0.0;
                e.left_orphans = left_set.len();
                e.right_orphans = right_set.len();
            }
        }
        e.scored = true;
    }
}

/// Files read by one folder scan before it stops. A relationship map of two
/// hundred boxes is not a map, and reading values is the expensive half.
pub const DEFAULT_MAX_FILES: usize = 30;

/// Depth limit for a recursive scan, matching the schema-drift walk.
const MAX_DEPTH: usize = 8;

/// What a folder scan found.
pub struct CollectedTables {
    /// Label and table, in the order they were read.
    pub tables: Vec<(String, DataTable)>,
    /// Files that could not be read, with the reason. One bad file in a
    /// folder must not lose the rest.
    pub skipped: Vec<(String, String)>,
    /// The scan stopped at `max_files` with more still to read.
    pub truncated: bool,
}

/// Read the tables under `dir` so they can be mapped.
///
/// Deliberately not `schema_drift::collect_schemas`: that reads column names
/// only, and relationships are decided by values.
///
/// **Ceiling:** each file is read through the normal reader, so the streaming
/// row cap applies and the rows are then truncated to `max_rows`. A folder of
/// very large files is therefore read before it is sampled.
/// Everything the recursive walk needs, bundled so it stays a three-argument
/// function rather than an eight-argument one.
struct ScanOpts<'a> {
    recursive: bool,
    max_files: usize,
    max_rows: usize,
    registry: &'a FormatRegistry,
    cancel: &'a dyn Fn() -> bool,
}

/// Read the tables under `dir` so they can be mapped.
///
/// Deliberately not `schema_drift::collect_schemas`: that reads column names
/// only, and relationships are decided by values.
///
/// **Ceiling:** each file is read through the normal reader, so the streaming
/// row cap applies and the rows are only then truncated to `max_rows`. A folder
/// of very large files is therefore read before it is sampled.
pub fn collect_tables(
    dir: &Path,
    recursive: bool,
    max_files: usize,
    max_rows: usize,
    cancel: &dyn Fn() -> bool,
) -> CollectedTables {
    let registry = FormatRegistry::new();
    let opts = ScanOpts {
        recursive,
        max_files,
        max_rows,
        registry: &registry,
        cancel,
    };
    let mut out = CollectedTables {
        tables: Vec::new(),
        skipped: Vec::new(),
        truncated: false,
    };
    walk(dir, 0, &opts, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, opts: &ScanOpts, out: &mut CollectedTables) {
    let Ok(entries) = crate::ui::directory_tree::read_sorted_dir(dir) else {
        out.skipped.push((
            dir.display().to_string(),
            "unreadable directory".to_string(),
        ));
        return;
    };
    for path in entries {
        if (opts.cancel)() {
            return;
        }
        if out.tables.len() >= opts.max_files {
            out.truncated = true;
            return;
        }
        if path.is_dir() {
            if opts.recursive && depth + 1 < MAX_DEPTH {
                walk(&path, depth + 1, opts, out);
            }
            continue;
        }
        // A recursive scan can meet the same file name in two folders, so the
        // label carries the whole path there.
        let label = if opts.recursive {
            path.display().to_string()
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        };
        if opts.registry.reader_for_path(&path).is_none() {
            out.skipped
                .push((label, "no reader for this extension".to_string()));
            continue;
        }
        match crate::formats::read_table_auto(
            &path,
            None,
            crate::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
        ) {
            Ok(mut t) => {
                t.rows.truncate(opts.max_rows);
                out.tables.push((label, t));
            }
            Err(e) => out.skipped.push((label, e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn tbl(col: &str, vals: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: col.into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vals
            .iter()
            .map(|v| vec![CellValue::String((*v).into())])
            .collect();
        t
    }

    #[test]
    fn finds_a_perfect_key_with_no_orphans() {
        let a = tbl("customer_id", &["1", "2", "3"]);
        let b = tbl("id", &["1", "2", "3"]);
        let map = build_map(
            &[("orders".into(), &a), ("customers".into(), &b)],
            &RelMapOptions::default(),
        );
        assert_eq!(map.nodes.len(), 2);
        let edge = map.edges.first().expect("no relationship found");
        assert_eq!(edge.left_orphans, 0);
        assert!(edge.overlap > 0.99);
    }

    #[test]
    fn counts_orphans_on_the_left() {
        let a = tbl("customer_id", &["1", "2", "9"]);
        let b = tbl("id", &["1", "2", "3"]);
        let map = build_map(
            &[("orders".into(), &a), ("customers".into(), &b)],
            &RelMapOptions::default(),
        );
        let edge = map
            .edges
            .iter()
            .find(|e| e.left_table == 0 && e.right_table == 1)
            .expect("no orders -> customers relationship");
        assert_eq!(edge.left_orphans, 1, "value 9 has no partner");
        assert_eq!(edge.left_distinct_values, 3);
        assert_eq!(edge.matched(), 2);
    }

    #[test]
    fn unrelated_columns_produce_no_edge() {
        let a = tbl("colour", &["red", "green"]);
        let b = tbl("id", &["1", "2"]);
        let map = build_map(
            &[("a".into(), &a), ("b".into(), &b)],
            &RelMapOptions::default(),
        );
        assert!(map.edges.is_empty(), "got {:?}", map.edges);
    }

    #[test]
    fn nodes_carry_names_columns_and_row_counts() {
        let a = tbl("x", &["1", "2"]);
        let map = build_map(&[("only".into(), &a)], &RelMapOptions::default());
        assert_eq!(map.nodes[0].name, "only");
        assert_eq!(map.nodes[0].columns, vec!["x".to_string()]);
        assert_eq!(map.nodes[0].rows, 2);
        assert!(map.edges.is_empty(), "a single table has no pairs");
    }
}
