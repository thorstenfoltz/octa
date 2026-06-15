//! Find *near*-duplicate rows: rows that are almost the same on the chosen
//! columns (typos, spacing, reordered words). Sibling to [`duplicates`] (which
//! only catches exact matches).
//!
//! Pure and testable (no UI). The GUI dialog and the MCP `fuzzy_duplicates`
//! tool both call [`find_fuzzy_duplicates`]. Three hand-rolled similarity
//! methods (no `strsim` dependency), per-column averaging, optional exact-match
//! blocking, a row cap, transitive (union-find) clustering, and cooperative
//! cancellation.
//!
//! [`duplicates`]: crate::data::duplicates

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::data::DataTable;

/// How two cell strings are scored for similarity (0.0..=1.0).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityMethod {
    /// Normalised Levenshtein: `1 - edits / max_len`. Good for typos.
    #[default]
    EditRatio,
    /// Jaro-Winkler: prefix-weighted; strong on names.
    JaroWinkler,
    /// Jaccard over whitespace-split word sets; word order / punctuation
    /// matter less.
    TokenSet,
}

/// Pre-comparison normalisation toggles (all on by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NormalizeOpts {
    /// Lowercase.
    pub lower: bool,
    /// Trim and collapse internal whitespace runs to a single space.
    pub collapse_ws: bool,
    /// Remove punctuation.
    pub strip_punct: bool,
}

impl Default for NormalizeOpts {
    fn default() -> Self {
        Self {
            lower: true,
            collapse_ws: true,
            strip_punct: true,
        }
    }
}

/// Configuration for one fuzzy-duplicate scan.
#[derive(Debug, Clone)]
pub struct FuzzyDupConfig {
    /// Columns whose values are compared (averaged). Out-of-range skipped.
    pub key_cols: Vec<usize>,
    pub method: SimilarityMethod,
    /// Match threshold, 0.0..=1.0. A pair matches when its average similarity
    /// across `key_cols` is `>= threshold`.
    pub threshold: f64,
    pub normalize: NormalizeOpts,
    /// Optional exact-match blocking key: only rows sharing this column's
    /// un-normalised value are compared, which makes large tables feasible.
    pub block_col: Option<usize>,
    /// Only the first `max_rows` rows are considered.
    pub max_rows: usize,
}

impl Default for FuzzyDupConfig {
    fn default() -> Self {
        Self {
            key_cols: Vec::new(),
            method: SimilarityMethod::EditRatio,
            threshold: 0.85,
            normalize: NormalizeOpts::default(),
            block_col: None,
            max_rows: 20_000,
        }
    }
}

/// One cluster of mutually-near rows. `score` is the **lowest** linking
/// similarity within the cluster (honest worst case).
#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyCluster {
    pub rows: Vec<usize>,
    pub score: f64,
}

/// Result of a scan.
#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyResult {
    pub clusters: Vec<FuzzyCluster>,
    /// How many rows were actually examined (after the cap).
    pub compared_rows: usize,
    /// True when the table held more than `max_rows` rows.
    pub capped: bool,
}

/// Normalise one cell string per `opts`.
pub fn normalize(s: &str, opts: &NormalizeOpts) -> String {
    let mut out = s.to_string();
    if opts.lower {
        out = out.to_lowercase();
    }
    if opts.strip_punct {
        out = out
            .chars()
            .map(|c| if c.is_ascii_punctuation() { ' ' } else { c })
            .collect();
    }
    if opts.collapse_ws {
        out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    out
}

/// Levenshtein edit distance (two-row DP) over chars.
fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// `1 - lev / max_len`. Two empty strings count as identical (1.0).
fn edit_ratio(a: &str, b: &str) -> f64 {
    let ca: Vec<char> = a.chars().collect();
    let cb: Vec<char> = b.chars().collect();
    let max_len = ca.len().max(cb.len());
    if max_len == 0 {
        return 1.0;
    }
    let d = levenshtein(&ca, &cb);
    1.0 - (d as f64 / max_len as f64)
}

/// Jaro-Winkler similarity.
fn jaro_winkler(a: &str, b: &str) -> f64 {
    let ca: Vec<char> = a.chars().collect();
    let cb: Vec<char> = b.chars().collect();
    let jaro = jaro(&ca, &cb);
    // Winkler boost for a common prefix (up to 4 chars), p = 0.1.
    let prefix = ca
        .iter()
        .zip(cb.iter())
        .take(4)
        .take_while(|(x, y)| x == y)
        .count();
    jaro + (prefix as f64) * 0.1 * (1.0 - jaro)
}

fn jaro(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let match_dist = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_matched = vec![false; a.len()];
    let mut b_matched = vec![false; b.len()];
    let mut matches = 0usize;
    for (i, &ca) in a.iter().enumerate() {
        let start = i.saturating_sub(match_dist);
        let end = (i + match_dist + 1).min(b.len());
        for j in start..end {
            if !b_matched[j] && b[j] == ca {
                a_matched[i] = true;
                b_matched[j] = true;
                matches += 1;
                break;
            }
        }
    }
    if matches == 0 {
        return 0.0;
    }
    // Count transpositions.
    let mut transpositions = 0usize;
    let mut k = 0usize;
    for (i, &m) in a_matched.iter().enumerate() {
        if !m {
            continue;
        }
        while !b_matched[k] {
            k += 1;
        }
        if a[i] != b[k] {
            transpositions += 1;
        }
        k += 1;
    }
    let m = matches as f64;
    let t = (transpositions / 2) as f64;
    (m / a.len() as f64 + m / b.len() as f64 + (m - t) / m) / 3.0
}

/// Jaccard similarity of whitespace-split word sets.
fn token_set(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let sa: HashSet<&str> = a.split_whitespace().collect();
    let sb: HashSet<&str> = b.split_whitespace().collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        return 1.0;
    }
    inter as f64 / union as f64
}

/// Similarity of two already-normalised strings under `method`.
fn similarity(method: SimilarityMethod, a: &str, b: &str) -> f64 {
    match method {
        SimilarityMethod::EditRatio => edit_ratio(a, b),
        SimilarityMethod::JaroWinkler => jaro_winkler(a, b),
        SimilarityMethod::TokenSet => token_set(a, b),
    }
}

/// Disjoint-set (union-find) over row indices `0..n`.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // Path compression.
        let mut cur = x;
        while self.parent[cur] != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Find near-duplicate clusters. See the module docs. `cancel` is polled
/// between rows; on cancellation the clusters found so far are returned.
pub fn find_fuzzy_duplicates(
    table: &DataTable,
    cfg: &FuzzyDupConfig,
    cancel: &AtomicBool,
) -> FuzzyResult {
    let total = table.row_count();
    let col_count = table.col_count();
    let key_cols: Vec<usize> = cfg
        .key_cols
        .iter()
        .copied()
        .filter(|&c| c < col_count)
        .collect();

    let capped = total > cfg.max_rows;
    let compared_rows = total.min(cfg.max_rows);
    if key_cols.is_empty() || compared_rows < 2 {
        return FuzzyResult {
            clusters: Vec::new(),
            compared_rows,
            capped,
        };
    }

    // Pre-normalise every key cell once.
    let norm: Vec<Vec<String>> = (0..compared_rows)
        .map(|r| {
            key_cols
                .iter()
                .map(|&c| {
                    let raw = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                    normalize(&raw, &cfg.normalize)
                })
                .collect()
        })
        .collect();

    // Partition rows into blocks (exact un-normalised block-col value, or one
    // block holding everything).
    let mut blocks: Vec<Vec<usize>> = Vec::new();
    match cfg.block_col.filter(|&c| c < col_count) {
        Some(bc) => {
            let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
            for r in 0..compared_rows {
                let key = table.get(r, bc).map(|v| v.to_string()).unwrap_or_default();
                by_key.entry(key).or_default().push(r);
            }
            blocks.extend(by_key.into_values());
        }
        None => blocks.push((0..compared_rows).collect()),
    }

    let mut uf = UnionFind::new(compared_rows);
    // Lowest linking similarity seen for each matched (smaller<-larger) edge,
    // used to compute each cluster's worst-case score.
    let mut edge_scores: Vec<f64> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    'outer: for block in &blocks {
        for (i, &ra) in block.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break 'outer;
            }
            for &rb in &block[i + 1..] {
                let sum: f64 = norm[ra]
                    .iter()
                    .zip(norm[rb].iter())
                    .map(|(a, b)| similarity(cfg.method, a, b))
                    .sum();
                let avg = sum / key_cols.len() as f64;
                if avg >= cfg.threshold {
                    uf.union(ra, rb);
                    edges.push((ra, rb));
                    edge_scores.push(avg);
                }
            }
        }
    }

    // Derive cluster membership from every row that participated in an edge,
    // grouped by its union-find root.
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    {
        let mut seen = vec![false; compared_rows];
        for &(ra, rb) in &edges {
            for r in [ra, rb] {
                if !seen[r] {
                    seen[r] = true;
                    let root = uf.find(r);
                    members.entry(root).or_default().push(r);
                }
            }
        }
    }

    // Per-cluster worst-case score = min over its edges.
    let mut cluster_min: HashMap<usize, f64> = HashMap::new();
    for (&(ra, _), &s) in edges.iter().zip(edge_scores.iter()) {
        let root = uf.find(ra);
        let e = cluster_min.entry(root).or_insert(f64::INFINITY);
        if s < *e {
            *e = s;
        }
    }

    let mut clusters: Vec<FuzzyCluster> = members
        .into_iter()
        .map(|(root, mut rows)| {
            rows.sort_unstable();
            FuzzyCluster {
                rows,
                score: cluster_min.get(&root).copied().unwrap_or(0.0),
            }
        })
        .collect();
    // Stable, deterministic order: by first row index.
    clusters.sort_by_key(|c| c.rows.first().copied().unwrap_or(0));

    FuzzyResult {
        clusters,
        compared_rows,
        capped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn table(cols: &[&str], rows: &[&[&str]]) -> DataTable {
        let mut t = DataTable::empty();
        for c in cols {
            t.columns.push(ColumnInfo {
                name: c.to_string(),
                data_type: "Utf8".to_string(),
            });
        }
        t.rows = rows
            .iter()
            .map(|r| r.iter().map(|v| CellValue::String(v.to_string())).collect())
            .collect();
        t
    }

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn edit_ratio_scores() {
        assert_eq!(edit_ratio("abc", "abc"), 1.0);
        assert!(edit_ratio("John Smith", "Jon Smith") > 0.85);
        assert!(edit_ratio("apple", "zzzzz") < 0.3);
    }

    #[test]
    fn jaro_winkler_scores() {
        assert_eq!(jaro_winkler("abc", "abc"), 1.0);
        assert!(jaro_winkler("Martha", "Marhta") > 0.9);
        assert!(jaro_winkler("abcd", "wxyz") < 0.3);
    }

    #[test]
    fn token_set_ignores_order() {
        assert!((token_set("john smith", "smith john") - 1.0).abs() < 1e-9);
        assert!(token_set("acme inc", "acme llc") > 0.0);
        assert!(token_set("acme inc", "acme inc") == 1.0);
    }

    #[test]
    fn normalisation_aligns_punctuation_and_case() {
        let opts = NormalizeOpts::default();
        assert_eq!(normalize("ACME, Inc.", &opts), "acme inc");
        assert_eq!(normalize("  a   b ", &opts), "a b");
    }

    #[test]
    fn threshold_gates_matches() {
        let t = table(&["name"], &[&["John Smith"], &["Jon Smith"], &["Zorbax"]]);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0],
            threshold: 0.8,
            ..Default::default()
        };
        let res = find_fuzzy_duplicates(&t, &cfg, &never());
        assert_eq!(res.clusters.len(), 1);
        assert_eq!(res.clusters[0].rows, vec![0, 1]);

        // A very high threshold finds nothing.
        let cfg_hi = FuzzyDupConfig {
            key_cols: vec![0],
            threshold: 0.99,
            ..Default::default()
        };
        assert!(
            find_fuzzy_duplicates(&t, &cfg_hi, &never())
                .clusters
                .is_empty()
        );
    }

    #[test]
    fn averages_across_columns() {
        // First column identical, second column differs a lot -> average
        // drops below a high threshold so the pair does NOT match.
        let t = table(&["a", "b"], &[&["same", "apple"], &["same", "zzzzz"]]);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0, 1],
            threshold: 0.8,
            ..Default::default()
        };
        assert!(
            find_fuzzy_duplicates(&t, &cfg, &never())
                .clusters
                .is_empty()
        );
    }

    #[test]
    fn transitive_clustering() {
        // A~B and B~C (chain of single-char typos) -> one cluster of three.
        let t = table(&["n"], &[&["aaaa"], &["aaab"], &["aabb"]]);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0],
            threshold: 0.7,
            ..Default::default()
        };
        let res = find_fuzzy_duplicates(&t, &cfg, &never());
        assert_eq!(res.clusters.len(), 1);
        assert_eq!(res.clusters[0].rows, vec![0, 1, 2]);
        // Worst-case score is the lowest linking edge (aaaa~aabb = 0.5 < thr,
        // but the chain links via 0.75 edges), so score is the min edge used.
        assert!(res.clusters[0].score >= 0.7);
    }

    #[test]
    fn blocking_partitions_comparisons() {
        // Same name in two different blocks must NOT cluster together.
        let t = table(&["name", "region"], &[&["Acme", "US"], &["Acme", "EU"]]);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0],
            threshold: 0.9,
            block_col: Some(1),
            ..Default::default()
        };
        assert!(
            find_fuzzy_duplicates(&t, &cfg, &never())
                .clusters
                .is_empty()
        );
        // Without blocking they cluster.
        let cfg2 = FuzzyDupConfig {
            block_col: None,
            ..cfg
        };
        assert_eq!(find_fuzzy_duplicates(&t, &cfg2, &never()).clusters.len(), 1);
    }

    #[test]
    fn cap_sets_capped_flag() {
        let rows: Vec<[&str; 1]> = vec![["x"]; 5];
        let row_refs: Vec<&[&str]> = rows.iter().map(|r| r.as_slice()).collect();
        let t = table(&["n"], &row_refs);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0],
            max_rows: 3,
            ..Default::default()
        };
        let res = find_fuzzy_duplicates(&t, &cfg, &never());
        assert!(res.capped);
        assert_eq!(res.compared_rows, 3);
    }

    #[test]
    fn cancel_short_circuits() {
        let t = table(&["n"], &[&["aaaa"], &["aaab"]]);
        let cfg = FuzzyDupConfig {
            key_cols: vec![0],
            threshold: 0.5,
            ..Default::default()
        };
        let cancelled = AtomicBool::new(true);
        let res = find_fuzzy_duplicates(&t, &cfg, &cancelled);
        // Cancelled before any comparison -> no clusters.
        assert!(res.clusters.is_empty());
    }
}
